use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use ssh_rs::ssh;
use ssh_rs::SshErrorKind;
use ssh_rs::TerminalSize;
use tauri::{AppHandle, Emitter, Manager};
use crate::zmodem;

pub struct SshConnection {
    pub write_tx: Sender<String>,
    kill_tx: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl SshConnection {
    pub fn connect(
        id: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
        rows: u16,
        cols: u16,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let addr = format!("{}:{}", host, port);

        let (write_tx, write_rx): (Sender<String>, _) = mpsc::channel();
        let (kill_tx, kill_rx): (Sender<()>, _) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let result = Self::run_session(
                &addr, &username, &password, private_key_path, rows, cols,
                write_rx, kill_rx, &app_handle, &id,
            );
            if let Err(e) = result {
                let _ = app_handle.emit("terminal-output", serde_json::json!({
                    "session_id": id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                }));
            }
        });

        Ok(Self { write_tx, kill_tx: Some(kill_tx), handle: Some(handle) })
    }

    fn run_session(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        rows: u16,
        cols: u16,
        write_rx: Receiver<String>,
        kill_rx: Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let mut builder = ssh::create_session()
            .username(username)
            .timeout(Some(Duration::from_secs(10)));

        if let Some(ref key_path) = private_key_path {
            builder = builder.private_key_path(key_path);
        }

        builder = builder.password(password);

        let mut session = builder.connect(addr)
            .map_err(|e| format!("SSH connect failed: {}", e))?
            .run_local();

        let mut shell = session
            .open_shell_terminal(TerminalSize::from(cols as u32, rows as u32))
            .map_err(|e| format!("SSH shell open failed: {}", e))?;

        session.set_timeout(Some(Duration::from_millis(30)));

        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            while let Ok(data) = write_rx.try_recv() {
                shell.write(data.as_bytes()).map_err(|e| format!("SSH write error: {}", e))?;
            }

            match shell.read() {
                Ok(data) => {
                    if data.is_empty() {
                        break;
                    }

                    // Zmodem detection
                    if zmodem::detect_init(&data) {
                        let _ = app_handle.emit("zmodem-start", serde_json::json!({
                            "session_id": session_id,
                        }));

                        let mut buf: Vec<u8> = data.to_vec();

                        // Wait for frontend response (poll)
                        let action = loop {
                            if kill_rx.try_recv().is_ok() {
                                return Ok(());
                            }
                            if let Some(state) = app_handle.try_state::<zmodem::ZmodemState>() {
                                if let Ok(responses) = state.responses.lock() {
                                    if let Some(response) = responses.get(session_id) {
                                        if let Some(path) = response {
                                            break Some(path.clone());
                                        }
                                    }
                                }
                            }
                            while let Ok(w) = write_rx.try_recv() {
                                shell.write(w.as_bytes()).map_err(|e| format!("SSH write error: {}", e))?;
                            }
                            match shell.read() {
                                Ok(more) if !more.is_empty() => buf.extend_from_slice(&more),
                                Ok(_) => {}
                                Err(e) if matches!(e.kind(), SshErrorKind::Timeout) => {}
                                Err(e) => return Err(e.to_string()),
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        };

                        if let Some(ref path) = action {
                            // Capture remaining Zmodem data with idle timeout
                            let mut idle = 0u32;
                            loop {
                                if kill_rx.try_recv().is_ok() {
                                    return Ok(());
                                }
                                while let Ok(w) = write_rx.try_recv() {
                                    let _ = shell.write(w.as_bytes());
                                }
                                match shell.read() {
                                    Ok(more) if !more.is_empty() => {
                                        buf.extend_from_slice(&more);
                                        idle = 0;
                                    }
                                    Ok(_) | Err(_) => {
                                        idle += 1;
                                        if idle > 50 {
                                            break;
                                        }
                                        std::thread::sleep(Duration::from_millis(100));
                                    }
                                }
                            }

                            if let Err(e) = zmodem::save_to_file(path, &buf) {
                                let _ = app_handle.emit("zmodem-end", serde_json::json!({
                                    "session_id": session_id, "error": e,
                                }));
                            } else {
                                let _ = app_handle.emit("zmodem-end", serde_json::json!({
                                    "session_id": session_id,
                                }));
                            }
                        } else {
                            let _ = app_handle.emit("zmodem-end", serde_json::json!({
                                "session_id": session_id,
                            }));
                        }

                        // Clean up response entry
                        if let Some(state) = app_handle.try_state::<zmodem::ZmodemState>() {
                            let _ = state.responses.lock().map(|mut m| m.remove(session_id));
                        }

                        continue;
                    }

                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": session_id, "data": String::from_utf8_lossy(&data),
                    }));
                }
                Err(e) => {
                    if matches!(e.kind(), SshErrorKind::Timeout) {
                        continue;
                    }
                    return Err(e.to_string());
                }
            }
        }

        let _ = shell.close();
        session.close();
        Ok(())
    }

    pub fn write(&self, data: &str) -> Result<(), String> {
        self.write_tx.send(data.to_string()).map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn resize(&mut self, _rows: u16, _cols: u16) -> Result<(), String> {
        Ok(())
    }

    pub fn kill(&mut self) {
        if let Some(tx) = self.kill_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
