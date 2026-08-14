const MENU_LABEL: &str = "在 AidTerm 中打开";
const USER_ENVIRONMENT_KEY: &str = r"Software\Environment";

#[cfg(target_os = "windows")]
const MENU_ENTRIES: [(&str, &str); 3] = [
    (r"Software\Classes\Directory\shell\AidTerm", "%1"),
    (r"Software\Classes\Directory\Background\shell\AidTerm", "%V"),
    (r"Software\Classes\DesktopBackground\Shell\AidTerm", "%V"),
];

#[cfg(target_os = "windows")]
fn command_value(executable: &std::path::Path, target: &str) -> String {
    format!(r#""{}" --cwd "{}""#, executable.display(), target)
}

#[cfg(target_os = "windows")]
pub fn context_menu_enabled() -> Result<bool, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    for (key_path, target) in MENU_ENTRIES {
        let Ok(key) = root.open_subkey(key_path) else {
            return Ok(false);
        };
        let Ok(label) = key.get_value::<String, _>("") else {
            return Ok(false);
        };
        let Ok(command_key) = key.open_subkey("command") else {
            return Ok(false);
        };
        let Ok(command) = command_key.get_value::<String, _>("") else {
            return Ok(false);
        };
        if label != MENU_LABEL || command != command_value(&executable, target) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(target_os = "windows")]
pub fn set_context_menu_enabled(enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let root = RegKey::predef(HKEY_CURRENT_USER);
    if !enabled {
        for (key_path, _) in MENU_ENTRIES {
            let _ = root.delete_subkey_all(key_path);
        }
        return Ok(());
    }

    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let icon = format!(r#""{}",0"#, executable.display());
    let result = (|| -> Result<(), String> {
        for (key_path, target) in MENU_ENTRIES {
            let (key, _) = root.create_subkey(key_path).map_err(|e| e.to_string())?;
            key.set_value("", &MENU_LABEL).map_err(|e| e.to_string())?;
            key.set_value("Icon", &icon).map_err(|e| e.to_string())?;
            let (command_key, _) = key.create_subkey("command").map_err(|e| e.to_string())?;
            command_key
                .set_value("", &command_value(&executable, target))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();

    if result.is_err() {
        for (key_path, _) in MENU_ENTRIES {
            let _ = root.delete_subkey_all(key_path);
        }
    }
    result
}

#[cfg(target_os = "windows")]
fn executable_directory() -> Result<std::path::PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "Failed to resolve AidTerm executable directory".to_string())
}

#[cfg(target_os = "windows")]
fn normalized_path(path: &str) -> String {
    path.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase()
}

#[cfg(target_os = "windows")]
fn update_process_path(directory: &std::path::Path, enabled: bool) {
    let current = std::env::var("PATH").unwrap_or_default();
    let target = normalized_path(&directory.to_string_lossy());
    let mut entries: Vec<String> = current
        .split(';')
        .filter(|entry| !entry.trim().is_empty() && normalized_path(entry) != target)
        .map(str::to_string)
        .collect();
    if enabled {
        entries.push(directory.to_string_lossy().into_owned());
    }
    std::env::set_var("PATH", entries.join(";"));
}

#[cfg(target_os = "windows")]
fn broadcast_environment_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let name: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            std::ptr::null_mut(),
        );
    }
}

#[cfg(target_os = "windows")]
pub fn path_environment_enabled() -> Result<bool, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let directory = executable_directory()?;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = root.open_subkey(USER_ENVIRONMENT_KEY) else {
        return Ok(false);
    };
    let path = key.get_value::<String, _>("Path").unwrap_or_default();
    let target = normalized_path(&directory.to_string_lossy());
    Ok(path.split(';').any(|entry| normalized_path(entry) == target))
}

#[cfg(target_os = "windows")]
pub fn set_path_environment_enabled(enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let directory = executable_directory()?;
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = root.create_subkey(USER_ENVIRONMENT_KEY).map_err(|e| e.to_string())?;
    let current = key.get_value::<String, _>("Path").unwrap_or_default();
    let target = normalized_path(&directory.to_string_lossy());
    let mut entries: Vec<String> = current
        .split(';')
        .filter(|entry| !entry.trim().is_empty() && normalized_path(entry) != target)
        .map(str::to_string)
        .collect();
    if enabled {
        entries.push(directory.to_string_lossy().into_owned());
    }
    key.set_value("Path", &entries.join(";")).map_err(|e| e.to_string())?;
    update_process_path(&directory, enabled);
    broadcast_environment_change();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn context_menu_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(target_os = "windows"))]
pub fn set_context_menu_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Explorer context menu is only supported on Windows".to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn path_environment_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(target_os = "windows"))]
pub fn set_path_environment_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Adding AidTerm to PATH is only supported on Windows".to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn shell_context_menu_get_enabled() -> Result<bool, String> {
    context_menu_enabled()
}

#[tauri::command]
pub fn shell_context_menu_set_enabled(enabled: bool) -> Result<(), String> {
    set_context_menu_enabled(enabled)
}

#[tauri::command]
pub fn path_environment_get_enabled() -> Result<bool, String> {
    path_environment_enabled()
}

#[tauri::command]
pub fn path_environment_set_enabled(enabled: bool) -> Result<(), String> {
    set_path_environment_enabled(enabled)
}
