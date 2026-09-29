use serde::Serialize;
use tauri::{Emitter, State};

use crate::domain::{ForwardRuntimeStatus, NewPortForwardInput, PortForward, UpdatePortForwardInput};
use crate::error::{AppError, AppResult};
use crate::repositories::ForwardRepository;
use crate::state::AppState;

#[derive(Serialize, Clone)]
struct ForwardStatusEvent {
    id: String,
    status: ForwardRuntimeStatus,
}

#[tauri::command]
pub async fn forwards_list(state: State<'_, AppState>) -> AppResult<Vec<PortForward>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::list(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn forwards_get(state: State<'_, AppState>, id: String) -> AppResult<PortForward> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::get(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn forwards_create(state: State<'_, AppState>, input: NewPortForwardInput) -> AppResult<PortForward> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::create(&conn, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn forwards_update(state: State<'_, AppState>, input: UpdatePortForwardInput) -> AppResult<PortForward> {
    let db = state.db.clone();
    let id = input.id.clone();
    let was_running = state.ssh.is_forward_running(&id);
    if was_running {
        state.ssh.stop_forward(&id);
    }
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::update(&conn, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn forwards_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.ssh.stop_forward(&id);
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::delete(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn forwards_duplicate(state: State<'_, AppState>, id: String, new_name: String) -> AppResult<PortForward> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::duplicate(&conn, &id, &new_name)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Only changes which group the forward is filed under, so unlike
/// `forwards_update` a running forward is left running.
#[tauri::command]
pub async fn forwards_move_to_group(
    state: State<'_, AppState>,
    id: String,
    group_id: Option<String>,
) -> AppResult<PortForward> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::move_to_group(&conn, &id, group_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub fn forwards_start(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    start_forward_with_events(app, &state, id);
    Ok(())
}

/// Starts a forward and wires its status updates to the `forward-status`
/// event, the same path used whether the user clicked Start or the forward
/// is being auto-started (with `start_on_launch`) right after vault unlock.
pub fn start_forward_with_events(app: tauri::AppHandle, state: &AppState, id: String) {
    let db = state.db.clone();
    let vault = state.vault.clone();
    let forward_id = id.clone();
    state.ssh.start_forward(id, db, vault, move |status| {
        let _ = app.emit("forward-status", ForwardStatusEvent { id: forward_id.clone(), status });
    });
}

#[tauri::command]
pub fn forwards_stop(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.ssh.stop_forward(&id);
    Ok(())
}
