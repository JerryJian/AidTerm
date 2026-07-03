use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use portable_pty::{PtyPair, PtySize, PtySystem, ChildKiller};
use tauri::{AppHandle, Emitter};

pub struct Session {
    pub writer: Box<dyn Write + Send>,
    pub killer: Box<dyn ChildKiller + Send>,
}

pub struct SessionManager {
    pub sessions: Mutex<HashMap<String, Session>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn spawn_local(&self, id: String, rows: u16, cols: u16, app_handle: AppHandle) -> Result<(), String> {
        let native_pty = portable_pty::NativePtySystem::default();
        let pair: PtyPair = native_pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let cmd = if cfg!(target_os = "windows") {
            "cmd.exe"
        } else {
            "bash"
        };

        let child = pair
            .slave
            .spawn_command(
                portable_pty::CommandBuilder::new(cmd),
            )
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;

        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| format!("Failed to clone reader: {}", e))?;
        let writer = pair.master.take_writer()
            .map_err(|e| format!("Failed to get writer: {}", e))?;

        let session_id = id.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_handle.emit("terminal-output", serde_json::json!({
                            "session_id": session_id,
                            "data": data,
                        }));
                    }
                    Err(_) => break,
                }
            }
            let _ = app_handle.emit("terminal-output", serde_json::json!({
                "session_id": session_id,
                "data": "\r\n[Process exited]\r\n",
            }));
        });

        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(
            id,
            Session {
                writer: Box::new(writer),
                killer: child,
            },
        );

        Ok(())
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get_mut(id).ok_or("Session not found")?;
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("Write error: {}", e))
    }

    #[allow(unused_variables)]
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        Ok(())
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(mut session) = sessions.remove(id) {
            let _ = session.killer.kill();
        }
        Ok(())
    }
}
