use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TunnelType {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TunnelStatus {
    Starting,
    Running,
    Stopped,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreateRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub tunnel_type: TunnelType,
    pub bind_addr: String,
    pub bind_port: u16,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub id: String,
    pub tunnel_type: TunnelType,
    pub bind_addr: String,
    pub bind_port: u16,
    pub target_host: Option<String>,
    pub target_port: Option<u16>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub status: TunnelStatus,
}

struct Tunnel {
    info: TunnelInfo,
    kill_tx: Sender<()>,
    handle: Option<JoinHandle<()>>,
}

impl Tunnel {
    fn new(info: TunnelInfo, kill_tx: Sender<()>, handle: JoinHandle<()>) -> Self {
        Self { info, kill_tx, handle: Some(handle) }
    }

    fn kill(&mut self) {
        let _ = self.kill_tx.send(());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[derive(Clone)]
struct SshAuth {
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    private_key_path: Option<String>,
}

impl SshAuth {
    fn connect(&self) -> Result<ssh2::Session, String> {
        let addr = format!("{}:{}", self.host, self.port);
        let tcp = TcpStream::connect_timeout(
            &addr.parse().map_err(|_| format!("Invalid address: {}", addr))?,
            Duration::from_secs(10),
        )
        .map_err(|e| format!("TCP connect: {}", e))?;

        let mut sess = ssh2::Session::new()
            .map_err(|e| format!("Session create: {}", e))?;
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(|e| format!("Handshake: {}", e))?;

        if let Some(ref key_path) = self.private_key_path {
            sess.userauth_pubkey_file(&self.username, None, Path::new(key_path), None)
                .map_err(|e| format!("Key auth: {}", e))?;
        } else if let Some(ref pass) = self.password {
            sess.userauth_password(&self.username, pass)
                .map_err(|e| format!("Password auth: {}", e))?;
        } else {
            return Err("No auth method".to_string());
        }

        if !sess.authenticated() {
            return Err("Auth failed".to_string());
        }

        // 100ms timeout so channel reads don't block forever
        sess.set_timeout(100);
        Ok(sess)
    }
}

pub struct TunnelManager {
    tunnels: Arc<Mutex<HashMap<String, Tunnel>>>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self { tunnels: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn create(&self, req: TunnelCreateRequest) -> Result<TunnelInfo, String> {
        let tunnel_id = uuid::Uuid::new_v4().to_string();
        let (kill_tx, kill_rx) = mpsc::channel::<()>();
        let auth = SshAuth {
            host: req.host.clone(),
            port: req.port,
            username: req.username.clone(),
            password: req.password.clone(),
            private_key_path: req.private_key_path.clone(),
        };

        let info = TunnelInfo {
            id: tunnel_id.clone(),
            tunnel_type: req.tunnel_type.clone(),
            bind_addr: req.bind_addr.clone(),
            bind_port: req.bind_port,
            target_host: req.target_host.clone(),
            target_port: req.target_port,
            host: req.host.clone(),
            port: req.port,
            username: req.username.clone(),
            status: TunnelStatus::Starting,
        };

        let report = self.tunnels.clone();
        let spawn_id = tunnel_id.clone();

        let handle = std::thread::spawn(move || {
            let result = match req.tunnel_type {
                TunnelType::Local => run_local_tunnel(
                    &auth, &req.bind_addr, req.bind_port,
                    &req.target_host.unwrap_or_default(),
                    req.target_port.unwrap_or(0), &kill_rx,
                ),
                TunnelType::Remote => run_remote_tunnel(
                    &auth, &req.bind_addr, req.bind_port,
                    &req.target_host.unwrap_or_default(),
                    req.target_port.unwrap_or(0), &kill_rx,
                ),
                TunnelType::Dynamic => run_dynamic_tunnel(
                    &auth, &req.bind_addr, req.bind_port, &kill_rx,
                ),
            };

            let mut tunnels = report.lock().unwrap();
            if let Some(t) = tunnels.get_mut(&spawn_id) {
                t.info.status = match result {
                    Ok(()) => TunnelStatus::Stopped,
                    Err(e) => TunnelStatus::Error(e),
                };
            }
        });

        {
            let mut tunnels = self.tunnels.lock().unwrap();
            tunnels.insert(tunnel_id.clone(), Tunnel::new(info.clone(), kill_tx, handle));
        }

        Ok(info)
    }

    pub fn list(&self) -> Vec<TunnelInfo> {
        let tunnels = self.tunnels.lock().unwrap();
        tunnels.values().map(|t| t.info.clone()).collect()
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut tunnels = self.tunnels.lock().unwrap();
        if let Some(mut t) = tunnels.remove(id) {
            t.kill();
            Ok(())
        } else {
            Err(format!("Tunnel {} not found", id))
        }
    }
}

fn forward_one_connection(
    mut tcp: TcpStream,
    auth: &SshAuth,
    target_host: &str,
    target_port: u16,
) {
    let sess = match auth.connect() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut channel = match sess.channel_direct_tcpip(target_host, target_port, None) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut buf = [0u8; 65536];
    loop {
        match tcp.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if channel.write_all(&buf[..n]).is_err() { break; }
                if channel.flush().is_err() { break; }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
        match channel.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tcp.write_all(&buf[..n]).is_err() { break; }
                if tcp.flush().is_err() { break; }
            }
            Err(_) => break,
        }
    }
}

fn run_local_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    target_host: &str,
    target_port: u16,
    kill_rx: &Receiver<()>,
) -> Result<(), String> {
    let listener = TcpListener::bind(format!("{}:{}", bind_addr, bind_port))
        .map_err(|e| format!("Bind {}:{}: {}", bind_addr, bind_port, e))?;
    listener.set_nonblocking(true)
        .map_err(|e| format!("Set nonblocking: {}", e))?;

    let auth = auth.clone();
    let target_host = target_host.to_string();

    loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        match listener.accept() {
            Ok((incoming, _)) => {
                let auth = auth.clone();
                let th = target_host.clone();
                std::thread::spawn(move || {
                    forward_one_connection(incoming, &auth, &th, target_port);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Accept: {}", e)),
        }
    }
}

fn run_remote_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    target_host: &str,
    target_port: u16,
    kill_rx: &Receiver<()>,
) -> Result<(), String> {
    let sess = auth.connect()?;
    let (mut listener, _actual_port) = sess.channel_forward_listen(bind_port, Some(bind_addr), None)
        .map_err(|e| format!("Remote listen on {}:{}: {}", bind_addr, bind_port, e))?;

    loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        match listener.accept() {
            Ok(mut channel) => {
                let mut remote = match TcpStream::connect(format!("{}:{}", target_host, target_port)) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Connect to target {}:{}: {}", target_host, target_port, e);
                        continue;
                    }
                };
                let mut buf = [0u8; 65536];
                loop {
                    match channel.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if remote.write_all(&buf[..n]).is_err() { break; }
                            if remote.flush().is_err() { break; }
                        }
                        Err(_) => break,
                    }
                    match remote.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if channel.write_all(&buf[..n]).is_err() { break; }
                            if channel.flush().is_err() { break; }
                        }
                        Err(_) => break,
                    }
                }
            }
            Err(e) => return Err(format!("Accept remote: {}", e)),
        }
    }
}

