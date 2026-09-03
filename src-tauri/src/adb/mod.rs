use std::collections::HashSet;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::sftp::FileEntry;

/// Isolated ADB server port, used only by the adb binary bundled with AidTerm.
///
/// The bundled adb may differ in version from the user's system server, so it
/// runs on its own 5038 server: a version mismatch can only ever kill
/// AidTerm's isolated server, never the user's default 5037 instance.
pub const ADB_PORT: &str = "5038";

/// The default ADB server port used by external/system adb binaries
/// (`AIDTERM_ADB` override or adb found on PATH). Such an adb matches the
/// user's environment, so we talk to the default 5037 server and see the same
/// devices as the user's other adb tools.
pub const ADB_DEFAULT_PORT: &str = "5037";

#[derive(Debug, Clone, Serialize)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: String,
    pub product: String,
    pub transport_id: Option<String>,
}

/// Where the resolved adb binary came from. Only "bundled" uses the isolated
/// 5038 server; every external binary talks to the default 5037 server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbSource {
    Env,
    Bundled,
    Path,
}

impl AdbSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AdbSource::Env => "env",
            AdbSource::Bundled => "bundled",
            AdbSource::Path => "path",
        }
    }
}

/// Probe result surfaced to the ADB connect dialog so the UI can explain why
/// no device list is shown and how to install/point at an adb binary.
#[derive(Debug, Clone, Serialize)]
pub struct AdbStatus {
    pub available: bool,
    /// "env" | "bundled" | "path" | "missing"
    pub source: &'static str,
    pub path: Option<String>,
    /// Server port the resolved adb talks to ("5038" bundled / "5037" external).
    pub port: Option<String>,
}

/// Resolve the adb binary plus the server port it should talk to, in priority
/// order:
///   1. `AIDTERM_ADB` env var (explicit override, dev / power users) -> default 5037
///   2. An adb process already running on the host -> reuse its executable (5037)
///   3. Bundled resource `bin/adb(.exe)` shipped inside the app -> isolated 5038
///   4. adb found on PATH (fallback) -> default 5037
/// Returns `None` when no adb is available (e.g. arm64 packages ship no
/// bundled adb and the system has none installed).
fn resolve_full(app: &AppHandle) -> Option<(PathBuf, &'static str, AdbSource)> {
    if let Ok(p) = std::env::var("AIDTERM_ADB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some((p, ADB_DEFAULT_PORT, AdbSource::Env));
        }
        log::warn!("[adb] AIDTERM_ADB set but not a file, ignoring: {}", p.display());
    }

    // When an adb process is already running, reuse its executable rather than
    // starting AidTerm's own isolated 5038 server; this lets us share the
    // user's already-attached devices (5037) instead of treating them as busy.
    // The process scan result is cached after the dialog first detects it, so
    // the whole session + its cast reuse the same adb without re-enumerating.
    if let Some(p) = cached_running_adb() {
        log::info!("[adb] existing adb process found, reusing: {}", p.display());
        return Some((p, ADB_DEFAULT_PORT, AdbSource::Path));
    }

    let exe_name = if cfg!(target_os = "windows") { "adb.exe" } else { "adb" };
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("bin").join(exe_name);
        if bundled.is_file() {
            return Some((bundled, ADB_PORT, AdbSource::Bundled));
        }
    }

    if let Some(p) = find_in_path(exe_name) {
        return Some((p, ADB_DEFAULT_PORT, AdbSource::Path));
    }

    None
}

/// Cache of a detected running adb executable.
/// Inner: `None` = not probed yet, `Some(None)` = probed, no adb running.
static ADB_RUNNING: OnceLock<Mutex<Option<Option<PathBuf>>>> = OnceLock::new();

fn adb_running_state() -> &'static Mutex<Option<Option<PathBuf>>> {
    ADB_RUNNING.get_or_init(|| Mutex::new(None))
}

/// Return the cached running-adb executable without re-enumerating processes.
fn cached_running_adb() -> Option<PathBuf> {
    adb_running_state().lock().unwrap().clone().flatten()
}

/// Locate the executable of an adb process that is already running on the host
/// and cache it. Only scans the process list when `force` is true (i.e. when
/// the ADB connect dialog lists devices); afterwards every adb call reuses the
/// cached result so the session and its cast share the same adb.
fn refresh_running_adb(force: bool) -> Option<PathBuf> {
    let state = adb_running_state();
    let mut guard = state.lock().unwrap();
    if !force && guard.is_some() {
        return guard.clone().flatten();
    }

    let found = scan_adb_process();
    *guard = Some(found.clone());
    found
}

