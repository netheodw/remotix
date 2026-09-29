use serde::{Deserialize, Serialize};

/// A node in a nested group tree. The same shape backs both `terminal_groups`
/// and `port_forward_groups`; which table it lives in is decided by the
/// repository, not by this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewGroupInput {
    pub parent_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGroupInput {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort_order: Option<i64>,
}
