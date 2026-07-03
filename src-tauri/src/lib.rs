mod commands;
mod session;
mod session_store;

use session::SessionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SessionManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::spawn_terminal,
            commands::ssh_connect,
            commands::telnet_connect,
            commands::write_terminal,
            commands::resize_terminal,
            commands::kill_terminal,
            commands::load_session_store,
            commands::save_session_store,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
