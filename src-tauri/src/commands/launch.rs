use tauri::{Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::platform::shortcut;
use crate::repositories::ProfileRepository;
use crate::state::AppState;

/// Event sent to the UI when a desktop shortcut is used while Remotix is
/// already running. The UI then calls `launch_take_pending_profile`.
pub const LAUNCH_EVENT: &str = "launch-open-profile";

/// Records the profile a shortcut asked for. Called once at startup with
/// this process's arguments, and from the single-instance callback with a
/// second launch's arguments.
pub fn handle_launch_args<I, S>(app: &tauri::AppHandle, args: I, notify: bool)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(profile_id) = shortcut::profile_from_args(args) else {
        return;
    };
    let state = app.state::<AppState>();
    *state.pending_launch_profile.lock().unwrap() = Some(profile_id);
    if notify {
        let _ = app.emit(LAUNCH_EVENT, ());
    }
}

/// Returns (and clears) the profile id a desktop shortcut asked to open.
/// The UI calls it only after the vault is unlocked.
#[tauri::command]
pub fn launch_take_pending_profile(state: State<'_, AppState>) -> AppResult<Option<String>> {
    if !state.vault.is_unlocked() {
        return Ok(None);
    }
    Ok(state.pending_launch_profile.lock().unwrap().take())
}

/// Creates `Remotix - <profile name>.lnk` on the desktop that starts
/// Remotix straight into that profile's terminal. Returns the file path.
#[tauri::command]
pub async fn profiles_create_desktop_shortcut(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<String> {
    let db = state.db.clone();
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|e| AppError::Internal(format!("desktop folder not found: {e}")))?;
    let exe = std::env::current_exe()?;
    tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        let profile = ProfileRepository::get(&conn, &id)?;
        let link = desktop.join(format!("Remotix - {}.lnk", shortcut::safe_file_stem(&profile.name)));
        shortcut::create(
            &link,
            &exe,
            &format!("{} {}", shortcut::OPEN_PROFILE_FLAG, profile.id),
            &format!("Open {} in Remotix", profile.name),
        )?;
        Ok(link.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}
