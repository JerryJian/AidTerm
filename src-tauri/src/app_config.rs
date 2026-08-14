use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "aidterm-config.json";
const IDENTIFIER: &str = "com.jwlsn.aidterm";

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default = "default_single_instance")]
    single_instance: bool,
}

fn default_single_instance() -> bool {
    true
}

fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library").join("Application Support"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
            })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = CONFIG_FILE;
        None
    }
}

fn config_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(IDENTIFIER).join(CONFIG_FILE))
}

fn read_config(path: &std::path::Path) -> Option<AppConfig> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn single_instance_enabled() -> bool {
    match config_path() {
        Some(path) => read_config(&path)
            .map(|config| config.single_instance)
            .unwrap_or(true),
        None => true,
    }
}

fn set_single_instance_enabled(enabled: bool) -> Result<(), String> {
    let path = config_path().ok_or_else(|| "Failed to resolve app config directory".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let config = AppConfig {
        single_instance: enabled,
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub fn get_single_instance() -> bool {
    single_instance_enabled()
}

#[tauri::command]
pub fn set_single_instance(enabled: bool) -> Result<(), String> {
    set_single_instance_enabled(enabled)
}
