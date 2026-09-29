use tauri::State;

use crate::domain::{NewSshProfileInput, SshProfile, UpdateSshProfileInput};
use crate::error::{AppError, AppResult};
use crate::repositories::ProfileRepository;
use crate::ssh::session::{self, ConnectStage};
use crate::state::AppState;

/// Reads a private key file the user selected via the native file picker.
/// The path itself is never persisted; the caller is expected to encrypt
/// and store the returned contents (or discard them) immediately.
#[tauri::command]
pub async fn profiles_import_private_key_file(path: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let contents = std::fs::read_to_string(&path)
            .map_err(|_| AppError::Validation("could not read the selected file".into()))?;
        Ok::<_, AppError>(contents)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_list(state: State<'_, AppState>) -> AppResult<Vec<SshProfile>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::list(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_get(state: State<'_, AppState>, id: String) -> AppResult<SshProfile> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::get(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_create(state: State<'_, AppState>, input: NewSshProfileInput) -> AppResult<SshProfile> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::create(&conn, &vault, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_update(state: State<'_, AppState>, input: UpdateSshProfileInput) -> AppResult<SshProfile> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::update(&conn, &vault, input)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::delete(&conn, &id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_duplicate(state: State<'_, AppState>, id: String, new_name: String) -> AppResult<SshProfile> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::duplicate(&conn, &vault, &id, &new_name)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn profiles_move_to_group(
    state: State<'_, AppState>,
    id: String,
    group_id: Option<String>,
) -> AppResult<SshProfile> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        ProfileRepository::move_to_group(&conn, &id, group_id)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[derive(Clone, serde::Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum TestConnectionStage {
    Connecting,
    VerifyingHost,
    Authenticating,
}

#[tauri::command]
pub async fn profiles_test_connection(
    state: State<'_, AppState>,
    id: String,
    channel: tauri::ipc::Channel<TestConnectionStage>,
) -> AppResult<()> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        let profile = ProfileRepository::get(&conn, &id)?;
        let auth = ProfileRepository::load_auth_material(&conn, &vault, &id)?;
        let on_stage = |stage: ConnectStage| {
            let dto = match stage {
                ConnectStage::Connecting => TestConnectionStage::Connecting,
                ConnectStage::VerifyingHost => TestConnectionStage::VerifyingHost,
                ConnectStage::Authenticating => TestConnectionStage::Authenticating,
            };
            let _ = channel.send(dto);
        };
        let result = session::connect_verified(
            &conn,
            &profile.hostname,
            profile.port,
            &profile.username,
            &auth,
            on_stage,
        );
        match &result {
            Ok(_) => ProfileRepository::record_connection_result(&conn, &id, "success")?,
            Err(_) => ProfileRepository::record_connection_result(&conn, &id, "failed")?,
        }
        result.map(|_| ())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