fn run_dynamic_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    kill_rx: &Receiver<()>,
) -> Result<(), String> {
    let listener = TcpListener::bind(format!("{}:{}", bind_addr, bind_port))
        .map_err(|e| format!("Bind {}:{}: {}", bind_addr, bind_port, e))?;
    listener.set_nonblocking(true)
        .map_err(|e| format!("Set nonblocking: {}", e))?;

    loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        match listener.accept() {
            Ok((mut incoming, _)) => {
                let auth = auth.clone();
                std::thread::spawn(move || {
                    let (target_host, target_port) = match socks5_handshake(&mut incoming) {
                        Ok(r) => r,
                        Err(_) => return,
                    };
                    forward_one_connection(incoming, &auth, &target_host, target_port);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Accept: {}", e)),
        }
    }
}

fn socks5_handshake(stream: &mut TcpStream) -> Result<(String, u16), String> {
    let mut buf = [0u8; 512];

    let n = stream.read(&mut buf)
        .map_err(|e| format!("Read greeting: {}", e))?;
    if n < 3 || buf[0] != 0x05 {
        return Err("Not SOCKS5".to_string());
    }
    stream.write_all(&[0x05, 0x00])
        .map_err(|e| format!("Write greeting: {}", e))?;

    let n = stream.read(&mut buf)
        .map_err(|e| format!("Read request: {}", e))?;
    if n < 7 || buf[0] != 0x05 {
        return Err("Invalid request".to_string());
    }
    if buf[1] != 0x01 {
        let _ = stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
        return Err("Unsupported cmd".to_string());
    }

    let (host, port) = match buf[3] {
        0x01 => {
            if n < 10 { return Err("Truncated IPv4".to_string()); }
            (format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]),
             u16::from_be_bytes([buf[8], buf[9]]))
        }
        0x03 => {
            let len = buf[4] as usize;
            if n < 7 + len { return Err("Truncated domain".to_string()); }
            (String::from_utf8_lossy(&buf[5..5+len]).to_string(),
             u16::from_be_bytes([buf[5+len], buf[6+len]]))
        }
        0x04 => {
            if n < 22 { return Err("Truncated IPv6".to_string()); }
            let h: Vec<String> = buf[4..20].chunks(2)
                .map(|c| format!("{:02x}{:02x}", c[0], c[1])).collect();
            (h.join(":"), u16::from_be_bytes([buf[20], buf[21]]))
        }
        at => {
            let _ = stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            return Err(format!("Unsupported addr type {}", at));
        }
    };

    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .map_err(|e| format!("Write response: {}", e))?;

    Ok((host, port))
}