/// Enumerate running processes looking for `adb` / `adb.exe`. When found,
/// `Process::exe()` yields the concrete binary the running server belongs to;
/// if that path is unavailable/stale we fall back to matching it on PATH so we
/// still can talk to the same 5037 server.
fn scan_adb_process() -> Option<PathBuf> {
    use sysinfo::ProcessesToUpdate;
    let target = if cfg!(target_os = "windows") { "adb.exe" } else { "adb" };
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for proc_ in sys.processes().values() {
        let name = proc_.name();
        let name_matches = std::path::Path::new(name)
            .file_name()
            .map(|f| f.eq_ignore_ascii_case(target))
            .unwrap_or(false);
        if !name_matches {
            continue;
        }
        if let Some(exe) = proc_.exe() {
            if exe.is_file() {
                return Some(exe.to_path_buf());
            }
        }
        if let Some(p) = find_in_path(target) {
            return Some(p);
        }
    }
    None
}

pub fn adb_path(app: &AppHandle) -> Result<(PathBuf, &'static str), String> {
    resolve_full(app)
        .map(|(p, port, _)| (p, port))
        .ok_or_else(|| "adb not found. Set AIDTERM_ADB to a platform-tools adb path, or add adb to PATH.".to_string())
}

/// Probe adb availability for the connect dialog (see `AdbStatus`).
pub fn status(app: &AppHandle) -> AdbStatus {
    // The dialog open is the discovery point: refresh the running-adb cache.
    refresh_running_adb(true);
    match resolve_full(app) {
        Some((path, port, src)) => AdbStatus {
            available: true,
            source: src.as_str(),
            path: Some(path.display().to_string()),
            port: Some(port.to_string()),
        },
        None => AdbStatus {
            available: false,
            source: "missing",
            path: None,
            port: None,
        },
    }
}

fn find_in_path(exe_name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe_name))
            .find(|p| p.is_file())
    })
}

/// Build an adb Command pinned to the server port chosen for the resolved
/// binary: bundled adb -> isolated 5038, external/system adb -> default 5037.
#[cfg(target_os = "windows")]
fn adb_command(app: &AppHandle) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    let (bin, port) = adb_path(app)?;
    let mut cmd = Command::new(bin);
    cmd.arg("-P").arg(port);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    Ok(cmd)
}

#[cfg(not(target_os = "windows"))]
fn adb_command(app: &AppHandle) -> Result<Command, String> {
    let (bin, port) = adb_path(app)?;
    let mut cmd = Command::new(bin);
    cmd.arg("-P").arg(port);
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

/// Make sure an adb server is running on the resolved port (5038 for the
/// bundled adb, 5037 for external/system adb). `adb devices` transparently
/// starts the server when missing.
pub fn ensure_server(app: &AppHandle) -> Result<(), String> {
    let (stdout, stderr, ok) = run_adb(app, &["devices"])?;
    if !ok {
        return Err(format!("adb server start failed: {}{}", stderr, stdout));
    }
    Ok(())
}

/// Kill the adb server AidTerm is using. Only the bundled adb's isolated 5038
/// server is ever stopped; with an external/system adb (5037) this is a no-op
/// because that server belongs to the user and may be used by other tools.
pub fn kill_server(app: &AppHandle) -> Result<(), String> {
    if adb_path(app)?.1 == ADB_DEFAULT_PORT {
        log::info!("[adb] external/system adb (5037), skipping kill-server");
        return Ok(());
    }
    let (_, stderr, ok) = run_adb(app, &["kill-server"])?;
    if !ok {
        return Err(format!("adb kill-server failed: {}", stderr));
    }
    Ok(())
}

/// List attached devices via `adb -P <port> devices -l` (port depends on the
/// resolved binary: 5038 bundled / 5037 external).
/// Ensure the server is running first so a freshly connected phone shows up.
/// Emulators are visible automatically (the server discovers local emulator
/// transports on its own), so no extra port scanning is needed.
pub fn list_devices(app: &AppHandle) -> Result<Vec<AdbDevice>, String> {
    // The device scan is the discovery point for an existing adb process.
    refresh_running_adb(true);
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
/// when AidTerm runs its own isolated 5038 server (bundled adb) anything
/// already grabbed by the user's 5037 server will never show up there. We
/// query the user's server strictly read-only over the raw adb wire protocol
/// (never via the adb binary, whose client-version check would kill + restart
/// the user's server on mismatch) and return the serials we cannot see. With an
/// external/system adb both sides use 5037, so this naturally reports nothing.
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
//  All calls stay pinned to the resolved server port via `-P <port>`
//  (5038 for the bundled adb, 5037 for external/system adb).
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
