use tauri::{Manager, State};
use crate::session::SessionManager;
use crate::session_store;

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
