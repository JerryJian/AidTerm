use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use russh::client;
use russh::keys::*;
use russh::ChannelMsg;
use tokio::runtime::Runtime;

pub struct SftpManager {
    pub connections: Mutex<HashMap<String, SftpConnection>>,
}

impl SftpManager {
    pub fn new() -> Self {
        Self { connections: Mutex::new(HashMap::new()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub permissions: String,
}

pub(crate) type Resp<T> = Sender<Result<T, String>>;

pub(crate) enum SftpCmd {
    ListDir { path: String, resp: Resp<Vec<FileEntry>> },
    Download { remote: String, local: String, resp: Resp<()> },
    Upload { local: String, remote: String, resp: Resp<()> },
    Remove { path: String, resp: Resp<()> },
    Rename { old: String, new: String, resp: Resp<()> },
    Mkdir { path: String, resp: Resp<()> },
    ReadFile { remote: String, resp: Resp<String> },
    WriteFile { remote: String, content: String, resp: Resp<()> },
}

struct SftpHandler;

impl client::Handler for SftpHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct SftpConnection {
    cmd_tx: Sender<SftpCmd>,
    kill_tx: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl SftpConnection {
    pub fn connect(
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
    ) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (kill_tx, kill_rx) = mpsc::channel();
        let addr = format!("{}:{}", host, port);

        let handle = std::thread::spawn(move || {
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("SFTP runtime error: {}", e);
                    return;
                }
            };
            if let Err(e) = rt.block_on(Self::run_async(&addr, &username, &password, private_key_path, cmd_rx, kill_rx)) {
                eprintln!("SFTP session error: {}", e);
            }
        });

        Ok(Self { cmd_tx, kill_tx: Some(kill_tx), handle: Some(handle) })
    }

    async fn authenticate(
        handle: &mut client::Handle<SftpHandler>,
        username: &str,
        password: &str,
        private_key_path: Option<&str>,
    ) -> Result<(), String> {
        if let Some(key_path) = private_key_path {
            let key = load_secret_key(key_path, None)
                .map_err(|e| format!("Failed to load private key: {}", e))?;
            let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            let auth = handle.authenticate_publickey(username, key_with_alg).await
                .map_err(|e| format!("Public key auth failed: {}", e))?;
            if !auth.success() {
                return Err("Public key authentication rejected".to_string());
            }
        } else {
            let auth = handle.authenticate_password(username, password).await
                .map_err(|e| format!("Password auth failed: {}", e))?;
            if !auth.success() {
                return Err("Password authentication rejected".to_string());
            }
        }
        Ok(())
    }

    async fn exec_cmd(
        handle: &mut client::Handle<SftpHandler>,
        cmd: &str,
    ) -> Result<Vec<u8>, String> {
        let mut channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel.exec(true, cmd).await
            .map_err(|e| format!("Exec failed: {}", e))?;

        let mut output = Vec::new();
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => output.extend_from_slice(&data),
                Some(ChannelMsg::ExtendedData { ext: 1, data }) => output.extend_from_slice(&data),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        let _ = channel.close().await;
        Ok(output)
    }

    async fn write_file_remote(
        handle: &mut client::Handle<SftpHandler>,
        remote: &str,
        content: &[u8],
    ) -> Result<(), String> {
        let mut channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel.exec(true, format!("cat > {}", shell_escape(remote))).await
            .map_err(|e| format!("Exec failed: {}", e))?;
        let _ = channel.data(content).await;
        let _ = channel.eof().await;

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        let _ = channel.close().await;
        Ok(())
    }

    async fn run_async(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        cmd_rx: Receiver<SftpCmd>,
        kill_rx: Receiver<()>,
    ) -> Result<(), String> {
        let config = Arc::new(client::Config::default());
        let mut handle = client::connect(config, addr, SftpHandler).await
            .map_err(|e| format!("SFTP SSH connect failed: {}", e))?;

        Self::authenticate(&mut handle, username, password, private_key_path.as_deref()).await?;

        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            match cmd_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(cmd) => match cmd {
                    SftpCmd::ListDir { path, resp } => {
                        let result = Self::exec_cmd(&mut handle, &format!("ls -la {}", shell_escape(&path))).await
                            .and_then(|out| String::from_utf8(out).map_err(|e| format!("UTF-8: {}", e)))
                            .and_then(|out| Self::parse_ls_output(&out, &path));
                        let _ = resp.send(result);
                    }
                    SftpCmd::Download { remote, local, resp } => {
                        let result = Self::exec_cmd(&mut handle, &format!("cat {}", shell_escape(&remote))).await
                            .and_then(|content| {
                                std::fs::write(&local, &content).map_err(|e| format!("Local write: {}", e))
                            });
                        let _ = resp.send(result);
                    }
                    SftpCmd::Upload { local, remote, resp } => {
                        let content = std::fs::read(&local).map_err(|e| format!("Local read: {}", e));
                        let result = match content {
                            Ok(c) => Self::write_file_remote(&mut handle, &remote, &c).await,
                            Err(e) => Err(e),
                        };
                        let _ = resp.send(result);
                    }
                    SftpCmd::Remove { path, resp } => {
                        let result = Self::exec_cmd(&mut handle, &format!("rm -rf {}", shell_escape(&path))).await
                            .map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::Rename { old, new, resp } => {
                        let result = Self::exec_cmd(
                            &mut handle,
                            &format!("mv {} {}", shell_escape(&old), shell_escape(&new)),
                        ).await.map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::Mkdir { path, resp } => {
                        let result = Self::exec_cmd(&mut handle, &format!("mkdir -p {}", shell_escape(&path))).await
                            .map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::ReadFile { remote, resp } => {
                        let result = Self::exec_cmd(&mut handle, &format!("cat {}", shell_escape(&remote))).await
                            .and_then(|out| String::from_utf8(out).map_err(|e| format!("UTF-8: {}", e)));
                        let _ = resp.send(result);
                    }
                    SftpCmd::WriteFile { remote, content, resp } => {
                        let result = Self::write_file_remote(&mut handle, &remote, content.as_bytes()).await;
                        let _ = resp.send(result);
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        tokio::spawn(handle);
        Ok(())
    }

    fn parse_ls_output(output: &str, _base_path: &str) -> Result<Vec<FileEntry>, String> {
        let mut entries = Vec::new();
        for line in output.lines() {
            if line.starts_with("total ") || line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }
            let perms = parts[0];
            let is_dir = perms.starts_with('d');
            let size: u64 = parts[4].parse().unwrap_or(0);
            let modified = format!("{} {} {}", parts[5], parts[6], parts[7]);
            let name = parts[8..].join(" ");
            if name == "." || name == ".." {
                continue;
            }
            entries.push(FileEntry {
                name,
                is_dir,
                size,
                modified,
                permissions: perms.to_string(),
            });
        }
        Ok(entries)
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::ListDir { path: path.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn download(&self, remote: &str, local: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Download { remote: remote.to_string(), local: local.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn upload(&self, local: &str, remote: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Upload { local: local.to_string(), remote: remote.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn remove(&self, path: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Remove { path: path.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn rename(&self, old: &str, new: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Rename { old: old.to_string(), new: new.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Mkdir { path: path.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn read_file(&self, remote: &str) -> Result<String, String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::ReadFile { remote: remote.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn write_file(&self, remote: &str, content: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::WriteFile { remote: remote.to_string(), content: content.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn cmd_tx(&self) -> Sender<SftpCmd> {
        self.cmd_tx.clone()
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

fn shell_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
