mod commands;
pub mod db;
pub mod domain;
mod error;
mod importers;
mod repositories;
mod sftp;
mod ssh;
mod state;
pub mod testing;
mod util;
mod validation;
pub mod vault;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("remotix.db");
            let pool = db::init_pool(&db_path).expect("failed to initialize local database");
            app.manage(state::AppState::new(pool));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let state = window.state::<state::AppState>();
                state.ssh.stop_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_status,
            commands::vault::vault_setup,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::groups::terminal_groups_list,
            commands::groups::terminal_groups_create,
            commands::groups::terminal_groups_update,
            commands::groups::terminal_groups_delete,
            commands::groups::port_forward_groups_list,
            commands::groups::port_forward_groups_create,
            commands::groups::port_forward_groups_update,
            commands::groups::port_forward_groups_delete,
            commands::profiles::profiles_list,
            commands::profiles::profiles_get,
            commands::profiles::profiles_create,
            commands::profiles::profiles_update,
            commands::profiles::profiles_delete,
            commands::profiles::profiles_duplicate,
            commands::profiles::profiles_move_to_group,
            commands::profiles::profiles_test_connection,
            commands::profiles::profiles_import_private_key_file,
            commands::import::import_termius_preview,
            commands::import::import_termius_apply,
            commands::known_hosts::known_hosts_list,
            commands::known_hosts::known_hosts_trust,
            commands::forwards::forwards_list,
            commands::forwards::forwards_get,
            commands::forwards::forwards_create,
            commands::forwards::forwards_update,
            commands::forwards::forwards_delete,
            commands::forwards::forwards_duplicate,
            commands::forwards::forwards_move_to_group,
            commands::forwards::forwards_start,
            commands::forwards::forwards_stop,
            commands::terminal::terminal_open,
            commands::terminal::terminal_input,
            commands::terminal::terminal_resize,
            commands::terminal::terminal_close,
            commands::command_history::command_history_list,
            commands::command_history::command_history_record,
            commands::command_history::command_history_clear,
            commands::sftp::sftp_connect,
            commands::sftp::sftp_disconnect,
            commands::sftp::sftp_list,
            commands::sftp::sftp_enqueue,
            commands::sftp::sftp_cancel,
            commands::sftp::sftp_cancel_all,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_edit_open,
            commands::local_fs::local_list_dir,
            commands::local_fs::local_home_dir,
            commands::local_fs::local_roots,
            commands::local_fs::local_mkdir,
            commands::local_fs::local_delete,
            commands::local_fs::local_file_mtime,
            commands::local_fs::local_discard_scratch,
            commands::settings::settings_get_all,
            commands::settings::settings_set,
            commands::settings::app_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
