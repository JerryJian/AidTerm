use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedSession {
    pub id: String,
    pub name: String,
    pub session_type: String,
    pub group_id: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub last_connected: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedSessionGroup {
    pub id: String,
    pub name: String,
    pub expanded: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SessionStoreData {
    pub groups: Vec<SavedSessionGroup>,
    pub sessions: Vec<SavedSession>,
}

fn store_path(app_data_dir: PathBuf) -> PathBuf {
    app_data_dir.join("sessions.json")
}

pub fn load(app_data_dir: PathBuf) -> Result<SessionStoreData, String> {
    let path = store_path(app_data_dir);
    if !path.exists() {
        return Ok(SessionStoreData::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read sessions: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse sessions: {}", e))
}

pub fn save(app_data_dir: PathBuf, data: &SessionStoreData) -> Result<(), String> {
    fs::create_dir_all(&app_data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    let path = store_path(app_data_dir);
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize sessions: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write sessions: {}", e))
}
