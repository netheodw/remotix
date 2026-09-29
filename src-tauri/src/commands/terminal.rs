use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::ssh::terminal::TerminalEvent;
use crate::state::AppState;

#[tauri::command]
pub fn terminal_open(
    state: State<'_, AppState>,
    profile_id: String,
    cols: u32,
    rows: u32,
    channel: tauri::ipc::Channel<TerminalEvent>,
) -> AppResult<String> {
    let id = crate::util::new_id();
    let db = state.db.clone();
    let vault = state.vault.clone();
    state.ssh.open_terminal(id.clone(), db, vault, profile_id, cols, rows, move |event| {
        let _ = channel.send(event);
    });
    Ok(id)
}

#[tauri::command]
pub fn terminal_input(state: State<'_, AppState>, id: String, data_b64: String) -> AppResult<()> {
    let bytes = BASE64
        .decode(data_b64)
        .map_err(|_| AppError::Validation("invalid input encoding".into()))?;
    state.ssh.terminal_input(&id, bytes);
    Ok(())
}

#[tauri::command]
pub fn terminal_resize(state: State<'_, AppState>, id: String, cols: u32, rows: u32) -> AppResult<()> {
    state.ssh.terminal_resize(&id, cols, rows);
    Ok(())
}

#[tauri::command]
pub fn terminal_close(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.ssh.close_terminal(&id);
    Ok(())
}
