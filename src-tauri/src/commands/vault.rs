use serde::Serialize;
use tauri::State;

use crate::commands::forwards::start_forward_with_events;
use crate::error::{AppError, AppResult};
use crate::platform::hello;
use crate::repositories::ForwardRepository;
use crate::state::AppState;
use crate::vault::manager::HelloStatus;
use crate::vault::VaultManager;

#[derive(Serialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    let db = state.db.clone();
    let unlocked = state.vault.is_unlocked();
    let initialized = tokio::task::spawn_blocking(move || VaultManager::is_initialized(&db))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(VaultStatus { initialized, unlocked })
}

#[tauri::command]
pub async fn vault_setup(state: State<'_, AppState>, password: String) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || vault.setup(&db, &password))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn vault_unlock(app: tauri::AppHandle, state: State<'_, AppState>, password: String) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || vault.unlock(&db, &password))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    start_forwards_flagged_for_launch(app, &state).await;
    Ok(())
}

/// Starts every saved forward with `start_on_launch` set, right after a
/// successful unlock (never before - starting a forward needs the vault
/// key to decrypt the SSH profile's credential).
async fn start_forwards_flagged_for_launch(app: tauri::AppHandle, state: &AppState) {
    let db = state.db.clone();
    let forwards = tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ForwardRepository::list(&conn)
    })
    .await
    .ok()
    .and_then(|r| r.ok())
    .unwrap_or_default();

    for forward in forwards {
        if forward.start_on_launch && forward.ssh_profile_id.is_some() {
            start_forward_with_events(app.clone(), state, forward.id);
        }
    }
}

#[tauri::command]
pub async fn vault_lock(state: State<'_, AppState>) -> AppResult<()> {
    state.ssh.stop_all();
    state.vault.lock();
    Ok(())
}

#[derive(Serialize)]
pub struct HelloInfo {
    /// Windows Hello is set up for this Windows user (PIN at minimum).
    pub supported: bool,
    #[serde(flatten)]
    pub status: HelloStatus,
}

#[tauri::command]
pub async fn vault_hello_status(state: State<'_, AppState>) -> AppResult<HelloInfo> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let status = VaultManager::hello_status(&db)?;
        Ok(HelloInfo {
            supported: hello::is_supported(),
            status,
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn vault_hello_enable(state: State<'_, AppState>, password: String) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || vault.enable_hello(&db, &password, hello::create_and_sign))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn vault_hello_disable(state: State<'_, AppState>) -> AppResult<()> {
    if !state.vault.is_unlocked() {
        return Err(AppError::VaultLocked);
    }
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        VaultManager::disable_hello(&db)?;
        hello::delete();
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn vault_unlock_hello(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || vault.unlock_with_hello(&db, hello::sign))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    start_forwards_flagged_for_launch(app, &state).await;
    Ok(())
}
