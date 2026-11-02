use rusqlite::{Connection, OptionalExtension};

use crate::domain::{Snippet, SnippetCategory, SnippetInput};
use crate::error::{AppError, AppResult};
use crate::util::{new_id, now_iso};
use crate::validation::validate_display_name;

const MAX_COMMAND_LEN: usize = 8192;
const MAX_DESCRIPTION_LEN: usize = 1000;

pub struct SnippetRepository;

impl SnippetRepository {
    pub fn list_categories(conn: &Connection) -> AppResult<Vec<SnippetCategory>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, sort_order, created_at, updated_at FROM snippet_categories ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([], Self::map_category)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_category(conn: &Connection, id: &str) -> AppResult<SnippetCategory> {
        conn.query_row(
            "SELECT id, name, sort_order, created_at, updated_at FROM snippet_categories WHERE id = ?1",
            [id],
            Self::map_category,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("snippet category".into()))
    }

    pub fn create_category(conn: &Connection, name: &str) -> AppResult<SnippetCategory> {
        validate_display_name(name)?;
        Self::assert_unique_category(conn, name, None)?;
        let id = new_id();
        let now = now_iso();
        let next_sort: i64 =
            conn.query_row("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM snippet_categories", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO snippet_categories (id, name, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, name.trim(), next_sort, now, now],
        )?;
        Self::get_category(conn, &id)
    }

    pub fn rename_category(conn: &Connection, id: &str, name: &str) -> AppResult<SnippetCategory> {
        validate_display_name(name)?;
        Self::get_category(conn, id)?;
        Self::assert_unique_category(conn, name, Some(id))?;
        conn.execute(
            "UPDATE snippet_categories SET name = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![name.trim(), now_iso(), id],
        )?;
        Self::get_category(conn, id)
    }

    /// Deletes a category together with its snippets (ON DELETE CASCADE).
    pub fn delete_category(conn: &Connection, id: &str) -> AppResult<()> {
        Self::get_category(conn, id)?;
        conn.execute("DELETE FROM snippet_categories WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list(conn: &Connection) -> AppResult<Vec<Snippet>> {
        let mut stmt = conn.prepare(
            "SELECT id, category_id, name, command, description, sort_order, created_at, updated_at
             FROM snippets ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([], Self::map_snippet)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(conn: &Connection, id: &str) -> AppResult<Snippet> {
        conn.query_row(
            "SELECT id, category_id, name, command, description, sort_order, created_at, updated_at
             FROM snippets WHERE id = ?1",
            [id],
            Self::map_snippet,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("snippet".into()))
    }

    pub fn create(conn: &Connection, input: SnippetInput) -> AppResult<Snippet> {
        Self::validate(conn, &input)?;
        let id = new_id();
        let now = now_iso();
        let next_sort: i64 = conn.query_row("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM snippets", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO snippets (id, category_id, name, command, description, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                input.category_id,
                input.name.trim(),
                input.command.trim(),
                input.description.trim(),
                next_sort,
                now,
                now
            ],
        )?;
        Self::get(conn, &id)
    }

    pub fn update(conn: &Connection, id: &str, input: SnippetInput) -> AppResult<Snippet> {
        Self::get(conn, id)?;
        Self::validate(conn, &input)?;
        conn.execute(
            "UPDATE snippets SET category_id = ?1, name = ?2, command = ?3, description = ?4, updated_at = ?5
             WHERE id = ?6",
            rusqlite::params![
                input.category_id,
                input.name.trim(),
                input.command.trim(),
                input.description.trim(),
                now_iso(),
                id
            ],
        )?;
        Self::get(conn, id)
    }

    pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
        Self::get(conn, id)?;
        conn.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }

    fn validate(conn: &Connection, input: &SnippetInput) -> AppResult<()> {
        validate_display_name(&input.name)?;
        if input.command.trim().is_empty() {
            return Err(AppError::Validation("command is required".into()));
        }
        if input.command.len() > MAX_COMMAND_LEN {
            return Err(AppError::Validation("command is too long".into()));
        }
        if input.description.len() > MAX_DESCRIPTION_LEN {
            return Err(AppError::Validation("description is too long".into()));
        }
        Self::get_category(conn, &input.category_id)?;
        Ok(())
    }

    fn assert_unique_category(conn: &Connection, name: &str, exclude_id: Option<&str>) -> AppResult<()> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snippet_categories WHERE lower(name) = lower(?1) AND id IS NOT ?2",
            rusqlite::params![name.trim(), exclude_id],
            |r| r.get(0),
        )?;
        if count > 0 {
            return Err(AppError::Validation(format!(
                "a category named \"{}\" already exists",
                name.trim()
            )));
        }
        Ok(())
    }

    fn map_category(row: &rusqlite::Row) -> rusqlite::Result<SnippetCategory> {
        Ok(SnippetCategory {
            id: row.get(0)?,
            name: row.get(1)?,
            sort_order: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }

    fn map_snippet(row: &rusqlite::Row) -> rusqlite::Result<Snippet> {
        Ok(Snippet {
            id: row.get(0)?,
            category_id: row.get(1)?,
            name: row.get(2)?,
            command: row.get(3)?,
            description: row.get(4)?,
            sort_order: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_pool() -> (tempfile::TempDir, db::Pool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("t.db")).unwrap();
        (dir, pool)
    }

    fn input(category_id: &str, name: &str, command: &str) -> SnippetInput {
        SnippetInput {
            category_id: category_id.into(),
            name: name.into(),
            command: command.into(),
            description: String::new(),
        }
    }

    #[test]
    fn migration_seeds_default_linux_commands() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let categories = SnippetRepository::list_categories(&conn).unwrap();
        assert!(categories.iter().any(|c| c.name == "System"));
        let snippets = SnippetRepository::list(&conn).unwrap();
        assert!(snippets.iter().any(|s| s.command == "df -h"));
        assert!(snippets.iter().all(|s| categories.iter().any(|c| c.id == s.category_id)));
    }

    #[test]
    fn create_update_and_delete_snippet() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let cat = SnippetRepository::create_category(&conn, "Mine").unwrap();
        let s = SnippetRepository::create(&conn, input(&cat.id, "Hello", "echo hi")).unwrap();
        let s = SnippetRepository::update(&conn, &s.id, input(&cat.id, "Hello", "echo hello")).unwrap();
        assert_eq!(s.command, "echo hello");
        SnippetRepository::delete(&conn, &s.id).unwrap();
        assert!(SnippetRepository::get(&conn, &s.id).is_err());
    }

    #[test]
    fn rejects_blank_command_and_unknown_category() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let cat = SnippetRepository::create_category(&conn, "Mine").unwrap();
        assert!(SnippetRepository::create(&conn, input(&cat.id, "Blank", "   ")).is_err());
        assert!(SnippetRepository::create(&conn, input("missing", "X", "ls")).is_err());
    }

    #[test]
    fn duplicate_category_name_rejected() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        assert!(SnippetRepository::create_category(&conn, "docker").is_err());
    }

    #[test]
    fn deleting_category_removes_its_snippets() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let cat = SnippetRepository::create_category(&conn, "Mine").unwrap();
        let s = SnippetRepository::create(&conn, input(&cat.id, "Hello", "echo hi")).unwrap();
        SnippetRepository::delete_category(&conn, &cat.id).unwrap();
        assert!(SnippetRepository::get(&conn, &s.id).is_err());
    }
}
