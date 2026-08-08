use std::collections::HashSet;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::sftp::FileEntry;

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
/// Emulators are visible automatically (the server discovers local emulator
/// transports on its own), so no extra port scanning is needed.
pub fn list_devices(app: &AppHandle) -> Result<Vec<AdbDevice>, String> {
    ensure_server(app)?;
    let (stdout, _, ok) = run_adb(app, &["devices", "-l"])?;
    if !ok {
        return Err("adb devices failed".to_string());
    }
    Ok(parse_devices(&stdout))
}

/// USB devices held by the user's own adb server (default port 5037).
///
/// A physical USB device can only be claimed by one adb server at a time, so
/// anything already grabbed by the user's 5037 server will never show up on our
/// isolated 5038 server. We query the user's server strictly read-only over the
/// raw adb wire protocol (never via the adb binary, whose client-version check
/// would kill + restart the user's server on mismatch) and return the serials
/// we cannot see.
pub fn occupied_devices(app: &AppHandle) -> Vec<String> {
    let own: HashSet<String> = list_devices(app)
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.serial)
        .collect();
    query_5037_devices()
        .into_iter()
        .filter(|d| is_usb_serial(&d.serial) && !own.contains(&d.serial))
        .map(|d| d.serial)
        .collect()
}

fn is_usb_serial(serial: &str) -> bool {
    !serial.starts_with("emulator-") && !serial.contains(':')
}

