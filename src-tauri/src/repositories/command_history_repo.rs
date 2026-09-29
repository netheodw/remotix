use rusqlite::{Connection, OptionalExtension};

use crate::domain::CommandHistoryEntry;
use crate::error::AppResult;
use crate::util::{new_id, now_iso};

/// Newest entries kept per profile; older ones are pruned on every insert so
/// a long-lived profile cannot grow the database without bound.
const MAX_ENTRIES_PER_PROFILE: usize = 2000;

/// Anything longer than this is almost certainly a pasted blob rather than a
/// command worth replaying, so it is dropped instead of stored.
const MAX_COMMAND_LEN: usize = 4096;

pub struct CommandHistoryRepository;

impl CommandHistoryRepository {
    /// Oldest-first, capped to the newest `limit` entries - the order the
    /// terminal wants for Up/Down navigation.
    pub fn list(conn: &Connection, profile_id: &str, limit: usize) -> AppResult<Vec<CommandHistoryEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, ssh_profile_id, command, executed_at FROM (
                 SELECT id, ssh_profile_id, command, executed_at, rowid FROM command_history
                 WHERE ssh_profile_id = ?1 ORDER BY executed_at DESC, rowid DESC LIMIT ?2
             ) ORDER BY executed_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![profile_id, limit as i64], |row| {
            Ok(CommandHistoryEntry {
                id: row.get(0)?,
                ssh_profile_id: row.get(1)?,
                command: row.get(2)?,
                executed_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Stores one executed command, returning the stored entry - or `None`
    /// when the line is not worth recording. See [`normalize`] for the rules.
    pub fn record(conn: &Connection, profile_id: &str, raw: &str) -> AppResult<Option<CommandHistoryEntry>> {
        let Some(command) = normalize(raw) else {
            return Ok(None);
        };

        let previous: Option<String> = conn
            .query_row(
                "SELECT command FROM command_history WHERE ssh_profile_id = ?1 ORDER BY executed_at DESC, rowid DESC LIMIT 1",
                [profile_id],
                |row| row.get(0),
            )
            .optional()?;
        if previous.as_deref() == Some(command.as_str()) {
            return Ok(None);
        }

        let entry = CommandHistoryEntry {
            id: new_id(),
            ssh_profile_id: profile_id.to_string(),
            command,
            executed_at: now_iso(),
        };
        conn.execute(
            "INSERT INTO command_history (id, ssh_profile_id, command, executed_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![entry.id, entry.ssh_profile_id, entry.command, entry.executed_at],
        )?;
        Self::prune(conn, profile_id)?;
        Ok(Some(entry))
    }

    pub fn clear(conn: &Connection, profile_id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM command_history WHERE ssh_profile_id = ?1", [profile_id])?;
        Ok(())
    }

    fn prune(conn: &Connection, profile_id: &str) -> AppResult<()> {
        conn.execute(
            "DELETE FROM command_history WHERE ssh_profile_id = ?1 AND id NOT IN (
                 SELECT id FROM command_history WHERE ssh_profile_id = ?1
                 ORDER BY executed_at DESC, rowid DESC LIMIT ?2
             )",
            rusqlite::params![profile_id, MAX_ENTRIES_PER_PROFILE as i64],
        )?;
        Ok(())
    }
}

/// Decides whether a captured terminal line should be remembered, and in what
/// form. Mirrored by `normalizeCommand` in `src/components/terminalHistory.ts`,
/// which keeps the in-memory list the terminal navigates in sync with what
/// actually lands in the database.
///
/// A leading space or tab means "do not remember this one", matching the
/// `HISTCONTROL=ignorespace` convention shells already use.
pub fn normalize(raw: &str) -> Option<String> {
    if raw.starts_with(' ') || raw.starts_with('\t') {
        return None;
    }
    let command = raw.trim_end_matches([' ', '\t', '\r', '\n']);
    if command.is_empty() || command.contains('\n') || command.len() > MAX_COMMAND_LEN {
        return None;
    }
    Some(command.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const INSERT_PROFILE: &str = "INSERT INTO ssh_profiles \
        (id, name, hostname, port, username, authentication_kind, terminal_type, created_at, updated_at) \
        VALUES (?1, ?2, 'example.com', 22, 'root', 'password', 'xterm-256color', ?3, ?3)";

    fn pool_with_profile() -> (tempfile::TempDir, db::Pool, String) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("t.db")).unwrap();
        let profile_id = new_id();
        {
            let conn = pool.get().unwrap();
            conn.execute(INSERT_PROFILE, rusqlite::params![profile_id, "a", now_iso()])
                .unwrap();
        }
        (dir, pool, profile_id)
    }

    #[test]
    fn normalize_drops_blank_and_space_prefixed_lines() {
        assert_eq!(normalize("ls -la"), Some("ls -la".to_string()));
        assert_eq!(normalize("ls -la   \r"), Some("ls -la".to_string()));
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("   "), None);
        assert_eq!(normalize(" secret --token=abc"), None);
        assert_eq!(normalize("\techo hi"), None);
        assert_eq!(normalize("a\nb"), None);
        assert_eq!(normalize(&"x".repeat(MAX_COMMAND_LEN + 1)), None);
    }

