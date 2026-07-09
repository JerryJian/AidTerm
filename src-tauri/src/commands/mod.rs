use std::sync::mpsc::Sender;
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
    shell: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    manager.spawn_local(id.clone(), rows, cols, app, shell)?;
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
    log::info!("[ssh_connect] calling manager.connect_ssh (id={}, host={}:{})", id, host, port);
    let result = manager.connect_ssh(
        id.clone(), host.clone(), port, username, password, private_key_path.clone(),
        proxy, rows, cols,
        agent_forwarding.unwrap_or(false),
        x11_forwarding.unwrap_or(false),
        app,
    );
    log::info!("[ssh_connect] manager.connect_ssh returned: {:?}", result);
    result?;
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

async fn sftp_call<T: Send + 'static>(
    manager: &sftp::SftpManager,
    conn_id: &str,
    make_cmd: impl FnOnce(Sender<Result<T, String>>) -> sftp::SftpCmd,
) -> Result<T, String> {
    let cmd_tx = {
        let connections = manager.connections.lock().map_err(|e| e.to_string())?;
        let conn = connections.get(conn_id).ok_or("SFTP connection not found")?;
        conn.cmd_tx()
    };
    let (tx, rx) = std::sync::mpsc::channel();
    cmd_tx.send(make_cmd(tx)).map_err(|e| format!("Send error: {}", e))?;
    tokio::task::spawn_blocking(move || rx.recv().map_err(|e| format!("Receive error: {}", e))?)
        .await
        .map_err(|e| format!("Join error: {}", e))?
}

#[tauri::command]
pub async fn sftp_connect(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    host: String,
    port: u16,
    username: String,
    password: String,
    private_key_path: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = sftp::SftpConnection::connect(host, port, username, password, private_key_path, app)?;
    let mut connections = manager.connections.lock().map_err(|e| e.to_string())?;
    connections.insert(id.clone(), conn);
    Ok(id)
}

#[tauri::command]
pub async fn sftp_disconnect(
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
pub async fn sftp_list_dir(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<Vec<sftp::FileEntry>, String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::ListDir { path, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_download(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    transfer_id: String,
    remote: String,
    local: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Download { id: transfer_id, remote, local, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_upload(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    transfer_id: String,
    remote: String,
    local: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Upload { id: transfer_id, local, remote, resp: tx }).await
}

#[tauri::command]
#[allow(dead_code)]
pub async fn sftp_cancel_transfer(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    transfer_id: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::CancelTransfer { id: transfer_id, resp: tx }).await
}

#[tauri::command]
#[allow(dead_code)]
pub async fn sftp_create(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
    is_dir: bool,
    mode: u32,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Create { path, is_dir, mode, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_remove(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Remove { path, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_rename(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Rename { old: old_path, new: new_path, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_mkdir(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    path: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::Mkdir { path, resp: tx }).await
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
pub async fn sftp_read_file(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    remote: String,
) -> Result<String, String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::ReadFile { remote, resp: tx }).await
}

#[tauri::command]
pub async fn sftp_write_file(
    manager: State<'_, sftp::SftpManager>,
    conn_id: String,
    remote: String,
    content: String,
) -> Result<(), String> {
    sftp_call(&manager, &conn_id, |tx| sftp::SftpCmd::WriteFile { remote, content, resp: tx }).await
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
pub async fn get_remote_system_info(
    manager: State<'_, crate::session::SessionManager>,
    session_id: String,
) -> Result<SystemInfo, String> {
    log::info!("[get_remote_system_info] start session={}", session_id);
    let t0 = std::time::Instant::now();

    let uname_output = manager.exec(&session_id, "uname -a").await?;
    log::info!("[get_remote_system_info] uname done in {:?}", t0.elapsed());

    let parts: Vec<&str> = uname_output.split_whitespace().collect();
    let os = parts.first().unwrap_or(&"remote").to_string();
    let hostname = parts.get(1).unwrap_or(&"remote").to_string();
    let kernel = parts.get(2).unwrap_or(&"remote").to_string();
    let arch = parts.iter().nth_back(1).unwrap_or(&"remote").to_string();

    let t1 = std::time::Instant::now();
    let os_release = manager.exec(&session_id, "cat /etc/os-release").await;
    let os_label = os_release.as_ref().ok().and_then(|out| {
        for line in out.lines() {
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                return Some(val.trim_matches('"').to_string());
            }
        }
        for line in out.lines() {
            if let Some(val) = line.strip_prefix("NAME=") {
                return Some(val.trim_matches('"').to_string());
            }
        }
        None
    });
    log::info!("[get_remote_system_info] os-release done in {:?}", t1.elapsed());

    log::info!("[get_remote_system_info] complete in {:?}", t0.elapsed());
    Ok(SystemInfo {
        os: os_label.unwrap_or(os),
        arch,
        hostname,
        kernel,
        shell: "remote".to_string(),
    })
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
            tool_calls: Some(response.tool_calls.clone()),
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
        tool_calls: None,
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
            tool_calls: Some(response.tool_calls.clone()),
        });
        ai_state.save_history(&session_id, updated);
    }

    Ok(response)
}

#[tauri::command]
pub async fn fetch_ai_models(
    provider: String,
    base_url: String,
    api_key: String,
) -> Result<Vec<String>, String> {
    ai::fetch_models(&provider, &base_url, &api_key).await
}

#[tauri::command]
pub fn ai_clear_history(
    ai_state: State<'_, ai::AiState>,
    session_id: String,
) -> Result<(), String> {
    ai_state.clear_history(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn open_devtools(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        w.open_devtools();
    }
}

#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

fn exe_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|dir| {
                let full = dir.join(name);
                full.exists()
            })
        })
        .unwrap_or(false)
}

#[tauri::command]
pub fn detect_shells() -> Vec<String> {
    let mut shells = Vec::new();

    if cfg!(target_os = "windows") {
        shells.push("cmd.exe".into());
        if std::path::Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe").exists() {
            shells.push("powershell.exe".into());
        }
        if exe_in_path("pwsh.exe") {
            shells.push("pwsh.exe".into());
        }
        if exe_in_path("wsl.exe") {
            shells.push("wsl.exe".into());
        }
        if exe_in_path("bash.exe") {
            shells.push("bash.exe".into());
        }
    } else {
        // macOS default is zsh since Catalina 2019
        if cfg!(target_os = "macos") {
            shells.push("zsh".into());
        }
        shells.push("bash".into());
        shells.push("sh".into());
        if cfg!(not(target_os = "macos")) {
            if exe_in_path("zsh") { shells.push("zsh".into()); }
        }
        if exe_in_path("fish") { shells.push("fish".into()); }
    }

    shells
}
