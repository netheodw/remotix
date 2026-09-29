use std::path::PathBuf;

use tauri::Manager;

use crate::error::{AppError, AppResult};
use crate::sftp::local::{self, LocalListing, LocalRoot};

#[tauri::command]
pub async fn local_list_dir(path: String) -> AppResult<LocalListing> {
    tokio::task::spawn_blocking(move || local::list_dir(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn local_home_dir(app: tauri::AppHandle) -> AppResult<String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(home.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn local_roots(app: tauri::AppHandle) -> AppResult<Vec<LocalRoot>> {
    // Scoped so the borrow of `app` ends before the await below.
    let shortcuts: Vec<LocalRoot> = {
        let resolver = app.path();
        [
            ("Home", resolver.home_dir()),
            ("Desktop", resolver.desktop_dir()),
            ("Downloads", resolver.download_dir()),
            ("Documents", resolver.document_dir()),
        ]
        .into_iter()
        .filter_map(|(label, dir)| {
            dir.ok().map(|d| LocalRoot {
                label: label.to_string(),
                path: d.to_string_lossy().to_string(),
                kind: "shortcut".to_string(),
            })
        })
        .collect()
    };

    let drives = tokio::task::spawn_blocking(local::drive_roots)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(shortcuts.into_iter().chain(drives).collect())
}

#[tauri::command]
pub async fn local_delete(path: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || local::remove_entry(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Polled while a remote file is open in an editor, to notice a save.
#[tauri::command]
pub async fn local_file_mtime(path: String) -> AppResult<Option<i64>> {
    Ok(local::modified_at(&PathBuf::from(path)))
}

#[tauri::command]
pub async fn local_discard_scratch(path: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || local::discard_scratch(&PathBuf::from(path)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn local_mkdir(parent: String, name: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || local::make_dir(&PathBuf::from(parent), &name))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}
