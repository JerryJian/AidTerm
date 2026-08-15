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
    Cast,
    Monitor,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::File => "file",
            Capability::Tunnel => "tunnel",
            Capability::Exec => "exec",
            Capability::Zmodem => "zmodem",
            Capability::Cast => "cast",
            Capability::Monitor => "monitor",
        }
    }
}

pub const CAP_FILE_CAST: &[Capability] = &[Capability::File, Capability::Cast];
pub const CAP_FILE_MONITOR: &[Capability] = &[Capability::File, Capability::Monitor];
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
                local::LocalSession::spawn(id.clone(), rows, cols, app_handle, shell, working_dir, Vec::new(), CAP_FILE_MONITOR)?,
            ),
            ConnectionConfig::Wsl { distro, working_dir } => {
                if !cfg!(target_os = "windows") {
                    return Err("WSL is only available on Windows".to_string());
                }
                let mut args = distro.map(|d| vec!["-d".to_string(), d]).unwrap_or_default();
                if let Some(working_dir) = working_dir.filter(|dir| !dir.trim().is_empty()) {
                    args.extend(["--cd".to_string(), working_dir]);
                }
                Box::new(
                    local::LocalSession::spawn(id.clone(), rows, cols, app_handle, Some("wsl.exe".to_string()), None, args, CAP_FILE_MONITOR)?,
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
                let (adb_bin, adb_port) = adb::adb_path(&app_handle)?;
                let adb_bin = adb_bin.to_string_lossy().to_string();
                let args = vec![
                    "-P".to_string(),
                    adb_port.to_string(),
                    "-s".to_string(),
                    serial,
                    "shell".to_string(),
                ];
                Box::new(local::LocalSession::spawn(id.clone(), rows, cols, app_handle, Some(adb_bin), None, args, CAP_FILE_CAST)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_local_config() {
        let cfg: ConnectionConfig =
            serde_json::from_str(r#"{"type":"local","shell":null,"working_dir":"/home/u"}"#).unwrap();
        match cfg {
            ConnectionConfig::Local { shell, working_dir } => {
                assert!(shell.is_none());
                assert_eq!(working_dir.as_deref(), Some("/home/u"));
            }
            _ => panic!("expected local config"),
        }
    }

    #[test]
    fn deserializes_wsl_config() {
        let cfg: ConnectionConfig =
            serde_json::from_str(r#"{"type":"wsl","distro":"Ubuntu","working_dir":null}"#).unwrap();
        match cfg {
            ConnectionConfig::Wsl { distro, working_dir } => {
                assert_eq!(distro.as_deref(), Some("Ubuntu"));
                assert!(working_dir.is_none());
            }
            _ => panic!("expected wsl config"),
        }
    }

    #[test]
    fn deserializes_ssh_config() {
        let cfg: ConnectionConfig = serde_json::from_str(
            r#"{"type":"ssh","host":"h","port":2222,"username":"u","password":"p",
                "private_key_path":null,"proxy_id":"p1","agent_forwarding":true,"x11_forwarding":false}"#,
        )
        .unwrap();
        match cfg {
            ConnectionConfig::Ssh {
                host,
                port,
                username,
                password,
                private_key_path,
                proxy_id,
                agent_forwarding,
                x11_forwarding,
            } => {
                assert_eq!(host, "h");
                assert_eq!(port, 2222);
                assert_eq!(username, "u");
                assert_eq!(password, "p");
                assert!(private_key_path.is_none());
                assert_eq!(proxy_id.as_deref(), Some("p1"));
                assert!(agent_forwarding);
                assert!(!x11_forwarding);
            }
            _ => panic!("expected ssh config"),
        }
    }

    #[test]
    fn deserializes_telnet_serial_adb_config() {
        let telnet: ConnectionConfig =
            serde_json::from_str(r#"{"type":"telnet","host":"h","port":23}"#).unwrap();
        assert!(matches!(telnet, ConnectionConfig::Telnet { host, port } if host == "h" && port == 23));

        let serial: ConnectionConfig = serde_json::from_str(
            r#"{"type":"serial","port_name":"COM3","baud_rate":115200,"data_bits":8,
                "stop_bits":1,"parity":"None","flow_control":"None"}"#,
        )
        .unwrap();
        assert!(matches!(
            serial,
            ConnectionConfig::Serial { port_name, baud_rate, .. } if port_name == "COM3" && baud_rate == 115200
        ));

        let adb: ConnectionConfig =
            serde_json::from_str(r#"{"type":"adb","serial":"emulator-5554"}"#).unwrap();
        assert!(matches!(adb, ConnectionConfig::Adb { serial } if serial == "emulator-5554"));
    }

    #[test]
    fn capability_as_str_matches_frontend() {
        assert_eq!(Capability::File.as_str(), "file");
        assert_eq!(Capability::Tunnel.as_str(), "tunnel");
        assert_eq!(Capability::Exec.as_str(), "exec");
        assert_eq!(Capability::Zmodem.as_str(), "zmodem");
        assert_eq!(Capability::Monitor.as_str(), "monitor");
    }
}
