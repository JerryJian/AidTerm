use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use ssh_rs::ssh;
use ssh_rs::SshErrorKind;
use ssh_rs::TerminalSize;
use tauri::{AppHandle, Emitter};

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
        rows: u16,
        cols: u16,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let addr = format!("{}:{}", host, port);

        let (write_tx, write_rx): (Sender<String>, _) = mpsc::channel();
        let (kill_tx, kill_rx): (Sender<()>, _) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let result = Self::run_session(
                &addr, &username, &password, rows, cols,
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
        rows: u16,
        cols: u16,
        write_rx: Receiver<String>,
        kill_rx: Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let mut session = ssh::create_session()
            .username(username)
            .password(password)
            .timeout(Some(Duration::from_secs(10)))
            .connect(addr)
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
