mod ai;
mod adb;
mod cast;
mod commands;
mod crypto;
mod file_fs;
mod keychain;
mod known_hosts;
mod netaddr;
mod proxy;
mod serial;
mod session;
mod session_store;
mod sftp;
mod sysproxy;
mod tunnel;
mod zmodem;

use ai::AiState;
use keychain::KeychainManager;
use known_hosts::KnownHostsManager;
use session::SessionManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // All platforms: use custom Vue titlebar (decorations:false in tauri.conf.json)

            // Keychain manager with app data dir
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            app.manage(KeychainManager::new(app_data));

            // Known hosts manager (reads ~/.ssh/known_hosts)
            let home_dir = app.path().home_dir().map_err(|e| e.to_string())?;
            app.manage(KnownHostsManager::new(home_dir));

            Ok(())
        })
        .manage(AiState::new())
        .manage(SessionManager::new())
        .manage(sftp::SftpManager::new())
        .manage(zmodem::ZmodemState::new())
        .manage(tunnel::TunnelManager::new())
        .manage(proxy::ProxyManager::new())
        .manage(cast::CastState::default())
        .invoke_handler(tauri::generate_handler![
            commands::connection_create,
            commands::connection_write,
            commands::connection_resize,
            commands::connection_kill,
            commands::wsl_list_distros,
            commands::serial_list_ports,
            commands::adb_list_devices,
            commands::adb_status,
            commands::adb_kill_server,
            commands::adb_occupied_devices,
            commands::cast_start,
            commands::cast_stop,
            commands::cast_frame,
            commands::cast_input,
            commands::file_connect,
            commands::file_disconnect,
            commands::file_home_dir,
            commands::file_list_dir,
            commands::file_download,
            commands::file_upload,
            commands::file_cancel_transfer,
            commands::file_remove,
            commands::file_rename,
            commands::file_mkdir,
            commands::file_create,
            commands::file_read,
            commands::file_write,
            commands::load_session_store,
            commands::save_session_store,
            commands::zmodem_respond,
            commands::tunnel_create,
            commands::tunnel_list,
            commands::tunnel_remove,
            commands::proxy_list,
            commands::proxy_save,
            commands::proxy_delete,
            commands::cli_args,
            commands::get_system_info,
            commands::get_remote_system_info,
            commands::key_list,
            commands::key_generate_rsa,
            commands::key_generate_ed25519,
            commands::key_delete,
            commands::key_import,
            commands::known_hosts_list,
            commands::known_hosts_add,
            commands::known_hosts_remove,
            commands::ai_chat,
            commands::ai_cancel,
            commands::ai_execute,
            commands::ai_continue,
            commands::ai_clear_history,
            commands::fetch_ai_models,
            commands::get_platform,
            commands::detect_shells,
            commands::write_text_file,
            commands::open_devtools,
            commands::update::check_for_update,
            commands::update::download_update,
            commands::update::install_update,
            commands::update::get_app_version,
            commands::update::get_installer_type_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
