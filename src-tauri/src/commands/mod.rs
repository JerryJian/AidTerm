use tauri::{Manager, State};
use crate::ai;
use crate::keychain;
use crate::known_hosts;
use crate::proxy;
use crate::session::SessionManager;
use crate::session_store;
use crate::sftp;
use crate::tunnel;
use crate::zmodem;

#[tauri::command]
pub async fn spawn_terminal(
    app: tauri::AppHandle,
    manager: State<'_, SessionManager>,
    rows: u16,
    cols: u16,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    manager.spawn_local(id.clone(), rows, cols, app)?;
    Ok(id)
}

#[tauri::command]
pub async fn telnet_connect(
    app: tauri::AppHandle,
    manager: State<'_, SessionManager>,
    host: String,
    port: u16,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    manager.connect_telnet(id.clone(), host, port, app)?;
    Ok(id)
}

#[tauri::command]
pub async fn ssh_connect(
    app: tauri::AppHandle,
    manager: State<'_, SessionManager>,
    proxy_manager: State<'_, proxy::ProxyManager>,
    host: String,
    port: u16,
    username: String,
    password: String,
    private_key_path: Option<String>,
    proxy_id: Option<String>,
    rows: u16,
    cols: u16,
    agent_forwarding: Option<bool>,
    x11_forwarding: Option<bool>,
) -> Result<String, String> {
    let proxy = proxy_id.and_then(|id| proxy_manager.get(&id));
    let id = uuid::Uuid::new_v4().to_string();
    manager.connect_ssh(
        id.clone(), host, port, username, password, private_key_path,
        proxy, rows, cols,
        agent_forwarding.unwrap_or(false),
        x11_forwarding.unwrap_or(false),
        app,
    )?;
    Ok(id)
}

#[tauri::command]
pub async fn write_terminal(
    manager: State<'_, SessionManager>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    manager.write(&session_id, &data)
}

#[tauri::command]
pub async fn resize_terminal(
    manager: State<'_, SessionManager>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    manager.resize(&session_id, rows, cols)
}

#[tauri::command]
pub async fn kill_terminal(
    manager: State<'_, SessionManager>,
    session_id: String,
) -> Result<(), String> {
    manager.kill(&session_id)
}

#[tauri::command]
pub async fn load_session_store(app: tauri::AppHandle) -> Result<session_store::SessionStoreData, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    session_store::load(dir)
}

#[tauri::command]
pub async fn save_session_store(
    app: tauri::AppHandle,
    data: session_store::SessionStoreData,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    session_store::save(dir, &data)
}

#[tauri::command]
pub fn sftp_connect(
    manager: State<'_, sftp::SftpManager>,
    host: String,
    port: u16,
    username: String,
    password: String,
    private_key_path: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = sftp::SftpConnection::connect(host, port, username, password, private_key_path)?;
    let mut connections = manager.connections.lock().map_err(|e| e.to_string())?;
    connections.insert(id.clone(), conn);
    Ok(id)
}

#[tauri::command]
pub fn sftp_disconnect(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
) -> Result<(), String> {
    let mut connections = manager.connections.lock().map_err(|e| e.to_string())?;
    if let Some(mut conn) = connections.remove(&conn_id) {
        conn.kill();
    }
    Ok(())
}

#[tauri::command]
pub fn sftp_list_dir(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<Vec<sftp::FileEntry>, String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.list_dir(&path)
}

#[tauri::command]
pub fn sftp_download(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    remote: String,
    local: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.download(&remote, &local)
}

#[tauri::command]
pub fn sftp_upload(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    local: String,
    remote: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.upload(&local, &remote)
}

#[tauri::command]
pub fn sftp_remove(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.remove(&path)
}

#[tauri::command]
pub fn sftp_rename(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.rename(&old_path, &new_path)
}

#[tauri::command]
pub fn sftp_mkdir(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.mkdir(&path)
}

#[tauri::command]
pub fn zmodem_respond(
    state: State<'_, zmodem::ZmodemState>,
    session_id: String,
    save_path: Option<String>,
) -> Result<(), String> {
    let mut responses = state.responses.lock().map_err(|e| e.to_string())?;
    responses.insert(session_id, save_path);
    Ok(())
}

#[tauri::command]
pub fn sftp_read_file(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    remote: String,
) -> Result<String, String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.read_file(&remote)
}

#[tauri::command]
pub fn sftp_write_file(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    remote: String,
    content: String,
) -> Result<(), String> {
    let connections = manager.connections.lock().map_err(|e| e.to_string())?;
    let conn = connections.get(&conn_id).ok_or("SFTP connection not found")?;
    conn.write_file(&remote, &content)
}

#[tauri::command]
pub fn tunnel_create(
    manager: State<'_, tunnel::TunnelManager>,
    req: tunnel::TunnelCreateRequest,
) -> Result<tunnel::TunnelInfo, String> {
    manager.create(req)
}

#[tauri::command]
pub fn tunnel_list(
    manager: State<'_, tunnel::TunnelManager>,
) -> Result<Vec<tunnel::TunnelInfo>, String> {
    Ok(manager.list())
}

#[tauri::command]
pub fn tunnel_remove(
    manager: State<'_, tunnel::TunnelManager>,
    id: String,
) -> Result<(), String> {
    manager.remove(&id)
}

