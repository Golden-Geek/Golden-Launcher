#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod operations;
mod platform;
mod releases;
mod state;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let path = app.path().app_local_data_dir()?;
            app.manage(state::Library::new(path)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            state::library,
            state::refresh,
            state::set_channel,
            operations::install,
            operations::cancel_download,
            operations::launch,
            operations::uninstall
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Golden Launcher");
}
