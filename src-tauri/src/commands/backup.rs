use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Writes an encrypted backup of all data to `path`. Needs the vault unlocked.
#[tauri::command]
pub async fn backup_export(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || crate::backup::export(&db, &vault, &PathBuf::from(path)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[derive(Serialize)]
pub struct ImportResult {
    /// Where the replaced data was copied, if there was any.
    pub previous_data_copy: Option<String>,
}

/// Replaces all data with the backup at `path`. Once the password and file
/// check out, closes every session and locks the vault; the user then unlocks
/// with the backup's master password.
#[tauri::command]
pub async fn backup_import(state: State<'_, AppState>, path: String, password: String) -> AppResult<ImportResult> {
    let db = state.db.clone();
    let ssh = state.ssh.clone();
    let vault = state.vault.clone();
    let kept = tokio::task::spawn_blocking(move || {
        crate::backup::import(&db, &PathBuf::from(path), &password, || {
            ssh.stop_all();
            vault.lock();
        })
    })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(ImportResult { previous_data_copy: kept.map(|p| p.display().to_string()) })
}
