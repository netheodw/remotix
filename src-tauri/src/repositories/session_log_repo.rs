use rusqlite::{Connection, Row};

use crate::domain::{SessionCommand, SshProfile, TerminalSessionSummary};
use crate::error::AppResult;
use crate::repositories::command_history_repo::normalize;
use crate::util::{new_id, now_iso};

/// Newest sessions kept; older ones (and their commands) are pruned when a
/// new session starts.
const MAX_SESSIONS: i64 = 5000;
/// Commands kept per session, so a runaway loop cannot fill the disk.
const MAX_COMMANDS_PER_SESSION: i64 = 20000;

pub struct SessionLogRepository;

impl SessionLogRepository {
    /// Records that a terminal session is starting (before connecting).
    pub fn start(conn: &Connection, session_id: &str, profile: &SshProfile) -> AppResult<()> {
        conn.execute(
            "INSERT INTO terminal_sessions (id, ssh_profile_id, profile_name, hostname, port, username, started_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'connecting')",
            rusqlite::params![
                session_id,
                profile.id,
                profile.name,
                profile.hostname,
                profile.port,
                profile.username,
                now_iso()
            ],
        )?;
        conn.execute(
            "DELETE FROM terminal_sessions WHERE id NOT IN (
                 SELECT id FROM terminal_sessions ORDER BY started_at DESC, rowid DESC LIMIT ?1
             )",
            [MAX_SESSIONS],
        )?;
        Ok(())
    }

    pub fn mark_connected(conn: &Connection, session_id: &str) -> AppResult<()> {
        conn.execute(
            "UPDATE terminal_sessions SET status = 'connected', connected_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
            rusqlite::params![now_iso(), session_id],
        )?;
        Ok(())
    }

    /// Ends a session: `failed` if it never connected, `closed` otherwise.
    /// Only the first call counts.
    pub fn finish(conn: &Connection, session_id: &str, reason: &str) -> AppResult<()> {
        conn.execute(
            "UPDATE terminal_sessions
             SET ended_at = ?1, end_reason = ?2,
                 status = CASE WHEN connected_at IS NULL THEN 'failed' ELSE 'closed' END
             WHERE id = ?3 AND ended_at IS NULL",
            rusqlite::params![now_iso(), reason, session_id],
        )?;
        Ok(())
    }

    /// At startup nothing is live, so sessions still open were cut off when
    /// the app last exited (or crashed). Their end time is unknown.
    pub fn close_dangling(conn: &Connection) -> AppResult<usize> {
        Ok(conn.execute(
            "UPDATE terminal_sessions SET status = 'interrupted', end_reason = 'app closed while connected'
             WHERE ended_at IS NULL AND status IN ('connecting', 'connected')",
            [],
        )?)
    }

    /// Stores one command run in a session. Uses the same rules as the Up/Down
    /// history (a leading space skips it), but keeps repeats: this is a log.
    pub fn record_command(conn: &Connection, session_id: &str, raw: &str) -> AppResult<Option<SessionCommand>> {
        let Some(command) = normalize(raw) else {
            return Ok(None);
        };
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_commands WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )?;
        if count >= MAX_COMMANDS_PER_SESSION {
            return Ok(None);
        }
        let entry = SessionCommand {
            id: new_id(),
            session_id: session_id.to_string(),
            command,
            executed_at: now_iso(),
        };
        let inserted = conn.execute(
            "INSERT INTO session_commands (id, session_id, command, executed_at)
             SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM terminal_sessions WHERE id = ?2)",
            rusqlite::params![entry.id, entry.session_id, entry.command, entry.executed_at],
        )?;
        Ok((inserted == 1).then_some(entry))
    }

    /// Newest first. `search` matches the profile name, host, user, or any
    /// command run in the session.
    pub fn list(
        conn: &Connection,
        profile_id: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> AppResult<Vec<TerminalSessionSummary>> {
        let pattern = search
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{}%", escape_like(s)));
        let mut stmt = conn.prepare(
            "SELECT s.id, s.ssh_profile_id, s.profile_name, s.hostname, s.port, s.username, s.started_at,
                    s.connected_at, s.ended_at, s.status, s.end_reason,
                    (SELECT COUNT(*) FROM session_commands c WHERE c.session_id = s.id)
             FROM terminal_sessions s
             WHERE (?1 IS NULL OR s.ssh_profile_id = ?1)
               AND (?2 IS NULL
                    OR s.profile_name LIKE ?2 ESCAPE '\\'
                    OR s.hostname LIKE ?2 ESCAPE '\\'
                    OR s.username LIKE ?2 ESCAPE '\\'
                    OR EXISTS (SELECT 1 FROM session_commands c WHERE c.session_id = s.id AND c.command LIKE ?2 ESCAPE '\\'))
             ORDER BY s.started_at DESC, s.rowid DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![profile_id, pattern, limit as i64], Self::map_summary)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Oldest first, the order they were run.
    pub fn commands(conn: &Connection, session_id: &str) -> AppResult<Vec<SessionCommand>> {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, command, executed_at FROM session_commands
             WHERE session_id = ?1 ORDER BY executed_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map([session_id], |r| {
            Ok(SessionCommand { id: r.get(0)?, session_id: r.get(1)?, command: r.get(2)?, executed_at: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(conn: &Connection, session_id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM terminal_sessions WHERE id = ?1", [session_id])?;
        Ok(())
    }

    /// Deletes every finished session; live ones stay so they keep logging.
    pub fn clear_finished(conn: &Connection) -> AppResult<()> {
        conn.execute("DELETE FROM terminal_sessions WHERE ended_at IS NOT NULL OR status = 'interrupted'", [])?;
        Ok(())
    }

    fn map_summary(r: &Row) -> rusqlite::Result<TerminalSessionSummary> {
        Ok(TerminalSessionSummary {
            id: r.get(0)?,
            ssh_profile_id: r.get(1)?,
            profile_name: r.get(2)?,
            hostname: r.get(3)?,
            port: r.get(4)?,
            username: r.get(5)?,
            started_at: r.get(6)?,
            connected_at: r.get(7)?,
            ended_at: r.get(8)?,
            status: r.get(9)?,
            end_reason: r.get(10)?,
            command_count: r.get(11)?,
        })
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::ProfileRepository;
    use crate::testing::{create_password_profile, open_temp_vault};

    #[test]
    fn session_lifecycle_and_commands() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();

        SessionLogRepository::start(&conn, "s1", &p).unwrap();
        SessionLogRepository::mark_connected(&conn, "s1").unwrap();
        assert!(SessionLogRepository::record_command(&conn, "s1", "ls -la").unwrap().is_some());
        assert!(SessionLogRepository::record_command(&conn, "s1", "ls -la").unwrap().is_some());
        assert!(SessionLogRepository::record_command(&conn, "s1", " secret").unwrap().is_none());
        assert!(SessionLogRepository::record_command(&conn, "nope", "ls").unwrap().is_none());
        SessionLogRepository::finish(&conn, "s1", "disconnected").unwrap();
        SessionLogRepository::finish(&conn, "s1", "later").unwrap();

        let list = SessionLogRepository::list(&conn, None, None, 50).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, "closed");
        assert_eq!(list[0].end_reason.as_deref(), Some("disconnected"));
        assert_eq!(list[0].command_count, 2);
        let cmds = SessionLogRepository::commands(&conn, "s1").unwrap();
        assert_eq!(cmds.iter().map(|c| c.command.as_str()).collect::<Vec<_>>(), ["ls -la", "ls -la"]);
    }

    #[test]
    fn failed_and_interrupted_sessions() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();
        SessionLogRepository::start(&conn, "failed", &p).unwrap();
        SessionLogRepository::finish(&conn, "failed", "auth failed").unwrap();
        SessionLogRepository::start(&conn, "cut", &p).unwrap();
        SessionLogRepository::mark_connected(&conn, "cut").unwrap();
        assert_eq!(SessionLogRepository::close_dangling(&conn).unwrap(), 1);

        let list = SessionLogRepository::list(&conn, None, None, 50).unwrap();
        let status = |id: &str| list.iter().find(|s| s.id == id).unwrap().status.clone();
        assert_eq!(status("failed"), "failed");
        assert_eq!(status("cut"), "interrupted");
    }

    #[test]
    fn search_filter_and_survives_profile_delete() {
        let fx = open_temp_vault("correct-password-1");
        let a = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let b = create_password_profile(&fx, "db", "db.internal", "admin", "pw");
        let conn = fx.pool.get().unwrap();
        SessionLogRepository::start(&conn, "a1", &a).unwrap();
        SessionLogRepository::record_command(&conn, "a1", "systemctl restart nginx").unwrap();
        SessionLogRepository::start(&conn, "b1", &b).unwrap();

        let by_cmd = SessionLogRepository::list(&conn, None, Some("nginx"), 50).unwrap();
        assert_eq!(by_cmd.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["a1"]);
        let by_host = SessionLogRepository::list(&conn, None, Some("db.int"), 50).unwrap();
        assert_eq!(by_host.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b1"]);
        let by_profile = SessionLogRepository::list(&conn, Some(&b.id), None, 50).unwrap();
        assert_eq!(by_profile.len(), 1);
        assert!(SessionLogRepository::list(&conn, None, Some("100%"), 50).unwrap().is_empty());

        ProfileRepository::delete(&conn, &a.id).unwrap();
        let after = SessionLogRepository::list(&conn, None, Some("nginx"), 50).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].ssh_profile_id, None);
        assert_eq!(after[0].hostname, "example.com");
    }
}
