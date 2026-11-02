//! System tray icon. Closing the main window only hides it, so the vault
//! stays unlocked and terminals, SFTP sessions and forwards keep running.
//! The app really exits only through the tray's Quit item.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Passed by the autostart entry so a login start goes straight to the tray.
pub const MINIMIZED_FLAG: &str = "--minimized";

/// Emitted whenever the main window is brought up. A locked vault answers it
/// with a Windows Hello prompt, which would be missed while the window is
/// hidden in the tray.
pub const WINDOW_SHOWN_EVENT: &str = "main-window-shown";

pub fn init(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Remotix", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "quit", "Quit Remotix", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &exit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Remotix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => quit(app),
            _ => {}
        })
        // Not emitted on Linux, where the menu's Show item does the job.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit(WINDOW_SHOWN_EVENT, ());
    }
}

fn quit(app: &AppHandle) {
    app.state::<AppState>().ssh.stop_all();
    app.exit(0);
}
