mod commands;
mod session;
mod session_store;
mod sftp;
mod zmodem;

use session::SessionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(SessionManager::new())
        .manage(sftp::SftpManager::new())
        .manage(zmodem::ZmodemState::new())
        .invoke_handler(tauri::generate_handler![
            commands::spawn_terminal,
            commands::ssh_connect,
            commands::telnet_connect,
            commands::write_terminal,
            commands::resize_terminal,
            commands::kill_terminal,
            commands::load_session_store,
            commands::save_session_store,
            commands::sftp_connect,
            commands::sftp_disconnect,
            commands::sftp_list_dir,
            commands::sftp_download,
            commands::sftp_upload,
            commands::sftp_remove,
            commands::sftp_rename,
            commands::sftp_mkdir,
            commands::zmodem_respond,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
