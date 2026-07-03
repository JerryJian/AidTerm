use tauri::{Manager, State};
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
    host: String,
    port: u16,
    username: String,
    password: String,
    private_key_path: Option<String>,
    rows: u16,
    cols: u16,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    manager.connect_ssh(id.clone(), host, port, username, password, private_key_path, rows, cols, app)?;
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