    #[test]
    fn records_and_lists_oldest_first() {
        let (_dir, pool, profile) = pool_with_profile();
        let conn = pool.get().unwrap();
        CommandHistoryRepository::record(&conn, &profile, "first").unwrap();
        CommandHistoryRepository::record(&conn, &profile, "second").unwrap();
        let list = CommandHistoryRepository::list(&conn, &profile, 100).unwrap();
        let commands: Vec<_> = list.iter().map(|e| e.command.as_str()).collect();
        assert_eq!(commands, vec!["first", "second"]);
    }

    #[test]
    fn consecutive_duplicates_collapse_but_later_repeats_are_kept() {
        let (_dir, pool, profile) = pool_with_profile();
        let conn = pool.get().unwrap();
        CommandHistoryRepository::record(&conn, &profile, "ls").unwrap();
        assert!(CommandHistoryRepository::record(&conn, &profile, "ls").unwrap().is_none());
        CommandHistoryRepository::record(&conn, &profile, "pwd").unwrap();
        assert!(CommandHistoryRepository::record(&conn, &profile, "ls").unwrap().is_some());
        let list = CommandHistoryRepository::list(&conn, &profile, 100).unwrap();
        let commands: Vec<_> = list.iter().map(|e| e.command.as_str()).collect();
        assert_eq!(commands, vec!["ls", "pwd", "ls"]);
    }

    #[test]
    fn list_returns_the_newest_entries_when_limited() {
        let (_dir, pool, profile) = pool_with_profile();
        let conn = pool.get().unwrap();
        for i in 0..10 {
            CommandHistoryRepository::record(&conn, &profile, &format!("cmd{i}")).unwrap();
        }
        let list = CommandHistoryRepository::list(&conn, &profile, 3).unwrap();
        let commands: Vec<_> = list.iter().map(|e| e.command.as_str()).collect();
        assert_eq!(commands, vec!["cmd7", "cmd8", "cmd9"]);
    }

    #[test]
    fn history_is_scoped_per_profile_and_clearable() {
        let (_dir, pool, profile_a) = pool_with_profile();
        let conn = pool.get().unwrap();
        let profile_b = new_id();
        conn.execute(INSERT_PROFILE, rusqlite::params![profile_b, "b", now_iso()])
            .unwrap();

        CommandHistoryRepository::record(&conn, &profile_a, "on-a").unwrap();
        CommandHistoryRepository::record(&conn, &profile_b, "on-b").unwrap();
        assert_eq!(CommandHistoryRepository::list(&conn, &profile_a, 100).unwrap().len(), 1);

        CommandHistoryRepository::clear(&conn, &profile_a).unwrap();
        assert!(CommandHistoryRepository::list(&conn, &profile_a, 100).unwrap().is_empty());
        assert_eq!(CommandHistoryRepository::list(&conn, &profile_b, 100).unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_profile_removes_its_history() {
        let (_dir, pool, profile) = pool_with_profile();
        let conn = pool.get().unwrap();
        CommandHistoryRepository::record(&conn, &profile, "ls").unwrap();
        conn.execute("DELETE FROM ssh_profiles WHERE id = ?1", [&profile]).unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM command_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
