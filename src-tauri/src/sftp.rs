use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;
use ssh_rs::ssh;

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

type Resp<T> = Sender<Result<T, String>>;

enum SftpCmd {
    ListDir { path: String, resp: Resp<Vec<FileEntry>> },
    Download { remote: String, local: String, resp: Resp<()> },
    Upload { local: String, remote: String, resp: Resp<()> },
    Remove { path: String, resp: Resp<()> },
    Rename { old: String, new: String, resp: Resp<()> },
    Mkdir { path: String, resp: Resp<()> },
    ReadFile { remote: String, resp: Resp<String> },
    WriteFile { remote: String, content: String, resp: Resp<()> },
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
            if let Err(e) = Self::run(&addr, &username, &password, private_key_path, cmd_rx, kill_rx) {
                eprintln!("SFTP session error: {}", e);
            }
        });

        Ok(Self { cmd_tx, kill_tx: Some(kill_tx), handle: Some(handle) })
    }

    fn run(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        cmd_rx: Receiver<SftpCmd>,
        kill_rx: Receiver<()>,
    ) -> Result<(), String> {
        let mut builder = ssh::create_session()
            .username(username)
            .timeout(Some(Duration::from_secs(10)));

        if let Some(ref key_path) = private_key_path {
            builder = builder.private_key_path(key_path);
        }

        builder = builder.password(password);

        let mut session = builder
            .connect(addr)
            .map_err(|e| format!("SFTP SSH connect failed: {}", e))?
            .run_local();

        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            match cmd_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(cmd) => match cmd {
                    SftpCmd::ListDir { path, resp } => {
                        let result = Self::exec_cmd(&mut session, &format!("ls -la {}", shell_escape(&path)))
                            .and_then(|out| Self::parse_ls_output(&out, &path));
                        let _ = resp.send(result);
                    }
                    SftpCmd::Download { remote, local, resp } => {
                        let result = Self::scp_download(&mut session, &local, &remote);
                        let _ = resp.send(result);
                    }
                    SftpCmd::Upload { local, remote, resp } => {
                        let result = Self::scp_upload(&mut session, &local, &remote);
                        let _ = resp.send(result);
                    }
                    SftpCmd::Remove { path, resp } => {
                        let result =
                            Self::exec_cmd(&mut session, &format!("rm -rf {}", shell_escape(&path)))
                                .map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::Rename { old, new, resp } => {
                        let result = Self::exec_cmd(
                            &mut session,
                            &format!("mv {} {}", shell_escape(&old), shell_escape(&new)),
                        )
                        .map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::Mkdir { path, resp } => {
                        let result =
                            Self::exec_cmd(&mut session, &format!("mkdir -p {}", shell_escape(&path)))
                                .map(|_| ());
                        let _ = resp.send(result);
                    }
                    SftpCmd::ReadFile { remote, resp } => {
                        let result = Self::exec_cmd(&mut session, &format!("cat {}", shell_escape(&remote)));
                        let _ = resp.send(result);
                    }
                    SftpCmd::WriteFile { remote, content, resp } => {
                        let result = (|| {
                            let tmp = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
                            std::fs::write(&tmp, &content).map_err(|e| format!("Temp write: {}", e))?;
                            let r = Self::scp_upload(&mut session, tmp.to_str().unwrap_or("/tmp/aidterm"), &remote);
                            let _ = std::fs::remove_file(&tmp);
                            r
                        })();
                        let _ = resp.send(result);
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        session.close();
        Ok(())
    }

    fn exec_cmd(
        session: &mut ssh_rs::LocalSession<impl std::io::Read + std::io::Write>,
        cmd: &str,
    ) -> Result<String, String> {
        let exec = session
            .open_exec()
            .map_err(|e| format!("Failed to open exec: {}", e))?;
        let result = exec
            .send_command(cmd)
            .map_err(|e| format!("Command failed: {}", e))?;
        String::from_utf8(result).map_err(|e| format!("UTF-8 error: {}", e))
    }

    fn scp_download(
        session: &mut ssh_rs::LocalSession<impl std::io::Read + std::io::Write>,
        local: &str,
        remote: &str,
    ) -> Result<(), String> {
        let scp = session
            .open_scp()
            .map_err(|e| format!("Failed to open SCP: {}", e))?;
        scp.download(local, remote)
            .map_err(|e| format!("SCP download failed: {}", e))
    }

    fn scp_upload(
        session: &mut ssh_rs::LocalSession<impl std::io::Read + std::io::Write>,
        local: &str,
        remote: &str,
    ) -> Result<(), String> {
        let scp = session
            .open_scp()
            .map_err(|e| format!("Failed to open SCP: {}", e))?;
        scp.upload(local, remote)
            .map_err(|e| format!("SCP upload failed: {}", e))
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
