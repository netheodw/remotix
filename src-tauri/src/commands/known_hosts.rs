use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tauri::State;

use crate::domain::KnownHost;
use crate::error::{AppError, AppResult};
use crate::repositories::KnownHostRepository;
use crate::state::AppState;

#[tauri::command]
pub async fn known_hosts_list(state: State<'_, AppState>) -> AppResult<Vec<KnownHost>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        KnownHostRepository::list(&conn)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Records (or replaces) the trusted key for a host. Only ever called after
/// the user has explicitly confirmed a Trust/Replace decision in the UI -
/// this command performs no verification of its own.
#[tauri::command]
pub async fn known_hosts_trust(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    key_algorithm: String,
    host_key_b64: String,
    fingerprint_sha256: String,
) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        let key_bytes = BASE64
            .decode(host_key_b64)
            .map_err(|_| AppError::Validation("invalid host key encoding".into()))?;
        KnownHostRepository::trust(&conn, &host, port, &key_algorithm, &key_bytes, &fingerprint_sha256)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
