use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CommandHistoryEntry {
    pub id: String,
    pub ssh_profile_id: String,
    pub command: String,
    pub executed_at: String,
}
