use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Fixed, app-wide ADB server port, isolated from the user's default 5037.
///
/// AidTerm NEVER touches the user's adb server. Every adb subprocess call is
/// forced onto this port with `-P 5038`, so a version mismatch between the
/// bundled adb and the running server can only ever kill AidTerm's own
/// isolated server, never the one the user may have started for other tools.
pub const ADB_PORT: &str = "5038";

#[derive(Debug, Clone, Serialize)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: String,
    pub product: String,
    pub transport_id: Option<String>,
}

/// Resolve the adb binary to use, in priority order:
///   1. `AIDTERM_ADB` env var (explicit override, dev / power users)
///   2. Bundled resource `bin/adb(.exe)` shipped inside the app
///   3. adb found on PATH (fallback)
pub fn adb_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("AIDTERM_ADB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        log::warn!("[adb] AIDTERM_ADB set but not a file, ignoring: {}", p.display());
    }

    let exe_name = if cfg!(target_os = "windows") { "adb.exe" } else { "adb" };
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("bin").join(exe_name);
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    if let Some(p) = find_in_path(exe_name) {
        return Ok(p);
    }

    Err("adb not found. Set AIDTERM_ADB to a platform-tools adb path, or add adb to PATH.".to_string())
}

fn find_in_path(exe_name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe_name))
            .find(|p| p.is_file())
    })
}

/// Build an adb Command pinned to the isolated 5038 server.
#[cfg(target_os = "windows")]
fn adb_command(app: &AppHandle) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(adb_path(app)?);
    cmd.arg("-P").arg(ADB_PORT);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    Ok(cmd)
}

#[cfg(not(target_os = "windows"))]
fn adb_command(app: &AppHandle) -> Result<Command, String> {
    let mut cmd = Command::new(adb_path(app)?);
    cmd.arg("-P").arg(ADB_PORT);
    Ok(cmd)
}

fn run_adb(app: &AppHandle, args: &[&str]) -> Result<(String, String, bool), String> {
    let mut cmd = adb_command(app)?;
    cmd.args(args);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((stdout, stderr, output.status.success()))
}

/// Make sure an adb server is running on the isolated 5038 port.
/// `adb devices` transparently starts the server when missing.
pub fn ensure_server(app: &AppHandle) -> Result<(), String> {
    let (stdout, stderr, ok) = run_adb(app, &["devices"])?;
    if !ok {
        return Err(format!("adb server start failed: {}{}", stderr, stdout));
    }
    Ok(())
}

/// Kill the isolated 5038 adb server. The user's own 5037 server is never touched.
pub fn kill_server(app: &AppHandle) -> Result<(), String> {
    let (_, stderr, ok) = run_adb(app, &["kill-server"])?;
    if !ok {
        return Err(format!("adb kill-server failed: {}", stderr));
    }
    Ok(())
}

/// List attached devices via `adb -P 5038 devices -l`.
/// Ensure the server is running first so a freshly connected phone shows up.
pub fn list_devices(app: &AppHandle) -> Result<Vec<AdbDevice>, String> {
    ensure_server(app)?;
    let (stdout, _, ok) = run_adb(app, &["devices", "-l"])?;
    if !ok {
        return Err("adb devices failed".to_string());
    }
    Ok(parse_devices(&stdout))
}

fn parse_devices(output: &str) -> Vec<AdbDevice> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with("* daemon") {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let Some(serial) = tokens.next() else { continue };
        let state = tokens.next().unwrap_or("unknown").to_string();
        let mut model = String::new();
        let mut product = String::new();
        let mut transport_id = None;
        for kv in tokens {
            if let Some((key, value)) = kv.split_once(':') {
                match key {
                    "model" => model = value.to_string(),
                    "product" => product = value.to_string(),
                    "transport_id" => transport_id = Some(value.to_string()),
                    _ => {}
                }
            }
        }
        devices.push(AdbDevice { serial: serial.to_string(), state, model, product, transport_id });
    }
    devices
}
