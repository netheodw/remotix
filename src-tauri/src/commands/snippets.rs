use tauri::State;

use crate::domain::{Snippet, SnippetCategory, SnippetInput};
use crate::error::{AppError, AppResult};
use crate::repositories::SnippetRepository;
use crate::state::AppState;

#[tauri::command]
pub async fn snippet_categories_list(state: State<'_, AppState>) -> AppResult<Vec<SnippetCategory>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::list_categories(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippet_categories_create(state: State<'_, AppState>, name: String) -> AppResult<SnippetCategory> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::create_category(&conn, &name)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippet_categories_rename(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<SnippetCategory> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::rename_category(&conn, &id, &name)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippet_categories_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::delete_category(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippets_list(state: State<'_, AppState>) -> AppResult<Vec<Snippet>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::list(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippets_create(state: State<'_, AppState>, input: SnippetInput) -> AppResult<Snippet> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::create(&conn, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippets_update(state: State<'_, AppState>, id: String, input: SnippetInput) -> AppResult<Snippet> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::update(&conn, &id, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn snippets_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SnippetRepository::delete(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