/// Minimal, strictly read-only query of the user's default adb server (5037).
///
/// We intentionally do NOT shell out to the adb binary here: when the client
/// version differs from the running server's, `adb` prints "server version does
/// not match client" and kills + restarts the server. Speaking the
/// `host:devices-l` wire protocol directly keeps this read-only, so the user's
/// server is never touched. Returns an empty list when no server is listening.
fn query_5037_devices() -> Vec<AdbDevice> {
    use std::io::{Read, Write};
    use std::net::ToSocketAddrs;

    let Ok(mut addrs) = ("127.0.0.1", 5037u16).to_socket_addrs() else {
        return Vec::new();
    };
    let Some(addr) = addrs.next() else {
        return Vec::new();
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else {
        return Vec::new();
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

    let msg = b"host:devices-l";
    let mut req = Vec::with_capacity(8 + msg.len());
    req.extend_from_slice(format!("{:04x}", msg.len()).as_bytes());
    req.extend_from_slice(msg);
    if stream.write_all(&req).is_err() {
        return Vec::new();
    }

    let mut header = [0u8; 4];
    if stream.read_exact(&mut header).is_err() {
        return Vec::new();
    }
    if &header == b"FAIL" {
        return Vec::new();
    }
    if &header != b"OKAY" {
        return Vec::new();
    }

    let mut lenbuf = [0u8; 4];
    if stream.read_exact(&mut lenbuf).is_err() {
        return Vec::new();
    }
    let Ok(len) = usize::from_str_radix(String::from_utf8_lossy(&lenbuf).trim(), 16) else {
        return Vec::new();
    };
    let mut payload = vec![0u8; len];
    if stream.read_exact(&mut payload).is_err() {
        return Vec::new();
    }
    parse_devices(&String::from_utf8_lossy(&payload))
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

// ══════════════════════════════════════════════════════
//  ADB file operations (Android file browser)
//
//  All calls stay pinned to the isolated 5038 server via `-P 5038`.
//  Paths coming from the device are quoted with `shq` before they go through
//  the on-device shell; `pull`/`push` take paths as plain argv entries so they
//  need no quoting.
// ══════════════════════════════════════════════════════

/// Single-quote a string for the on-device mksh shell.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Build `adb shell <cmd...>` with the given shell-quoted arguments.
fn run_adb_shell(app: &AppHandle, serial: &str, quoted_parts: &[&str]) -> Result<(String, String, bool), String> {
    let mut args = vec!["-s", serial, "shell"];
    args.extend_from_slice(quoted_parts);
    run_adb(app, &args)
}

/// Parse `ls -la` output (toybox / GNU). Handles names with spaces and
/// symlink targets; `.` / `..` entries are dropped.
fn parse_ls_entries(output: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 7 {
            continue;
        }
        let perms = tokens[0];
        let is_dir = perms.starts_with('d');
        // Date is `YYYY-MM-DD HH:MM` (toybox, 2 tokens) or `MMM d HH:MM|YYYY`
        // (GNU, 3 tokens). Name starts right after the date block.
        let name_start = if tokens[5].contains('-') { 7 } else { 8 };
        if tokens.len() <= name_start {
            continue;
        }
        let mut name = tokens[name_start..].join(" ");
        if perms.starts_with('l') {
            if let Some(pos) = name.find(" -> ") {
                name.truncate(pos);
            }
        }
        if name == "." || name == ".." {
            continue;
        }
        entries.push(FileEntry {
            name,
            is_dir,
            size: tokens[4].parse().unwrap_or(0),
            modified: tokens[5..name_start].join(" "),
            permissions: perms.to_string(),
        });
    }
    entries
}

/// List a directory on the device.
pub fn list_dir(app: &AppHandle, serial: &str, path: &str) -> Result<Vec<FileEntry>, String> {
    let q = shq(path);
    let (stdout, stderr, ok) = run_adb_shell(app, serial, &["ls", "-la", &q])?;
    if !ok {
        return Err(format!("adb ls failed: {}", stderr.trim()));
    }
    Ok(parse_ls_entries(&stdout))
}

/// Pull a remote file/directory to a local path.
pub fn pull(app: &AppHandle, serial: &str, remote: &str, local: &str) -> Result<(), String> {
    let (stdout, stderr, ok) = run_adb(app, &["-s", serial, "pull", remote, local])?;
    if !ok {
        return Err(format!("adb pull failed: {}{}", stderr.trim(), stdout.trim()));
    }
    Ok(())
}

/// Push a local file/directory to a remote path.
pub fn push(app: &AppHandle, serial: &str, local: &str, remote: &str) -> Result<(), String> {
    let (stdout, stderr, ok) = run_adb(app, &["-s", serial, "push", local, remote])?;
    if !ok {
        return Err(format!("adb push failed: {}{}", stderr.trim(), stdout.trim()));
    }
    Ok(())
}

/// Create a directory (with parents) on the device.
pub fn mkdir(app: &AppHandle, serial: &str, path: &str) -> Result<(), String> {
    let q = shq(path);
    let (_, stderr, ok) = run_adb_shell(app, serial, &["mkdir", "-p", &q])?;
    if !ok {
        return Err(format!("adb mkdir failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Create an empty file on the device.
pub fn touch(app: &AppHandle, serial: &str, path: &str) -> Result<(), String> {
    let q = shq(path);
    let (_, stderr, ok) = run_adb_shell(app, serial, &["touch", &q])?;
    if !ok {
        return Err(format!("adb touch failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Remove a file (`rm -f`) or directory tree (`rm -rf`) on the device.
pub fn remove(app: &AppHandle, serial: &str, path: &str, is_dir: bool) -> Result<(), String> {
    let q = shq(path);
    let flag = if is_dir { "-rf" } else { "-f" };
    let (_, stderr, ok) = run_adb_shell(app, serial, &["rm", flag, &q])?;
    if !ok {
        return Err(format!("adb rm failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Rename/move a file or directory on the device.
pub fn rename_item(app: &AppHandle, serial: &str, old_path: &str, new_path: &str) -> Result<(), String> {
    let old = shq(old_path);
    let new = shq(new_path);
    let (_, stderr, ok) = run_adb_shell(app, serial, &["mv", &old, &new])?;
    if !ok {
        return Err(format!("adb mv failed: {}", stderr.trim()));
    }
    Ok(())
}

/// Read a text file from the device.
pub fn read_file(app: &AppHandle, serial: &str, remote: &str) -> Result<String, String> {
    let q = shq(remote);
    let (stdout, stderr, ok) = run_adb_shell(app, serial, &["cat", &q])?;
    if !ok {
        return Err(format!("adb cat failed: {}", stderr.trim()));
    }
    Ok(stdout)
}

/// Write a text file to the device by pushing a temp file (binary-safe).
pub fn write_file(app: &AppHandle, serial: &str, remote: &str, content: &str) -> Result<(), String> {
    use std::io::Write;

    let path = std::env::temp_dir().join(format!("aidterm_upload_{}.tmp", uuid::Uuid::new_v4()));
    let write_res = (|| -> Result<(), String> {
        let mut f = std::fs::File::create(&path).map_err(|e| format!("temp file create failed: {}", e))?;
        f.write_all(content.as_bytes()).map_err(|e| format!("temp file write failed: {}", e))?;
        Ok(())
    })();
    if let Err(e) = write_res {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    let res = push(app, serial, &path.to_string_lossy(), remote);
    let _ = std::fs::remove_file(&path);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ls_toybox_with_spaces() {
        let out = "\
total 16
-rw-rw---- 1 u0_a198 media_rw 6 2026-08-08 13:39 hello world.txt
-rw-rw---- 1 u0_a198 media_rw 3 2026-08-08 13:39 plain.txt
";
        let entries = parse_ls_entries(out);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "hello world.txt");
        assert_eq!(entries[0].size, 6);
        assert_eq!(entries[0].is_dir, false);
        assert_eq!(entries[0].modified, "2026-08-08 13:39");
        assert_eq!(entries[0].permissions, "-rw-rw----");
    }

    #[test]
    fn parse_ls_dirs_and_dots() {
        let out = "\
drwxrws--- 2 u0_a198 media_rw 4096 2026-08-08 13:39 .
drwxrws--- 3 u0_a198 media_rw 4096 2026-08-08 13:39 ..
drwxrws--- 2 u0_a198 media_rw 4096 2026-08-08 13:39 test
";
        let entries = parse_ls_entries(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test");
        assert_eq!(entries[0].is_dir, true);
    }

    #[test]
    fn parse_ls_symlink_target_stripped() {
        let out = "lrwxr-xr-x 1 root root 21 2009-01-01 00:00 /sdcard -> /storage/self/primary\n";
        let entries = parse_ls_entries(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "/sdcard");
        assert_eq!(entries[0].is_dir, false);
        assert_eq!(entries[0].permissions, "lrwxr-xr-x");
    }

    #[test]
    fn parse_ls_gnu_three_token_date() {
        let out = "-rw-r--r-- 1 user group 1024 Aug  8 2026 readme.md\n";
        let entries = parse_ls_entries(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "readme.md");
        assert_eq!(entries[0].modified, "Aug 8 2026");
    }
}
