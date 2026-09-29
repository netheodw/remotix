use rusqlite::{Connection, OptionalExtension};

use crate::domain::{Group, NewGroupInput, UpdateGroupInput};
use crate::error::{AppError, AppResult};
use crate::util::{new_id, now_iso};
use crate::validation::validate_display_name;

/// Strategy for deleting a group that still has children.
pub enum GroupDeleteStrategy {
    /// Move direct child groups and direct items to this group's parent
    /// (or to root if it had none).
    ReassignToParent,
    /// Delete the group, every descendant group, and every item contained
    /// anywhere in the subtree.
    DeleteSubtree,
}

/// Backs both `terminal_groups` and `port_forward_groups`; which table is
/// fixed per instance via the two constructors below, never from user input.
pub struct GroupRepository {
    table: &'static str,
}

impl GroupRepository {
    pub const fn terminal_groups() -> Self {
        Self { table: "terminal_groups" }
    }

    pub const fn port_forward_groups() -> Self {
        Self { table: "port_forward_groups" }
    }

    pub fn list(&self, conn: &Connection) -> AppResult<Vec<Group>> {
        let sql = format!(
            "SELECT id, parent_id, name, sort_order, created_at, updated_at FROM {} ORDER BY sort_order, name",
            self.table
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::map_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, conn: &Connection, id: &str) -> AppResult<Group> {
        let sql = format!(
            "SELECT id, parent_id, name, sort_order, created_at, updated_at FROM {} WHERE id = ?1",
            self.table
        );
        conn.query_row(&sql, [id], Self::map_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound("group".into()))
    }

    pub fn create(&self, conn: &Connection, input: NewGroupInput) -> AppResult<Group> {
        validate_display_name(&input.name)?;
        if let Some(parent) = &input.parent_id {
            self.get(conn, parent)?;
        }
        self.assert_unique_name(conn, input.parent_id.as_deref(), &input.name, None)?;

        let id = new_id();
        let now = now_iso();
        let next_sort = self.next_sort_order(conn, input.parent_id.as_deref())?;
        let sql = format!(
            "INSERT INTO {} (id, parent_id, name, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            self.table
        );
        conn.execute(&sql, rusqlite::params![id, input.parent_id, input.name.trim(), next_sort, now, now])?;
        self.get(conn, &id)
    }

    pub fn update(&self, conn: &Connection, input: UpdateGroupInput) -> AppResult<Group> {
        validate_display_name(&input.name)?;
        self.get(conn, &input.id)?;

        if let Some(parent) = &input.parent_id {
            if parent == &input.id {
                return Err(AppError::Validation("a group cannot be its own parent".into()));
            }
            self.get(conn, parent)?;
            if self.is_descendant(conn, parent, &input.id)? {
                return Err(AppError::Validation(
                    "cannot move a group under one of its own descendants".into(),
                ));
            }
        }
        self.assert_unique_name(conn, input.parent_id.as_deref(), &input.name, Some(&input.id))?;

        let sql = format!(
            "UPDATE {} SET parent_id = ?1, name = ?2, sort_order = COALESCE(?3, sort_order), updated_at = ?4 WHERE id = ?5",
            self.table
        );
        conn.execute(
            &sql,
            rusqlite::params![input.parent_id, input.name.trim(), input.sort_order, now_iso(), input.id],
        )?;
        self.get(conn, &input.id)
    }

    /// Deletes a group. Callers pass the repository for the sibling item
    /// table (ssh_profiles / port_forwards) so items in the subtree can be
    /// reassigned or removed consistently with the group tree.
    pub fn delete(
        &self,
        conn: &Connection,
        id: &str,
        strategy: GroupDeleteStrategy,
        item_table: &str,
        item_group_column: &str,
    ) -> AppResult<()> {
        let group = self.get(conn, id)?;

        match strategy {
            GroupDeleteStrategy::ReassignToParent => {
                let child_groups_sql = format!(
                    "UPDATE {} SET parent_id = ?1 WHERE parent_id = ?2",
                    self.table
                );
                conn.execute(&child_groups_sql, rusqlite::params![group.parent_id, id])?;

                let items_sql = format!(
                    "UPDATE {item_table} SET {item_group_column} = ?1 WHERE {item_group_column} = ?2"
                );
                conn.execute(&items_sql, rusqlite::params![group.parent_id, id])?;

                let delete_sql = format!("DELETE FROM {} WHERE id = ?1", self.table);
                conn.execute(&delete_sql, [id])?;
            }
            GroupDeleteStrategy::DeleteSubtree => {
                let ids = self.collect_subtree_ids(conn, id)?;
                if !ids.is_empty() {
                    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    let delete_items_sql =
                        format!("DELETE FROM {item_table} WHERE {item_group_column} IN ({placeholders})");
                    let params: Vec<&dyn rusqlite::ToSql> =
                        ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                    conn.execute(&delete_items_sql, params.as_slice())?;
                }
                // ON DELETE CASCADE removes descendant group rows once the
                // root of the subtree is deleted.
                let delete_sql = format!("DELETE FROM {} WHERE id = ?1", self.table);
                conn.execute(&delete_sql, [id])?;
            }
        }
        Ok(())
    }

    fn collect_subtree_ids(&self, conn: &Connection, root_id: &str) -> AppResult<Vec<String>> {
        let mut all = vec![root_id.to_string()];
        let mut frontier = vec![root_id.to_string()];
        let children_sql = format!("SELECT id FROM {} WHERE parent_id = ?1", self.table);
        while let Some(parent) = frontier.pop() {
            let mut stmt = conn.prepare(&children_sql)?;
            let child_ids: Vec<String> = stmt
                .query_map([&parent], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for child in child_ids {
                frontier.push(child.clone());
                all.push(child);
            }
        }
        Ok(all)
    }

    fn is_descendant(&self, conn: &Connection, candidate: &str, of_id: &str) -> AppResult<bool> {
        Ok(self.collect_subtree_ids(conn, of_id)?.iter().any(|id| id == candidate))
    }

    fn next_sort_order(&self, conn: &Connection, parent_id: Option<&str>) -> AppResult<i64> {
        let sql = format!(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM {} WHERE parent_id IS ?1",
            self.table
        );
        Ok(conn.query_row(&sql, [parent_id], |r| r.get(0))?)
    }

    fn assert_unique_name(
        &self,
        conn: &Connection,
        parent_id: Option<&str>,
        name: &str,
        exclude_id: Option<&str>,
    ) -> AppResult<()> {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE parent_id IS ?1 AND lower(name) = lower(?2) AND id IS NOT ?3",
            self.table
        );
        let count: i64 = conn.query_row(&sql, rusqlite::params![parent_id, name.trim(), exclude_id], |r| r.get(0))?;
        if count > 0 {
            return Err(AppError::Validation(format!(
                "a group named \"{}\" already exists at this level",
                name.trim()
            )));
        }
        Ok(())
    }

    fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Group> {
        Ok(Group {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            name: row.get(2)?,
            sort_order: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
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

    #[test]
    fn create_and_list_groups() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let repo = GroupRepository::terminal_groups();
        repo.create(&conn, NewGroupInput { parent_id: None, name: "Production".into() }).unwrap();
        repo.create(&conn, NewGroupInput { parent_id: None, name: "Staging".into() }).unwrap();
        let groups = repo.list(&conn).unwrap();
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn duplicate_name_at_same_level_rejected() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let repo = GroupRepository::terminal_groups();
        repo.create(&conn, NewGroupInput { parent_id: None, name: "Production".into() }).unwrap();
        let err = repo.create(&conn, NewGroupInput { parent_id: None, name: "production".into() });
        assert!(err.is_err());
    }

    #[test]
    fn cannot_set_self_as_parent() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let repo = GroupRepository::terminal_groups();
        let g = repo.create(&conn, NewGroupInput { parent_id: None, name: "A".into() }).unwrap();
        let err = repo.update(&conn, UpdateGroupInput { id: g.id.clone(), parent_id: Some(g.id.clone()), name: "A".into(), sort_order: None });
        assert!(err.is_err());
    }

    #[test]
    fn cannot_move_group_under_its_own_descendant() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let repo = GroupRepository::terminal_groups();
        let parent = repo.create(&conn, NewGroupInput { parent_id: None, name: "Parent".into() }).unwrap();
        let child = repo
            .create(&conn, NewGroupInput { parent_id: Some(parent.id.clone()), name: "Child".into() })
            .unwrap();
        let err = repo.update(
            &conn,
            UpdateGroupInput { id: parent.id.clone(), parent_id: Some(child.id.clone()), name: "Parent".into(), sort_order: None },
        );
        assert!(err.is_err());
    }

    #[test]
    fn delete_with_reassign_moves_children_to_parent() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let repo = GroupRepository::terminal_groups();
        let root = repo.create(&conn, NewGroupInput { parent_id: None, name: "Root".into() }).unwrap();
        let mid = repo.create(&conn, NewGroupInput { parent_id: Some(root.id.clone()), name: "Mid".into() }).unwrap();
        let leaf = repo.create(&conn, NewGroupInput { parent_id: Some(mid.id.clone()), name: "Leaf".into() }).unwrap();

        repo.delete(&conn, &mid.id, GroupDeleteStrategy::ReassignToParent, "ssh_profiles", "terminal_group_id").unwrap();

        let leaf_after = repo.get(&conn, &leaf.id).unwrap();
        assert_eq!(leaf_after.parent_id, Some(root.id));
    }
}
