use tauri::State;
use crate::session::SessionManager;

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
pub async fn ssh_connect(
    app: tauri::AppHandle,
    manager: State<'_, SessionManager>,
    host: String,
    port: u16,
    username: String,
    password: String,
    rows: u16,
    cols: u16,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    manager.connect_ssh(id.clone(), host, port, username, password, rows, cols, app)?;
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
