//! Saved sudo passwords. The value itself never crosses IPC: the terminal
//! thread saves and answers prompts on its own (`ssh/sudo.rs`); the UI can
//! only see whether one is saved, forget it, or turn remembering off.

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::repositories::{SudoRepository, SudoStatus};
use crate::state::AppState;

#[tauri::command]
pub async fn sudo_status(state: State<'_, AppState>, profile_id: String) -> AppResult<SudoStatus> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SudoRepository::status(&conn, &profile_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn sudo_forget(state: State<'_, AppState>, profile_id: String) -> AppResult<SudoStatus> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SudoRepository::forget(&conn, &profile_id)?;
        SudoRepository::status(&conn, &profile_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn sudo_set_remember(
    state: State<'_, AppState>,
    profile_id: String,
    remember: bool,
) -> AppResult<SudoStatus> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SudoRepository::set_remember(&conn, &profile_id, remember)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
