pub(crate) mod local;
pub(crate) mod ssh;
pub(crate) mod telnet;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;
use tokio::sync::oneshot;
use crate::serial;
use crate::proxy;

pub(crate) enum Session {
    Local(local::LocalSession),
    Ssh(ssh::SshConnection),
    Telnet(telnet::TelnetConnection),
    Serial(serial::SerialConnection),
}

pub(crate) struct SessionManager {
    sessions: Mutex<HashMap<String, Session>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self { sessions: Mutex::new(HashMap::new()) }
    }

    pub fn spawn_local(
        &self,
        id: String,
        rows: u16,
        cols: u16,
        app_handle: AppHandle,
        shell: Option<String>,
        working_dir: Option<String>,
    ) -> Result<(), String> {
        let session = local::LocalSession::spawn(id.clone(), rows, cols, app_handle, shell, working_dir)?;
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(id, Session::Local(session));
        Ok(())
    }

    pub fn connect_telnet(
        &self,
        id: String,
        host: String,
        port: u16,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        let session = telnet::TelnetConnection::connect(id.clone(), host, port, app_handle)?;
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(id, Session::Telnet(session));
        Ok(())
    }

    pub fn connect_serial(
        &self,
        id: String,
        config: serial::SerialConfig,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        let session = serial::SerialConnection::connect(id.clone(), config, app_handle)?;
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(id, Session::Serial(session));
        Ok(())
    }

    pub fn connect_ssh(
        &self,
        id: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
        proxy: Option<proxy::ProxyConfig>,
        rows: u16,
        cols: u16,
        agent_forwarding: bool,
        x11_forwarding: bool,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        let session = ssh::SshConnection::connect(
            id.clone(), host, port, username, password, private_key_path, proxy, rows, cols,
            agent_forwarding, x11_forwarding, app_handle,
        )?;
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(id, Session::Ssh(session));
        Ok(())
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get_mut(id).ok_or("Session not found")?;
        match session {
            Session::Local(s) => s.write(data),
            Session::Ssh(s) => s.write(data),
            Session::Telnet(s) => s.write(data),
            Session::Serial(s) => s.write(data),
        }
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get(id).ok_or("Session not found")?;
        match session {
            Session::Local(s) => s.resize(rows, cols),
            Session::Ssh(s) => s.resize(rows, cols),
            Session::Telnet(_) => Ok(()),
            Session::Serial(_) => Ok(()),
        }
    }

    pub async fn exec(&self, id: &str, command: &str) -> Result<String, String> {
        let exec_tx = {
            let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
            let session = sessions.get(id).ok_or("Session not found")?;
            match session {
                Session::Ssh(s) => s.exec_tx(),
                _ => return Err("Exec not supported for this session type".to_string()),
            }
        };
        let tx = exec_tx.ok_or("SSH exec unavailable")?;
        let (resp_tx, resp_rx) = oneshot::channel();
        tx.send((command.to_string(), resp_tx)).map_err(|e| format!("Exec send: {}", e))?;
        resp_rx.await.map_err(|e| format!("Exec recv: {}", e))?
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(session) = sessions.remove(id) {
            match session {
                Session::Local(s) => s.kill(),
                Session::Ssh(mut s) => s.kill(),
                Session::Telnet(mut s) => s.kill(),
                Session::Serial(mut s) => s.kill(),
            }
        }
        Ok(())
    }
}
