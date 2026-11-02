use serde::{Deserialize, Serialize};

/// A toolbox category grouping command snippets.
#[derive(Debug, Clone, Serialize)]
pub struct SnippetCategory {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A saved command the terminal toolbox can insert into or run in a tab.
/// `{{name}}` inside `command` marks a placeholder the UI fills in first.
#[derive(Debug, Clone, Serialize)]
pub struct Snippet {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub command: String,
    pub description: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnippetInput {
    pub category_id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub description: String,
}