#[tauri::command]
pub fn proxy_list(
    manager: State<'_, proxy::ProxyManager>,
) -> Result<Vec<proxy::ProxyConfig>, String> {
    Ok(manager.list())
}

#[tauri::command]
pub fn proxy_save(
    manager: State<'_, proxy::ProxyManager>,
    config: proxy::ProxyConfig,
) -> Result<(), String> {
    manager.save(config);
    Ok(())
}

#[tauri::command]
pub fn proxy_delete(
    manager: State<'_, proxy::ProxyManager>,
    id: String,
) -> Result<(), String> {
    manager.delete(&id);
    Ok(())
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub hostname: String,
    pub kernel: String,
    pub shell: String,
}

#[tauri::command]
pub async fn get_system_info() -> SystemInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string());

    let kernel = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        std::process::Command::new("uname")
            .arg("-a")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    let shell = std::env::var("SHELL")
        .or_else(|_| std::env::var("ComSpec"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") { "cmd.exe".into() } else { "sh".into() }
        });

    SystemInfo { os, arch, hostname, kernel, shell }
}

#[tauri::command]
pub fn get_cli_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

#[tauri::command]
pub fn key_list(
    manager: State<'_, keychain::KeychainManager>,
) -> Result<Vec<keychain::KeyInfo>, String> {
    manager.list()
}

#[tauri::command]
pub fn key_generate_rsa(
    manager: State<'_, keychain::KeychainManager>,
    name: String,
    bits: u32,
    passphrase: Option<String>,
) -> Result<keychain::KeyInfo, String> {
    manager.generate_rsa(name, bits, passphrase)
}

#[tauri::command]
pub fn key_generate_ed25519(
    manager: State<'_, keychain::KeychainManager>,
    name: String,
    passphrase: Option<String>,
) -> Result<keychain::KeyInfo, String> {
    manager.generate_ed25519(name, passphrase)
}

#[tauri::command]
pub fn key_delete(
    manager: State<'_, keychain::KeychainManager>,
    id: String,
) -> Result<(), String> {
    manager.delete(&id)
}

#[tauri::command]
pub fn key_import(
    manager: State<'_, keychain::KeychainManager>,
    name: String,
    private_key_path: String,
) -> Result<keychain::KeyInfo, String> {
    manager.import(name, private_key_path)
}

#[tauri::command]
pub fn known_hosts_list(
    manager: State<'_, known_hosts::KnownHostsManager>,
) -> Result<Vec<known_hosts::KnownHostEntry>, String> {
    manager.list()
}

#[tauri::command]
pub fn known_hosts_add(
    manager: State<'_, known_hosts::KnownHostsManager>,
    host: String,
    key_type: String,
    key: String,
) -> Result<(), String> {
    manager.add(&host, &key_type, &key)
}

#[tauri::command]
pub fn known_hosts_remove(
    manager: State<'_, known_hosts::KnownHostsManager>,
    host: String,
    key_type: String,
) -> Result<(), String> {
    manager.remove(&host, &key_type)
}

#[tauri::command]
pub async fn ai_chat(
    ai_state: State<'_, ai::AiState>,
    session_id: String,
    messages: Vec<ai::ChatMessage>,
    config: ai::AiConfig,
) -> Result<ai::AiResponse, String> {
    // Save conversation history
    ai_state.save_history(&session_id, messages.clone());

    let response = ai::chat_completion(messages, &config).await?;

    // If there are tool calls, append the assistant message with tool_calls to history
    if !response.tool_calls.is_empty() {
        let mut updated = ai_state.load_history(&session_id);
        updated.push(ai::ChatMessage {
            role: "assistant".to_string(),
            content: response.text.clone().unwrap_or_default(),
            tool_call_id: None,
        });
        ai_state.save_history(&session_id, updated);
    }

    Ok(response)
}

#[tauri::command]
pub async fn ai_execute(command: String) -> Result<String, String> {
    ai::execute_command(&command).await
}

#[tauri::command]
pub async fn ai_continue(
    ai_state: State<'_, ai::AiState>,
    session_id: String,
    tool_call_id: String,
    tool_result: String,
    config: ai::AiConfig,
) -> Result<ai::AiResponse, String> {
    let mut history = ai_state.load_history(&session_id);

    // Add tool result message
    history.push(ai::ChatMessage {
        role: "tool".to_string(),
        content: tool_result,
        tool_call_id: Some(tool_call_id),
    });

    ai_state.save_history(&session_id, history.clone());

    let response = ai::chat_completion(history, &config).await?;

    // If more tool calls, save the assistant message
    if !response.tool_calls.is_empty() {
        let mut updated = ai_state.load_history(&session_id);
        updated.push(ai::ChatMessage {
            role: "assistant".to_string(),
            content: response.text.clone().unwrap_or_default(),
            tool_call_id: None,
        });
        ai_state.save_history(&session_id, updated);
    }

    Ok(response)
}

#[tauri::command]
pub fn ai_clear_history(
    ai_state: State<'_, ai::AiState>,
    session_id: String,
) -> Result<(), String> {
    ai_state.clear_history(&session_id);
    Ok(())
}
