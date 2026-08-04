use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
    pub flow_control: String,
}

pub struct SerialConnection {
    pub write_tx: tokio::sync::mpsc::UnboundedSender<String>,
    kill_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

fn map_data_bits(bits: u8) -> tokio_serial::DataBits {
    match bits {
        5 => tokio_serial::DataBits::Five,
        6 => tokio_serial::DataBits::Six,
        7 => tokio_serial::DataBits::Seven,
        _ => tokio_serial::DataBits::Eight,
    }
}

fn map_stop_bits(bits: u8) -> tokio_serial::StopBits {
    match bits {
        2 => tokio_serial::StopBits::Two,
        _ => tokio_serial::StopBits::One,
    }
}

fn map_parity(parity: &str) -> tokio_serial::Parity {
    match parity {
        "Odd" => tokio_serial::Parity::Odd,
        "Even" => tokio_serial::Parity::Even,
        _ => tokio_serial::Parity::None,
    }
}

fn map_flow_control(fc: &str) -> tokio_serial::FlowControl {
    match fc {
        "Software" => tokio_serial::FlowControl::Software,
        "Hardware" => tokio_serial::FlowControl::Hardware,
        _ => tokio_serial::FlowControl::None,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SerialPortInfo {
    pub port_name: String,
}

pub fn list_available_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = tokio_serial::available_ports()
        .map_err(|e| format!("Failed to list ports: {}", e))?;
    Ok(ports.into_iter().map(|p| SerialPortInfo { port_name: p.port_name }).collect())
}

impl SerialConnection {
    pub fn connect(
        id: String,
        config: SerialConfig,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();

        let handle = std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Failed to create tokio runtime for serial: {}", e);
                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": id, "data": format!("\r\n[Serial Runtime Error: {}]\r\n", e),
                    }));
                    return;
                }
            };
            let result = rt.block_on(Self::run_session_async(
                &config, write_rx, kill_rx, &app_handle, &id,
            ));
            if let Err(e) = &result {
                log::error!("[serial] Session error ({}): {}", id, e);
                let _ = app_handle.emit("terminal-output", serde_json::json!({
                    "session_id": id, "data": format!("\r\n[Serial Error: {}]\r\n", e),
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

    async fn run_session_async(
        config: &SerialConfig,
        mut write_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
        mut kill_rx: oneshot::Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let builder = tokio_serial::new(&config.port_name, config.baud_rate)
            .data_bits(map_data_bits(config.data_bits))
            .stop_bits(map_stop_bits(config.stop_bits))
            .parity(map_parity(&config.parity))
            .flow_control(map_flow_control(&config.flow_control))
            .timeout(Duration::from_millis(100));

        let mut port = tokio_serial::SerialStream::open(&builder)
            .map_err(|e| format!("Failed to open serial port {}: {}", config.port_name, e))?;

        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": session_id, "status": "connected",
        }));
        let _ = app_handle.emit("terminal-output", serde_json::json!({
            "session_id": session_id,
            "data": format!("\r\n[Connected to {} @ {} baud]\r\n", config.port_name, config.baud_rate),
        }));

        let mut buf = [0u8; 4096];

        loop {
            tokio::select! {
                _ = &mut kill_rx => {
                    break;
                }
                Some(data) = write_rx.recv() => {
                    if let Err(e) = port.write_all(data.as_bytes()).await {
                        log::error!("Serial write error: {}", e);
                    }
                }
                result = port.read(&mut buf) => {
                    match result {
                        Ok(n) if n > 0 => {
                            let _ = app_handle.emit("terminal-output", serde_json::json!({
                                "session_id": session_id,
                                "data": String::from_utf8_lossy(&buf[..n]),
                            }));
                        }
                        _ => {}
                    }
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
