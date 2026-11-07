use serde::Serialize;

/// One terminal session in the session log, newest first in listings.
#[derive(Debug, Clone, Serialize)]
pub struct TerminalSessionSummary {
    pub id: String,
    /// `None` once the profile has been deleted; the copied fields remain.
    pub ssh_profile_id: Option<String>,
    pub profile_name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub started_at: String,
    pub connected_at: Option<String>,
    pub ended_at: Option<String>,
    /// connecting | connected | failed | closed | interrupted
    pub status: String,
    pub end_reason: Option<String>,
    pub command_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionCommand {
    pub id: String,
    pub session_id: String,
    pub command: String,
    pub executed_at: String,
}
