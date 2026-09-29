use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::sftp::session::{self, ControlRequest, RemoteListing};
use crate::sftp::transfer::{ConflictPolicy, Direction, TransferCommand, TransferEvent, TransferJob};
use crate::state::AppState;

#[derive(Serialize)]
pub struct SftpConnected {
    pub session_id: String,
    pub home_path: String,
}

/// Unlike `terminal_open`, this awaits the real connect so a first-use host key
/// or a failed authentication rejects the caller's promise and the UI can show
/// the trust dialog.
#[tauri::command]
pub async fn sftp_connect(
    state: State<'_, AppState>,
    profile_id: String,
    transfers: tauri::ipc::Channel<TransferEvent>,
) -> AppResult<SftpConnected> {
    let db = state.db.clone();
    let vault = state.vault.clone();
    let connected = tokio::task::spawn_blocking(move || {
        session::connect(db, vault, profile_id, move |event| {
            // A reloaded webview orphans the channel; the worker must keep
            // transferring rather than treat a dead channel as fatal.
            let _ = transfers.send(event);
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;

    let session_id = crate::util::new_id();
    state.ssh.register_sftp(session_id.clone(), connected.handle);
    Ok(SftpConnected {
        session_id,
        home_path: connected.home,
    })
}

#[tauri::command]
pub async fn sftp_disconnect(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.ssh.close_sftp(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<RemoteListing> {
    let control = state.ssh.sftp_control(&session_id)?;
    let (reply, response) = tokio::sync::oneshot::channel();
    control
        .send(ControlRequest::List { path, reply })
        .map_err(|_| AppError::Ssh("sftp session closed".into()))?;
    response
        .await
        .map_err(|_| AppError::Ssh("sftp session closed".into()))?
}

/// Deletes a remote file, or a directory and its whole contents. Directory
/// recursion happens on the worker; the caller does not say which it is.
#[tauri::command]
pub async fn sftp_delete(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> AppResult<()> {
    let control = state.ssh.sftp_control(&session_id)?;
    let (reply, response) = tokio::sync::oneshot::channel();
    control
        .send(ControlRequest::Remove { path, reply })
        .map_err(|_| AppError::Ssh("sftp session closed".into()))?;
    response
        .await
        .map_err(|_| AppError::Ssh("sftp session closed".into()))?
}

#[derive(Serialize)]
pub struct EditHandle {
    pub local_path: String,
    pub local_dir: String,
    pub name: String,
    pub remote_dir: String,
    pub modified_at: Option<i64>,
}

/// Downloads a remote file to a scratch copy and launches it in whatever editor
/// the OS associates with it. The caller polls the returned path's mtime;
/// saving it back is an ordinary upload through the transfer queue.
///
/// The launch happens here rather than through the opener plugin's JS binding
/// because that command is scope-gated against a static path allowlist, and a
/// per-edit scratch directory cannot be expressed as one. Calling the plugin
/// from Rust needs no webview capability.
#[tauri::command]
pub async fn sftp_edit_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> AppResult<EditHandle> {
    let name = remote_path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Validation("not a file".into()))?
        .to_string();
    let remote_dir = crate::sftp::path::parent_of(&remote_path)
        .ok_or_else(|| AppError::Validation("not a file".into()))?;

    let local_dir = std::env::temp_dir()
        .join("remotix-edit")
        .join(crate::util::new_id());
    let local_path = local_dir.join(&name);

    let control = state.ssh.sftp_control(&session_id)?;
    let (reply, response) = tokio::sync::oneshot::channel();
    control
        .send(ControlRequest::Fetch {
            remote: remote_path,
            local: local_path.clone(),
            reply,
        })
        .map_err(|_| AppError::Ssh("sftp session closed".into()))?;
    response
        .await
        .map_err(|_| AppError::Ssh("sftp session closed".into()))??;

    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(local_path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| AppError::Internal(format!("could not open an editor: {e}")))?;

    Ok(EditHandle {
        modified_at: crate::sftp::local::modified_at(&local_path),
        local_path: local_path.to_string_lossy().to_string(),
        local_dir: local_dir.to_string_lossy().to_string(),
        name,
        remote_dir,
    })
}

/// Takes the two panes' working directories plus bare names, never a joined
/// path - the frontend must not have to know which separator each side uses,
/// nor which entries are directories.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueInput {
    pub direction: Direction,
    pub local_dir: String,
    pub remote_dir: String,
    pub items: Vec<String>,
    pub conflict: ConflictPolicy,
}

#[tauri::command]
pub async fn sftp_enqueue(
    state: State<'_, AppState>,
    session_id: String,
    input: EnqueueInput,
) -> AppResult<Vec<String>> {
    let transfers = state.ssh.sftp_transfers(&session_id)?;
    let local_dir = PathBuf::from(&input.local_dir);
    let mut ids = Vec::with_capacity(input.items.len());

    for name in input.items {
        let item_id = crate::util::new_id();
        ids.push(item_id.clone());
        transfers
            .send(TransferCommand::Enqueue(TransferJob {
                item_id,
                direction: input.direction,
                name,
                local_dir: local_dir.clone(),
                remote_dir: input.remote_dir.clone(),
                conflict: input.conflict,
            }))
            .map_err(|_| AppError::Ssh("sftp session closed".into()))?;
    }
    Ok(ids)
}

#[tauri::command]
pub async fn sftp_cancel(
    state: State<'_, AppState>,
    session_id: String,
    item_id: String,
) -> AppResult<()> {
    state
        .ssh
        .sftp_transfers(&session_id)?
        .send(TransferCommand::Cancel { item_id })
        .map_err(|_| AppError::Ssh("sftp session closed".into()))
}

#[tauri::command]
pub async fn sftp_cancel_all(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state
        .ssh
        .sftp_transfers(&session_id)?
        .send(TransferCommand::CancelAll)
        .map_err(|_| AppError::Ssh("sftp session closed".into()))
}
