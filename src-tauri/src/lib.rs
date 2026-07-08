mod ai;
mod commands;
mod crypto;
mod keychain;
mod known_hosts;
mod proxy;
mod session;
mod session_store;
mod sftp;
mod tunnel;
mod zmodem;

use ai::AiState;
use keychain::KeychainManager;
use known_hosts::KnownHostsManager;
use session::SessionManager;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // Keychain manager with app data dir
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            app.manage(KeychainManager::new(app_data));

            // Known hosts manager (reads ~/.ssh/known_hosts)
            let home_dir = app.path().home_dir().map_err(|e| e.to_string())?;
            app.manage(KnownHostsManager::new(home_dir));

            // System tray
            let show_item = MenuItemBuilder::with_id("show", "Show/Hide").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&quit_item)
                .build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .manage(AiState::new())
        .manage(SessionManager::new())
        .manage(sftp::SftpManager::new())
        .manage(zmodem::ZmodemState::new())
        .manage(tunnel::TunnelManager::new())
        .manage(proxy::ProxyManager::new())
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
            commands::sftp_read_file,
            commands::sftp_write_file,
            commands::zmodem_respond,
            commands::tunnel_create,
            commands::tunnel_list,
            commands::tunnel_remove,
            commands::proxy_list,
            commands::proxy_save,
            commands::proxy_delete,
            commands::get_cli_args,
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
            commands::ai_execute,
            commands::ai_continue,
            commands::ai_clear_history,
            commands::get_platform,
            commands::detect_shells,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
