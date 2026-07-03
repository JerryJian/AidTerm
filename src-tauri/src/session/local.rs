use std::io::{Read, Write};
use portable_pty::{PtyPair, PtySize, PtySystem, ChildKiller};
use tauri::{AppHandle, Emitter};

pub struct LocalSession {
    pub writer: Box<dyn Write + Send>,
    pub killer: Box<dyn ChildKiller + Send>,
}

impl LocalSession {
    pub fn spawn(
        id: String,
        rows: u16,
        cols: u16,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let native_pty = portable_pty::NativePtySystem::default();
        let pair: PtyPair = native_pty
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let cmd = if cfg!(target_os = "windows") { "cmd.exe" } else { "bash" };

        let child = pair
            .slave
            .spawn_command(portable_pty::CommandBuilder::new(cmd))
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone reader: {}", e))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to get writer: {}", e))?;

        let sid = id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_handle.emit("terminal-output", serde_json::json!({
                            "session_id": sid, "data": data,
                        }));
                    }
                    Err(_) => break,
                }
            }
            let _ = app_handle.emit("terminal-output", serde_json::json!({
                "session_id": sid, "data": "\r\n[Process exited]\r\n",
            }));
        });

        Ok(Self { writer: Box::new(writer), killer: child })
    }

    pub fn write(&mut self, data: &str) -> Result<(), String> {
        self.writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Write error: {}", e))
    }

    pub fn kill(mut self) {
        let _ = self.killer.kill();
    }
}
