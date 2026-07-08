use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, UNIX_EPOCH};
use russh::client;
use russh::keys::*;
use russh_sftp::client::SftpSession;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::runtime::Runtime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct SftpManager {
    pub connections: Mutex<HashMap<String, SftpConnection>>,
}

impl SftpManager {
    pub fn new() -> Self {
        Self { connections: Mutex::new(HashMap::new()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpProgress {
    pub remote: String,
    pub local: String,
    pub r#type: String,
    pub bytes_transferred: u64,
    pub total_size: u64,
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
    Download { id: String, remote: String, local: String, resp: Resp<()> },
    Upload { id: String, local: String, remote: String, resp: Resp<()> },
    CancelTransfer { id: String, resp: Resp<()> },
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

fn format_mode(attrs: &russh_sftp::protocol::FileAttributes) -> String {
    let ft = attrs.file_type();
    let ty = match ft {
        russh_sftp::protocol::FileType::Dir => 'd',
        russh_sftp::protocol::FileType::Symlink => 'l',
        russh_sftp::protocol::FileType::File => '-',
        _ => '-',
    };
    let perm = attrs.permissions();
    let rwx = |r: bool, w: bool, x: bool| -> String {
        let rch = if r { 'r' } else { '-' };
        let wch = if w { 'w' } else { '-' };
        let xch = if x { 'x' } else { '-' };
        format!("{rch}{wch}{xch}")
    };
    format!("{}{}{}{}", ty, rwx(perm.owner_read, perm.owner_write, perm.owner_exec), rwx(perm.group_read, perm.group_write, perm.group_exec), rwx(perm.other_read, perm.other_write, perm.other_exec))
}

fn format_mtime(attrs: &russh_sftp::protocol::FileAttributes) -> String {
    if let Ok(t) = attrs.modified() {
        let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        // Format like "Jan 15 10:30" or "Jan 15  2024" depending on age
        let months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
        let now_days = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0);
        let tm = secs % 86400;
        let h = tm / 3600;
        let m = (tm % 3600) / 60;

        // Better: use a simple approach
        let total_days = secs / 86400;
        let month_days = [31,28,31,30,31,30,31,31,30,31,30,31];
        let mut remaining = total_days;
        let mut year = 1970i64;
        loop {
            let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 366 } else { 365 };
            if remaining < days_in_year { break; }
            remaining -= days_in_year;
            year += 1;
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let mut month = 0usize;
        let mut day_of_month = remaining;
        for (i, &md) in month_days.iter().enumerate() {
            let dim = if i == 1 && leap { 29 } else { md };
            if day_of_month < dim { month = i; break; }
            day_of_month -= dim;
        }
        day_of_month += 1;
        let month_name = months[month];
        let current_year = 1970 + (now_days / 365) as u64;
        if year == current_year as i64 {
            format!("{} {:2} {:02}:{:02}", month_name, day_of_month, h, m)
        } else {
            format!("{} {:2}  {}", month_name, day_of_month, year)
        }
    } else {
        String::new()
    }
}

#[allow(dead_code)]
impl SftpConnection {
    pub fn connect(
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
        app: AppHandle,
    ) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (kill_tx, kill_rx) = mpsc::channel();
        let addr = format!("{}:{}", host, port);

        let cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::new(Mutex::new(HashMap::new()));
        let cf = cancel_flags.clone();

        let handle = std::thread::spawn(move || {
            let rt = match Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("SFTP runtime error: {}", e);
                    return;
                }
            };
            if let Err(e) = rt.block_on(Self::run_async(&addr, &username, &password, private_key_path, cmd_rx, kill_rx, app, cf)) {
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

    async fn run_async(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        cmd_rx: Receiver<SftpCmd>,
        kill_rx: Receiver<()>,
        app: AppHandle,
        cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    ) -> Result<(), String> {
        let config = Arc::new(client::Config::default());
        let mut handle = client::connect(config, addr, SftpHandler).await
            .map_err(|e| format!("SFTP SSH connect failed: {}", e))?;

        Self::authenticate(&mut handle, username, password, private_key_path.as_deref()).await?;

        let channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel.request_subsystem(true, "sftp").await
            .map_err(|e| format!("Failed to start SFTP subsystem: {}", e))?;
        let stream = channel.into_stream();
        let session = SftpSession::new(stream).await
            .map_err(|e| format!("Failed to initialize SFTP session: {}", e))?;
        let session = Arc::new(session);

        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            match cmd_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(cmd) => match cmd {
                    SftpCmd::ListDir { path, resp } => {
                        let s = session.clone();
                        let result = Self::do_list_dir(&s, &path).await;
                        let _ = resp.send(result);
                    }
                    SftpCmd::Download { id, remote, local, resp } => {
                        let s = session.clone();
                        let a = app.clone();
                        let cf = cancel_flags.clone();
                        let cancel_flag = Arc::new(AtomicBool::new(false));
                        cf.lock().unwrap().insert(id.clone(), cancel_flag.clone());
                        // Spawn so other commands (listDir, etc.) can run concurrently
                        tokio::spawn(async move {
                            let result = Self::do_download(&s, &remote, &local, &remote, &local, &a, &cancel_flag).await;
                            cf.lock().unwrap().remove(&id);
                            let _ = resp.send(result);
                        });
                    }
                    SftpCmd::Upload { id, local, remote, resp } => {
                        let s = session.clone();
                        let a = app.clone();
                        let cf = cancel_flags.clone();
                        let cancel_flag = Arc::new(AtomicBool::new(false));
                        cf.lock().unwrap().insert(id.clone(), cancel_flag.clone());
                        tokio::spawn(async move {
                            let result = Self::do_upload(&s, &local, &remote, &remote, &local, &a, &cancel_flag).await;
                            cf.lock().unwrap().remove(&id);
                            let _ = resp.send(result);
                        });
                    }
                    SftpCmd::CancelTransfer { id, resp } => {
                        let result = if let Some(flag) = cancel_flags.lock().unwrap().remove(&id) {
                            flag.store(true, Ordering::Relaxed);
                            Ok(())
                        } else {
                            Err("Transfer not found or already completed".to_string())
                        };
                        let _ = resp.send(result);
                    }
                    SftpCmd::Remove { path, resp } => {
                        let s = session.clone();
                        let result = Self::do_remove(&s, &path).await;
                        let _ = resp.send(result);
                    }
                    SftpCmd::Rename { old, new, resp } => {
                        let s = session.clone();
                        let result = Self::do_rename(&s, &old, &new).await;
                        let _ = resp.send(result);
                    }
                    SftpCmd::Mkdir { path, resp } => {
                        let s = session.clone();
                        let result = Self::do_mkdir(&s, &path).await;
                        let _ = resp.send(result);
                    }
                    SftpCmd::ReadFile { remote, resp } => {
                        let s = session.clone();
                        let result = Self::do_read_file(&s, &remote).await;
                        let _ = resp.send(result);
                    }
                    SftpCmd::WriteFile { remote, content, resp } => {
                        let s = session.clone();
                        let result = Self::do_write_file(&s, &remote, &content).await;
                        let _ = resp.send(result);
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let _ = session.close().await;
        Ok(())
    }

    async fn do_list_dir(session: &SftpSession, path: &str) -> Result<Vec<FileEntry>, String> {
        let mut rd = session.read_dir(path).await
            .map_err(|e| format!("read_dir failed: {}", e))?;
        let mut entries = Vec::new();
        while let Some(entry) = rd.next() {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let attrs = entry.metadata();
            let is_dir = attrs.is_dir();
            let size = attrs.len();
            let permissions = format_mode(&attrs);
            let modified = format_mtime(&attrs);
            entries.push(FileEntry { name, is_dir, size, modified, permissions });
        }
        Ok(entries)
    }

    async fn do_download(session: &SftpSession, remote: &str, local: &str, remote_ident: &str, local_ident: &str, app: &AppHandle, cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        let attrs = session.metadata(remote).await
            .map_err(|e| format!("stat failed: {}", e))?;
        let total = attrs.len();

        let mut remote_file = session.open(remote).await
            .map_err(|e| format!("open remote failed: {}", e))?;
        let mut local_file = tokio::fs::File::create(local).await
            .map_err(|e| format!("local create: {}", e))?;

        let mut buf = vec![0u8; 65536];
        let mut transferred = 0u64;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let n = remote_file.read(&mut buf).await
                .map_err(|e| format!("read remote: {}", e))?;
            if n == 0 { break; }
            local_file.write_all(&buf[..n]).await
                .map_err(|e| format!("local write: {}", e))?;
            transferred += n as u64;
            if total > 0 {
                let _ = app.emit("sftp-progress", SftpProgress {
                    remote: remote_ident.to_string(),
                    local: local_ident.to_string(),
                    r#type: "download".to_string(),
                    bytes_transferred: transferred,
                    total_size: total,
                });
            }
        }

        local_file.flush().await.map_err(|e| format!("local flush: {}", e))?;
        Ok(())
    }

    async fn do_upload(session: &SftpSession, local: &str, remote: &str, remote_ident: &str, local_ident: &str, app: &AppHandle, cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        let local_metadata = std::fs::metadata(local)
            .map_err(|e| format!("local stat: {}", e))?;
        let total = local_metadata.len();

        let mut local_file = tokio::fs::File::open(local).await
            .map_err(|e| format!("local open: {}", e))?;
        let mut remote_file = session.create(remote).await
            .map_err(|e| format!("create remote: {}", e))?;

        let mut buf = vec![0u8; 65536];
        let mut transferred = 0u64;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let n = local_file.read(&mut buf).await
                .map_err(|e| format!("local read: {}", e))?;
            if n == 0 { break; }
            remote_file.write_all(&buf[..n]).await
                .map_err(|e| format!("write remote: {}", e))?;
            transferred += n as u64;
            if total > 0 {
                let _ = app.emit("sftp-progress", SftpProgress {
                    remote: remote_ident.to_string(),
                    local: local_ident.to_string(),
                    r#type: "upload".to_string(),
                    bytes_transferred: transferred,
                    total_size: total,
                });
            }
        }

        remote_file.flush().await.map_err(|e| format!("remote flush: {}", e))?;
        Ok(())
    }

    async fn do_remove(session: &SftpSession, path: &str) -> Result<(), String> {
        // Try as file first, then as directory
        let attrs = session.metadata(path).await
            .map_err(|e| format!("stat failed: {}", e))?;
        if attrs.is_dir() {
            session.remove_dir(path).await
                .map_err(|e| format!("remove_dir failed: {}", e))
        } else {
            session.remove_file(path).await
                .map_err(|e| format!("remove_file failed: {}", e))
        }
    }

    async fn do_rename(session: &SftpSession, old: &str, new: &str) -> Result<(), String> {
        session.rename(old, new).await
            .map_err(|e| format!("rename failed: {}", e))
    }

    async fn do_mkdir(session: &SftpSession, path: &str) -> Result<(), String> {
        session.create_dir(path).await
            .map_err(|e| format!("mkdir failed: {}", e))
    }

    async fn do_read_file(session: &SftpSession, remote: &str) -> Result<String, String> {
        let data = session.read(remote).await
            .map_err(|e| format!("read failed: {}", e))?;
        String::from_utf8(data).map_err(|e| format!("UTF-8: {}", e))
    }

    async fn do_write_file(session: &SftpSession, remote: &str, content: &str) -> Result<(), String> {
        session.write(remote, content.as_bytes()).await
            .map_err(|e| format!("write failed: {}", e))
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::ListDir { path: path.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn download(&self, id: &str, remote: &str, local: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Download { id: id.to_string(), remote: remote.to_string(), local: local.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn upload(&self, id: &str, local: &str, remote: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::Upload { id: id.to_string(), local: local.to_string(), remote: remote.to_string(), resp: tx })
            .map_err(|e| format!("Send error: {}", e))?;
        rx.recv().map_err(|e| format!("Receive error: {}", e))?
    }

    pub fn cancel_transfer(&self, id: &str) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(SftpCmd::CancelTransfer { id: id.to_string(), resp: tx })
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
