#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;

#[tauri::command]
fn catalog() -> &'static [catalog::Product] {
    &catalog::PRODUCTS
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![catalog])
        .run(tauri::generate_context!())
        .expect("failed to run Golden Launcher");
}
