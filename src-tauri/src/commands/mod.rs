use std::sync::mpsc::Sender;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub mod update;
use tauri::{Manager, State};
use crate::adb;
use crate::ai;
use crate::keychain;
use crate::known_hosts;
use crate::proxy;
use crate::serial;
use crate::session::{ConnectionConfig, SessionManager};
use crate::session_store;
use crate::sftp;
use crate::tunnel;
use crate::zmodem;

#[derive(serde::Serialize)]
pub struct ConnectionHandle {
    pub id: String,
    pub capabilities: Vec<String>,
}

/// Parse `wsl.exe -l -q` output. Handles UTF-16LE (with or without BOM,
/// depending on the WSL build) and plain UTF-8 (optionally with a UTF-8 BOM),
/// skipping empty lines.
fn parse_wsl_distros(output: &[u8]) -> Vec<String> {
    let is_utf8_bom = output.starts_with(&[0xEF, 0xBB, 0xBF]);
    let is_utf16le = output.starts_with(&[0xFF, 0xFE]) || (!is_utf8_bom && output.contains(&0x00));
    let text = if is_utf16le {
        let start = usize::from(output.starts_with(&[0xFF, 0xFE])) * 2;
        let units: Vec<u16> = output[start..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        let body = if is_utf8_bom { &output[3..] } else { output };
        String::from_utf8_lossy(body).into_owned()
    };
    text.lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// List installed WSL distributions (via `wsl.exe -l -q`). Empty on
/// non-Windows or when WSL has no distros installed.
#[tauri::command]
pub fn wsl_list_distros() -> Vec<String> {
    let output = match std::process::Command::new("wsl.exe").args(["-l", "-q"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    parse_wsl_distros(&output)
}

/// Unified connection creation — dispatch on `config.type`.
#[tauri::command]
pub async fn connection_create(
    app: tauri::AppHandle,
    manager: State<'_, SessionManager>,
    proxy_manager: State<'_, proxy::ProxyManager>,
    config: ConnectionConfig,
    rows: u16,
    cols: u16,
) -> Result<ConnectionHandle, String> {
    let proxy = match &config {
        ConnectionConfig::Ssh { proxy_id, .. } => proxy_id.as_deref().and_then(|id| proxy_manager.get(id)),
        _ => None,
    };
    let id = manager.create(app, config, rows, cols, proxy)?;
    let capabilities = manager.capabilities(&id).into_iter().map(|s| s.to_string()).collect();
    Ok(ConnectionHandle { id, capabilities })
}

#[tauri::command]
pub async fn connection_write(
    manager: State<'_, SessionManager>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    manager.write(&session_id, &data)
}

#[tauri::command]
pub async fn connection_resize(
    manager: State<'_, SessionManager>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    manager.resize(&session_id, rows, cols)
}

#[tauri::command]
pub async fn connection_kill(
    manager: State<'_, SessionManager>,
    session_id: String,
) -> Result<(), String> {
    manager.kill(&session_id)
}

#[tauri::command]
pub async fn serial_list_ports() -> Result<Vec<serial::SerialPortInfo>, String> {
    serial::list_available_ports()
}

/// List devices from the isolated 5038 adb server.
/// Uses spawn_blocking so a slow first server start never blocks the UI.
#[tauri::command]
pub async fn adb_list_devices(app: tauri::AppHandle) -> Result<Vec<adb::AdbDevice>, String> {
    tauri::async_runtime::spawn_blocking(move || adb::list_devices(&app))
        .await
        .map_err(|e| format!("adb list devices join error: {}", e))?
}

/// Kill the isolated 5038 adb server. Called when the last adb session closes.
#[tauri::command]
pub async fn adb_kill_server(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || adb::kill_server(&app))
        .await
        .map_err(|e| format!("adb kill-server join error: {}", e))?
}

/// USB devices held by the user's own 5037 server and therefore invisible to
/// our isolated 5038 server. The check is strictly read-only (raw wire
/// protocol), so the user's adb server is never killed or restarted.
#[tauri::command]
pub async fn adb_occupied_devices(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Ok(tauri::async_runtime::spawn_blocking(move || adb::occupied_devices(&app))
        .await
        .map_err(|e| format!("adb occupied devices join error: {}", e))?)
}

/// Unified file backend. sftp connections live in SftpManager (addressed by
/// connection id); adb devices are addressed by serial. All operations dispatch
/// on `kind`, so the frontend speaks one interface regardless of backend.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileConnectConfig {
    Sftp {
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
    },
    Adb {
        serial: String,
    },
    Local,
    Wsl {
        distro: Option<String>,
    },
}

/// Build the filesystem target used by the `local`/`wsl` file kinds.
fn fs_target(kind: &str, handle: &str) -> crate::file_fs::FsTarget {
    if kind == "wsl" {
        crate::file_fs::FsTarget::Wsl { distro: handle.to_string() }
    } else {
        crate::file_fs::FsTarget::Local
    }
}

/// Run a blocking filesystem operation for the `local`/`wsl` kinds.
async fn fs_op<T: Send + 'static>(
    kind: String,
    handle: String,
    op: impl FnOnce(crate::file_fs::FsTarget) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let kind_inner = kind.clone();
    tauri::async_runtime::spawn_blocking(move || op(fs_target(&kind_inner, &handle)))
        .await
        .map_err(|e| format!("{} file op join error: {}", kind, e))?
}

/// Connect a file backend and get its handle id (sftp conn id / adb serial).
#[tauri::command]
pub async fn file_connect(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    config: FileConnectConfig,
) -> Result<String, String> {
    match config {
        FileConnectConfig::Sftp { host, port, username, password, private_key_path } => {
            let id = uuid::Uuid::new_v4().to_string();
            let conn = sftp::SftpConnection::connect(host, port, username, password, private_key_path, app)?;
            manager.connections.lock().map_err(|e| e.to_string())?.insert(id.clone(), conn);
            Ok(id)
        }
        FileConnectConfig::Adb { serial } => Ok(serial),
        FileConnectConfig::Local => Ok("local".to_string()),
        FileConnectConfig::Wsl { distro } => Ok(distro.unwrap_or_default()),
    }
}

#[tauri::command]
pub async fn file_disconnect(
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
) -> Result<(), String> {
    if kind == "sftp" {
        let mut connections = manager.connections.lock().map_err(|e| e.to_string())?;
        if let Some(mut conn) = connections.remove(&handle) {
            conn.kill();
        }
    }
    Ok(())
}

/// Home directory of the current user (used as the initial local browser path).
#[tauri::command]
pub fn file_home_dir() -> Result<String, String> {
    Ok(crate::file_fs::home_dir().to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn file_list_dir(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    path: String,
) -> Result<Vec<sftp::FileEntry>, String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::ListDir { path, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::list_dir(&app, &handle, &path))
            .await
            .map_err(|e| format!("adb list dir join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::list_dir(&t, &path)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_download(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    transfer_id: String,
    remote: String,
    local: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Download { id: transfer_id, remote, local, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::pull(&app, &handle, &remote, &local))
            .await
            .map_err(|e| format!("adb pull join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::download(&t, &remote, &local)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_upload(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    transfer_id: String,
    local: String,
    remote: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Upload { id: transfer_id, local, remote, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::push(&app, &handle, &local, &remote))
            .await
            .map_err(|e| format!("adb push join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::upload(&t, &local, &remote)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
#[allow(dead_code)]
pub async fn file_cancel_transfer(
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    transfer_id: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::CancelTransfer { id: transfer_id, resp: tx }).await,
        "adb" => Err("Cancel not supported for adb transfers".to_string()),
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_remove(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    path: String,
    is_dir: bool,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Remove { path, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::remove(&app, &handle, &path, is_dir))
            .await
            .map_err(|e| format!("adb remove join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::remove(&t, &path, is_dir)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_rename(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Rename { old: old_path, new: new_path, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::rename_item(&app, &handle, &old_path, &new_path))
            .await
            .map_err(|e| format!("adb rename join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::rename(&t, &old_path, &new_path)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_mkdir(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    path: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Mkdir { path, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::mkdir(&app, &handle, &path))
            .await
            .map_err(|e| format!("adb mkdir join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::mkdir(&t, &path)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_create(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    path: String,
    is_dir: bool,
    mode: u32,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::Create { path, is_dir, mode, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || {
            if is_dir {
                adb::mkdir(&app, &handle, &path)
            } else {
                adb::touch(&app, &handle, &path)
            }
        })
        .await
        .map_err(|e| format!("adb create join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::create_file(&t, &path, is_dir)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_read(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    remote: String,
) -> Result<String, String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::ReadFile { remote, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::read_file(&app, &handle, &remote))
            .await
            .map_err(|e| format!("adb read file join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::read_file(&t, &remote)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
}

#[tauri::command]
pub async fn file_write(
    app: tauri::AppHandle,
    manager: State<'_, sftp::SftpManager>,
    kind: String,
    handle: String,
    remote: String,
    content: String,
) -> Result<(), String> {
    match kind.as_str() {
        "sftp" => sftp_call(&manager, &handle, |tx| sftp::SftpCmd::WriteFile { remote, content, resp: tx }).await,
        "adb" => tauri::async_runtime::spawn_blocking(move || adb::write_file(&app, &handle, &remote, &content))
            .await
            .map_err(|e| format!("adb write file join error: {}", e))?,
        "local" | "wsl" => fs_op(kind, handle, move |t| crate::file_fs::write_file(&t, &remote, &content)).await,
        k => Err(format!("unsupported file kind: {}", k)),
    }
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

fn run_hostname_cmd() -> Option<String> {
    let mut cmd = std::process::Command::new("hostname");
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn get_hostname() -> String {
    if let Ok(name) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        return name;
    }
    if let Some(name) = run_hostname_cmd() {
        return name;
    }
    if let Ok(output) = std::process::Command::new("uname").arg("-n").output() {
        if let Ok(name) = String::from_utf8(output.stdout) {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    "unknown".to_string()
}

#[tauri::command]
pub async fn get_system_info() -> SystemInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let hostname = get_hostname();

    let kernel = if cfg!(target_os = "windows") {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "ver"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }
        #[cfg(not(target_os = "windows"))]
        {
            unreachable!()
        }
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
    let arch = manager.exec(&session_id, "uname -m").await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| parts.iter().nth_back(1).unwrap_or(&"remote").to_string());

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

    let shell = manager.exec(&session_id, "basename $SHELL 2>/dev/null || echo unknown").await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "unknown")
        .unwrap_or_else(|| "remote".to_string());

    log::info!("[get_remote_system_info] complete in {:?}", t0.elapsed());
    Ok(SystemInfo {
        os: os_label.unwrap_or(os),
        arch,
        hostname,
        kernel,
        shell,
    })
}

#[tauri::command]
pub fn cli_args() -> Vec<String> {
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

    // Race the completion against cancellation. When the cancel branch wins,
    // the in-flight future is dropped, aborting the underlying HTTP request.
    let token = ai_state.register_cancel(&session_id);
    let fut = ai::chat_completion(messages, &config);
    tokio::pin!(fut);
    let response = tokio::select! {
        r = &mut fut => {
            ai_state.unregister_cancel(&session_id);
            r
        }
        _ = token.cancelled() => {
            ai_state.unregister_cancel(&session_id);
            Err("AI 请求已取消".to_string())
        }
    };
    let response = response?;

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
pub fn ai_cancel(ai_state: State<'_, ai::AiState>, session_id: String) {
    ai_state.cancel_chat(&session_id);
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

    let token = ai_state.register_cancel(&session_id);
    let fut = ai::chat_completion(history, &config);
    tokio::pin!(fut);
    let response = tokio::select! {
        r = &mut fut => {
            ai_state.unregister_cancel(&session_id);
            r
        }
        _ = token.cancelled() => {
            ai_state.unregister_cancel(&session_id);
            Err("AI 请求已取消".to_string())
        }
    };
    let response = response?;

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
                if !full.is_file() {
                    return false;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    match std::fs::metadata(&full) {
                        Ok(m) => m.mode() & 0o111 != 0,
                        Err(_) => false,
                    }
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
        })
        .unwrap_or(false)
}

#[derive(serde::Serialize)]
pub struct ShellProfile {
    pub name: String,
    pub command: String,
    pub icon: String,
}

#[tauri::command]
pub fn detect_shells() -> Vec<ShellProfile> {
    let mut shells = Vec::new();

    if cfg!(target_os = "windows") {
        shells.push(ShellProfile { name: "\u{547D}\u{4EE4}\u{63D0}\u{793A}\u{7B26}" .into(), command: "cmd.exe".into(), icon: "\u{1F4DF}".into() });
        if std::path::Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe").exists() {
            shells.push(ShellProfile { name: "Windows PowerShell".into(), command: "powershell.exe".into(), icon: "\u{1F4DF}".into() });
        }
        if exe_in_path("pwsh.exe") {
            shells.push(ShellProfile { name: "PowerShell".into(), command: "pwsh.exe".into(), icon: "\u{1F4DF}".into() });
        }
        if exe_in_path("wsl.exe") {
            shells.push(ShellProfile { name: "WSL".into(), command: "wsl.exe".into(), icon: "\u{1F427}".into() });
        }
        if exe_in_path("bash.exe") {
            shells.push(ShellProfile { name: "Bash".into(), command: "bash.exe".into(), icon: "\u{1F40D}".into() });
        }
    } else {
        if cfg!(target_os = "macos") {
            if exe_in_path("zsh") {
                shells.push(ShellProfile { name: "Zsh".into(), command: "zsh".into(), icon: "\u{1F334}".into() });
            }
        }
        if exe_in_path("bash") {
            shells.push(ShellProfile { name: "Bash".into(), command: "bash".into(), icon: "\u{1F40D}".into() });
        }
        if exe_in_path("sh") {
            shells.push(ShellProfile { name: "Sh".into(), command: "sh".into(), icon: "\u{1F40D}".into() });
        }
        if cfg!(not(target_os = "macos")) {
            if exe_in_path("zsh") {
                shells.push(ShellProfile { name: "Zsh".into(), command: "zsh".into(), icon: "\u{1F334}".into() });
            }
        }
        if exe_in_path("fish") {
            shells.push(ShellProfile { name: "Fish".into(), command: "fish".into(), icon: "\u{1F41F}".into() });
        }
    }

    shells
}

#[tauri::command]
pub fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le_bom(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    fn utf16le(s: &str) -> Vec<u8> {
        let mut v = Vec::new();
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn parses_utf16le_bom_distros() {
        let out = utf16le_bom("Debian\nUbuntu\n");
        assert_eq!(parse_wsl_distros(&out), vec!["Debian", "Ubuntu"]);
    }

    #[test]
    fn parses_utf16le_without_bom_distros() {
        let out = utf16le("Debian\nUbuntu\n");
        assert_eq!(parse_wsl_distros(&out), vec!["Debian", "Ubuntu"]);
    }

    #[test]
    fn parses_utf8_distros() {
        let out = b"Ubuntu-22.04\n  Arch\n\n";
        assert_eq!(parse_wsl_distros(out), vec!["Ubuntu-22.04", "Arch"]);
    }

    #[test]
    fn parses_utf8_bom_distros() {
        let mut out = vec![0xEF, 0xBB, 0xBF];
        out.extend_from_slice(b"Ubuntu\n");
        assert_eq!(parse_wsl_distros(&out), vec!["Ubuntu"]);
    }

    #[test]
    fn parses_empty_output() {
        assert!(parse_wsl_distros(b"").is_empty());
        assert!(parse_wsl_distros(b"\n\n").is_empty());
    }

    #[test]
    fn file_connect_config_sftp_deserializes() {
        let cfg: FileConnectConfig = serde_json::from_str(
            r#"{"type":"sftp","host":"h","port":22,"username":"u","password":"p","private_key_path":"/k"}"#,
        )
        .unwrap();
        match cfg {
            FileConnectConfig::Sftp { host, port, username, password, private_key_path } => {
                assert_eq!(host, "h");
                assert_eq!(port, 22);
                assert_eq!(username, "u");
                assert_eq!(password, "p");
                assert_eq!(private_key_path.as_deref(), Some("/k"));
            }
            _ => panic!("expected sftp config"),
        }
    }

    #[test]
    fn file_connect_config_adb_deserializes() {
        let cfg: FileConnectConfig =
            serde_json::from_str(r#"{"type":"adb","serial":"emulator-5554"}"#).unwrap();
        match cfg {
            FileConnectConfig::Adb { serial } => assert_eq!(serial, "emulator-5554"),
            _ => panic!("expected adb config"),
        }
    }
}
