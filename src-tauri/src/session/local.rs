use std::io::{Read, Write};
use std::time::Duration;
use portable_pty::{PtyPair, MasterPty, PtySize, PtySystem, ChildKiller};
use tauri::{AppHandle, Emitter};

pub struct LocalSession {
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
    pub killer: Box<dyn ChildKiller + Send>,
    #[cfg_attr(not(unix), allow(dead_code))]
    pub pid: Option<u32>,
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
        working_dir: Option<String>,
    ) -> Result<Self, String> {
        let native_pty = portable_pty::NativePtySystem::default();
        let pair: PtyPair = native_pty
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let master = pair.master;
        let cmd = shell.unwrap_or_else(|| {
            if cfg!(target_os = "windows") {
                std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| {
                    if cfg!(target_os = "macos") { "zsh".into() } else { "bash".into() }
                })
            }
        });

        let mut cmd_builder = portable_pty::CommandBuilder::new(cmd);
        if let Some(ref wd) = working_dir {
            cmd_builder.cwd(wd);
        }
        cmd_builder.env("TERM", "xterm-256color");
        // Ensure UTF-8 locale so shells handle multi-byte characters correctly
        if ["LANG", "LC_ALL", "LC_CTYPE"].iter().all(|k| std::env::var(k).is_err()) {
            cmd_builder.env("LANG", "C.UTF-8");
        }
        let child = pair
            .slave
            .spawn_command(cmd_builder)
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
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = decode_windows_output(&buf[..n]);
                        let _ = app_h.emit("terminal-output", serde_json::json!({
                            "session_id": sid, "data": data,
                        }));
                    }
                }
            }
        });

        // Watcher thread: owns the child and polls for its exit. On Windows the
        // ConPTY output pipe never signals EOF when the shell exits, so the
        // reader thread alone can never detect a disconnect. Detecting the
        // child process exit here works on all platforms.
        let killer = child.clone_killer();
        let child_pid = child.process_id();
        let watch_sid = id.clone();
        let watch_app = app_handle.clone();
        std::thread::spawn(move || {
            let mut child = child;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) | Err(_) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
            let _ = watch_app.emit("terminal-output", serde_json::json!({
                "session_id": watch_sid, "data": "\r\n[Process exited]\r\n",
            }));
            let _ = watch_app.emit("session-status", serde_json::json!({
                "session_id": watch_sid, "status": "disconnected",
            }));
        });

        Ok(Self { writer: Box::new(writer), master, killer, pid: child_pid })
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
        #[cfg(unix)]
        {
            if let Some(pid) = self.pid {
                unsafe {
                    // Signal the foreground process group (e.g. vim/top) directly,
                    // then the shell's own group so it can clean up its remaining jobs.
                    let fg = self.master.process_group_leader();
                    if let Some(fg) = fg.filter(|&fg| fg != pid as libc::pid_t) {
                        libc::kill(-fg, libc::SIGHUP);
                    }
                    libc::kill(-(pid as libc::pid_t), libc::SIGHUP);
                }
                // Give the shell a short grace period to terminate its children,
                // then force-kill the group as a fallback for unresponsive shells.
                std::thread::sleep(Duration::from_millis(300));
                unsafe {
                    let fg = self.master.process_group_leader();
                    if let Some(fg) = fg.filter(|&fg| fg != pid as libc::pid_t) {
                        libc::kill(-fg, libc::SIGKILL);
                    }
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
                return;
            }
        }
        let _ = self.killer.kill();
    }
}
