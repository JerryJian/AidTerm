pub(crate) mod local;
pub(crate) mod ssh;
pub(crate) mod telnet;

use std::collections::HashMap;
use std::sync::Mutex;
use futures::future::BoxFuture;
use tauri::AppHandle;
use crate::serial;
use crate::proxy;
use crate::adb;

/// Declarative capability of a connection. Drives which tool panels the
/// frontend shows for a tab (file browser, port forwarding, exec, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    File,
    Tunnel,
    Exec,
    Zmodem,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::File => "file",
            Capability::Tunnel => "tunnel",
            Capability::Exec => "exec",
            Capability::Zmodem => "zmodem",
        }
    }
}

pub const CAP_FILE: &[Capability] = &[Capability::File];
pub const CAP_FILE_TUNNEL_EXEC_ZMODEM: &[Capability] = &[Capability::File, Capability::Tunnel, Capability::Exec, Capability::Zmodem];
pub const CAP_NONE: &[Capability] = &[];

/// Unified connection creation config, dispatched by the `type` tag.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionConfig {
    Local {
        shell: Option<String>,
        working_dir: Option<String>,
    },
    Wsl {
        distro: Option<String>,
        working_dir: Option<String>,
    },
    Ssh {
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
        proxy_id: Option<String>,
        agent_forwarding: bool,
        x11_forwarding: bool,
    },
    Telnet {
        host: String,
        port: u16,
    },
    Serial {
        port_name: String,
        baud_rate: u32,
        data_bits: u8,
        stop_bits: u8,
        parity: String,
        flow_control: String,
    },
    Adb {
        serial: String,
    },
}

/// Every connection type implements this interface; `SessionManager` only
/// ever talks through it, so adding a new connection type is a new module
/// plus one arm in `SessionManager::create`.
pub trait Connection: Send {
    fn write(&mut self, data: &str) -> Result<(), String>;
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String>;
    fn kill(&mut self);

    /// Run a non-interactive command and capture its output (SSH only today).
    fn exec(&self, _cmd: &str) -> BoxFuture<'static, Result<String, String>> {
        Box::pin(async { Err("Exec not supported for this session type".to_string()) })
    }

    /// Static capabilities advertised by this connection type.
    fn capabilities(&self) -> &'static [Capability];
}

pub struct SessionManager {
    sessions: Mutex<HashMap<String, Box<dyn Connection>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self { sessions: Mutex::new(HashMap::new()) }
    }

    /// The single connection factory. Add a new connection type here (and in
    /// the `ConnectionConfig` enum) — nothing else in the codebase needs to
    /// know about individual types.
    pub fn create(
        &self,
        app_handle: AppHandle,
        config: ConnectionConfig,
        rows: u16,
        cols: u16,
        proxy: Option<proxy::ProxyConfig>,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let session: Box<dyn Connection> = match config {
            ConnectionConfig::Local { shell, working_dir } => Box::new(
                local::LocalSession::spawn(id.clone(), rows, cols, app_handle, shell, working_dir, Vec::new(), CAP_NONE)?,
            ),
            ConnectionConfig::Wsl { distro, working_dir } => {
                if !cfg!(target_os = "windows") {
                    return Err("WSL is only available on Windows".to_string());
                }
                let args = distro.map(|d| vec!["-d".to_string(), d]).unwrap_or_default();
                Box::new(
                    local::LocalSession::spawn(id.clone(), rows, cols, app_handle, Some("wsl.exe".to_string()), working_dir, args, CAP_NONE)?,
                )
            }
            ConnectionConfig::Ssh {
                host,
                port,
                username,
                password,
                private_key_path,
                proxy_id: _,
                agent_forwarding,
                x11_forwarding,
            } => Box::new(
                ssh::SshConnection::connect(
                    id.clone(), host, port, username, password, private_key_path,
                    proxy, rows, cols, agent_forwarding, x11_forwarding, app_handle,
                )?,
            ),
            ConnectionConfig::Telnet { host, port } => Box::new(
                telnet::TelnetConnection::connect(id.clone(), host, port, app_handle)?,
            ),
            ConnectionConfig::Serial {
                port_name,
                baud_rate,
                data_bits,
                stop_bits,
                parity,
                flow_control,
            } => Box::new(
                serial::SerialConnection::connect(
                    id.clone(),
                    serial::SerialConfig {
                        port_name,
                        baud_rate,
                        data_bits,
                        stop_bits,
                        parity,
                        flow_control,
                    },
                    app_handle,
                )?,
            ),
            ConnectionConfig::Adb { serial } => {
                adb::ensure_server(&app_handle)?;
                let adb_bin = adb::adb_path(&app_handle)?.to_string_lossy().to_string();
                let args = vec![
                    "-P".to_string(),
                    adb::ADB_PORT.to_string(),
                    "-s".to_string(),
                    serial,
                    "shell".to_string(),
                ];
                Box::new(local::LocalSession::spawn(id.clone(), rows, cols, app_handle, Some(adb_bin), None, args, CAP_FILE)?)
            }
        };
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(id.clone(), session);
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get_mut(id).ok_or("Session not found")?;
        session.write(data)
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get(id).ok_or("Session not found")?;
        session.resize(rows, cols)
    }

    pub async fn exec(&self, id: &str, command: &str) -> Result<String, String> {
        let command = command.to_string();
        let fut = {
            let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
            let session = sessions.get(id).ok_or("Session not found")?;
            session.exec(&command)
        };
        fut.await
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(mut session) = sessions.remove(id) {
            session.kill();
        }
        Ok(())
    }

    pub fn capabilities(&self, id: &str) -> Vec<&'static str> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string()).ok();
        match sessions {
            Some(s) => s.get(id).map(|s| s.capabilities().iter().map(|c| c.as_str()).collect()).unwrap_or_default(),
            None => Vec::new(),
        }
    }
}
