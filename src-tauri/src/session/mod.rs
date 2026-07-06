pub(crate) mod local;
pub(crate) mod ssh;
pub(crate) mod telnet;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;
use crate::proxy;

pub(crate) enum Session {
    Local(local::LocalSession),
    Ssh(ssh::SshConnection),
    Telnet(telnet::TelnetConnection),
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
    ) -> Result<(), String> {
        let session = local::LocalSession::spawn(id.clone(), rows, cols, app_handle, shell)?;
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
        }
    }

    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions.get(id).ok_or("Session not found")?;
        match session {
            Session::Local(s) => s.resize(rows, cols),
            Session::Ssh(s) => s.resize(rows, cols),
            Session::Telnet(_) => Ok(()),
        }
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(session) = sessions.remove(id) {
            match session {
                Session::Local(s) => s.kill(),
                Session::Ssh(mut s) => s.kill(),
                Session::Telnet(mut s) => s.kill(),
            }
        }
        Ok(())
    }
}
