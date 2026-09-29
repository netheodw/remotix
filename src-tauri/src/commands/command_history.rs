use tauri::State;

use crate::domain::CommandHistoryEntry;
use crate::error::{AppError, AppResult};
use crate::repositories::CommandHistoryRepository;
use crate::state::AppState;

/// Upper bound on what a terminal tab can pull into memory at connect time.
const MAX_LIST_LIMIT: usize = 2000;

#[tauri::command]
pub async fn command_history_list(
    state: State<'_, AppState>,
    profile_id: String,
    limit: Option<usize>,
) -> AppResult<Vec<CommandHistoryEntry>> {
    let db = state.db.clone();
    let limit = limit.unwrap_or(MAX_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        CommandHistoryRepository::list(&conn, &profile_id, limit)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Records one command the user ran in a terminal tab. Returns the stored
/// entry, or `None` when the line was skipped (blank, space-prefixed, or a
/// repeat of the previous command).
#[tauri::command]
pub async fn command_history_record(
    state: State<'_, AppState>,
    profile_id: String,
    command: String,
) -> AppResult<Option<CommandHistoryEntry>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        CommandHistoryRepository::record(&conn, &profile_id, &command)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn command_history_clear(state: State<'_, AppState>, profile_id: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        CommandHistoryRepository::clear(&conn, &profile_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
