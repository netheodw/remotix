use tauri::State;

use crate::domain::{SessionCommand, TerminalSessionSummary};
use crate::error::{AppError, AppResult};
use crate::repositories::SessionLogRepository;
use crate::state::AppState;

const DEFAULT_LIST_LIMIT: usize = 500;
const MAX_LIST_LIMIT: usize = 5000;

#[tauri::command]
pub async fn session_log_list(
    state: State<'_, AppState>,
    profile_id: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<TerminalSessionSummary>> {
    let db = state.db.clone();
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SessionLogRepository::list(&conn, profile_id.as_deref(), search.as_deref(), limit)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn session_log_commands(state: State<'_, AppState>, session_id: String) -> AppResult<Vec<SessionCommand>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SessionLogRepository::commands(&conn, &session_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Records one command run in a terminal session. `session_id` is the id
/// `terminal_open` returned. Returns `None` when the line was skipped.
#[tauri::command]
pub async fn session_log_record(
    state: State<'_, AppState>,
    session_id: String,
    command: String,
) -> AppResult<Option<SessionCommand>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SessionLogRepository::record_command(&conn, &session_id, &command)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn session_log_delete(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SessionLogRepository::delete(&conn, &session_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn session_log_clear(state: State<'_, AppState>) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        SessionLogRepository::clear_finished(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
