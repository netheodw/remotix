use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::importers::termius::{self, TermiusImportReport, TermiusPreview};
use crate::state::AppState;

/// Describes what a Termius JSON export the user selected would import.
/// Carries no secret material - credentials stay in the file until
/// `import_termius_apply` reads it again.
#[tauri::command]
pub async fn import_termius_preview(state: State<'_, AppState>, path: String) -> AppResult<TermiusPreview> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        termius::preview(&conn, &PathBuf::from(path))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn import_termius_apply(
    state: State<'_, AppState>,
    path: String,
    host_indices: Vec<usize>,
) -> AppResult<TermiusImportReport> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        termius::apply(&conn, &vault, &PathBuf::from(path), &host_indices)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
