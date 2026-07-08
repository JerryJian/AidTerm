use std::io::{Read, Write};
use portable_pty::{PtyPair, MasterPty, PtySize, PtySystem, ChildKiller};
use tauri::{AppHandle, Emitter};

pub struct LocalSession {
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
    pub killer: Box<dyn ChildKiller + Send>,
}

#[cfg(windows)]
fn decode_windows_output(buf: &[u8]) -> String {
    // Try UTF-8 first
    if let Ok(s) = std::str::from_utf8(buf) {
        return s.to_string();
    }
    // Fallback: detect and decode from the system ANSI code page
    // CP936 (GBK) is the most common for Chinese Windows
    // CP1252 for Western European, CP932 for Japanese, etc.
    // Use a best-effort approach with GBK as primary fallback
    let (cow, _, had_errors) = encoding_rs::GBK.decode(buf);
    if !had_errors {
        return cow.to_string();
    }
    let (cow, _, had_errors) = encoding_rs::WINDOWS_1252.decode(buf);
    if !had_errors {
        return cow.to_string();
    }
    String::from_utf8_lossy(buf).to_string()
}

#[cfg(not(windows))]
fn decode_windows_output(buf: &[u8]) -> String {
    String::from_utf8_lossy(buf).to_string()
}

impl LocalSession {
    pub fn spawn(
        id: String,
        rows: u16,
        cols: u16,
        app_handle: AppHandle,
        shell: Option<String>,
    ) -> Result<Self, String> {
        let native_pty = portable_pty::NativePtySystem::default();
        let pair: PtyPair = native_pty
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let master = pair.master;
        let cmd = shell.unwrap_or_else(|| {
            if cfg!(target_os = "windows") { "cmd.exe".into() } else { "bash".into() }
        });

        let child = pair
            .slave
            .spawn_command(portable_pty::CommandBuilder::new(cmd))
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;

        let mut reader = master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone reader: {}", e))?;
        let writer = master
            .take_writer()
            .map_err(|e| format!("Failed to get writer: {}", e))?;

        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": id, "status": "connected",
        }));

        let sid = id.clone();
        let app_h = app_handle.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = decode_windows_output(&buf[..n]);
                        let _ = app_h.emit("terminal-output", serde_json::json!({
                            "session_id": sid, "data": data,
                        }));
                    }
                    Err(_) => break,
                }
            }
            let _ = app_h.emit("terminal-output", serde_json::json!({
                "session_id": sid, "data": "\r\n[Process exited]\r\n",
            }));
            let _ = app_h.emit("session-status", serde_json::json!({
                "session_id": sid, "status": "disconnected",
            }));
        });

        Ok(Self { writer: Box::new(writer), master, killer: child })
    }

    pub fn write(&mut self, data: &str) -> Result<(), String> {
        self.writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Write error: {}", e))
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("Resize error: {}", e))
    }

    pub fn kill(mut self) {
        let _ = self.killer.kill();
    }
}
