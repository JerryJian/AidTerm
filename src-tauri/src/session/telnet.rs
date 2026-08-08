use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use telnet::{Telnet, Event};
use tauri::{AppHandle, Emitter};
use crate::session::{Connection, Capability};

pub struct TelnetConnection {
    pub write_tx: Sender<String>,
    kill_tx: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl TelnetConnection {
    pub fn connect(
        id: String,
        host: String,
        port: u16,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let (write_tx, write_rx): (Sender<String>, _) = mpsc::channel();
        let (kill_tx, kill_rx): (Sender<()>, _) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let result = Self::run_session(
                &host, port, write_rx, kill_rx, &app_handle, &id,
            );
            if let Err(e) = &result {
                let _ = app_handle.emit("terminal-output", serde_json::json!({
                    "session_id": id, "data": format!("\r\n[Telnet Error: {}]\r\n", e),
                }));
            }
            // Notify on every session end (normal close or error) so the
            // frontend disconnect overlay always appears.
            let _ = app_handle.emit("session-status", serde_json::json!({
                "session_id": id, "status": "disconnected",
            }));
        });

        Ok(Self { write_tx, kill_tx: Some(kill_tx), handle: Some(handle) })
    }

    fn run_session(
        host: &str,
        port: u16,
        write_rx: Receiver<String>,
        kill_rx: Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let mut conn = Telnet::connect((host, port), 4096)
            .map_err(|e| format!("Telnet connect failed: {}", e))?;

        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": session_id, "status": "connected",
        }));
        let _ = app_handle.emit("terminal-output", serde_json::json!({
            "session_id": session_id,
            "data": format!("\r\n[Trying {}:{}... connected]\r\n", host, port),
        }));

        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            while let Ok(data) = write_rx.try_recv() {
                conn.write(data.as_bytes())
                    .map_err(|e| format!("Telnet write error: {}", e))?;
            }

            match conn.read_timeout(Duration::from_millis(30)) {
                Ok(Event::Data(data)) => {
                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": session_id,
                        "data": String::from_utf8_lossy(&data),
                    }));
                }
                Ok(Event::TimedOut) | Ok(Event::NoData) => continue,
                Ok(Event::Error(e)) => {
                    return Err(format!("Telnet error: {}", e));
                }
                Ok(_) => continue,
                Err(e) => {
                    return Err(format!("Telnet read error: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn write(&self, data: &str) -> Result<(), String> {
        self.write_tx.send(data.to_string()).map_err(|e| e.to_string())
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

impl Connection for TelnetConnection {
    fn write(&mut self, data: &str) -> Result<(), String> {
        TelnetConnection::write(self, data)
    }

    fn resize(&self, _rows: u16, _cols: u16) -> Result<(), String> {
        Ok(())
    }

    fn kill(&mut self) {
        self.kill()
    }

    fn capabilities(&self) -> &'static [Capability] {
        crate::session::CAP_NONE
    }
}
