use std::collections::HashMap;

use tauri::{Manager, State};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn settings_get_all(state: State<'_, AppState>) -> AppResult<HashMap<String, String>> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok::<_, AppError>(map)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok::<_, AppError>(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
pub fn app_data_dir(app: tauri::AppHandle) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(dir.to_string_lossy().to_string())
}
