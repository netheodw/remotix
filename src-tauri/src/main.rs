// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated helper that rewrites the hosts file, started by Remotix itself.
    if let Some(code) = remotix_lib::apply_hosts_from_args() {
        std::process::exit(code);
    }
    remotix_lib::run()
}
