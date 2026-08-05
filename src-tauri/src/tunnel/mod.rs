use russh::client;
use russh::keys::*;
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::mpsc as tokio_mpsc;

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
    kill_tx: tokio_mpsc::UnboundedSender<()>,
    handle: Option<JoinHandle<()>>,
}

impl Tunnel {
    fn new(info: TunnelInfo, kill_tx: tokio_mpsc::UnboundedSender<()>, handle: JoinHandle<()>) -> Self {
        Self { info, kill_tx, handle: Some(handle) }
    }

    fn kill(&mut self) {
        let _ = self.kill_tx.send(());
        self.handle.take();
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

struct TunnelHandler;
impl client::Handler for TunnelHandler {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, _server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

struct RemoteTunnelHandler {
    incoming_tx: tokio_mpsc::UnboundedSender<(russh::Channel<russh::client::Msg>, String, u32, String, u32)>,
}

impl client::Handler for RemoteTunnelHandler {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, _server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _handle: russh::client::ChannelOpenHandle,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let _ = self.incoming_tx.send((
            channel,
            connected_address.to_string(),
            connected_port,
            originator_address.to_string(),
            originator_port,
        ));
        Ok(())
    }
}

async fn authenticate_on<H: client::Handler<Error = anyhow::Error>>(
    handle: &mut client::Handle<H>,
    auth: &SshAuth,
) -> Result<(), String> {
    if let Some(ref key_path) = auth.private_key_path {
        let key = load_secret_key(key_path, None)
            .map_err(|e| format!("Key load: {}", e))?;
        let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), None);
        let result = handle
            .authenticate_publickey(&auth.username, key_with_alg)
            .await
            .map_err(|e| format!("Key auth: {}", e))?;
        if !result.success() {
            return Err("Key auth rejected".to_string());
        }
    } else if let Some(ref pass) = auth.password {
        let result = handle
            .authenticate_password(&auth.username, pass)
            .await
            .map_err(|e| format!("Password auth: {}", e))?;
        if !result.success() {
            return Err("Password auth rejected".to_string());
        }
    } else {
        return Err("No auth method".to_string());
    }
    Ok(())
}

async fn connect_and_auth<H: client::Handler<Error = anyhow::Error> + 'static>(
    auth: &SshAuth,
    handler: H,
) -> Result<client::Handle<H>, String> {
    let addr = crate::netaddr::sock_addr(&auth.host, auth.port);
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, &addr, handler)
        .await
        .map_err(|e| format!("TCP connect: {}", e))?;

    authenticate_on(&mut handle, auth).await?;
    Ok(handle)
}

async fn relay_between(
    tcp: std::net::TcpStream,
    mut channel: russh::Channel<russh::client::Msg>,
) {
    let mut tcp = match TokioTcpStream::from_std(tcp) {
        Ok(t) => t,
        Err(_) => return,
    };

    let mut buf = [0u8; 65536];
    loop {
        tokio::select! {
            n = tcp.read(&mut buf) => {
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if channel.data(&buf[..n]).await.is_err() { break; }
                    }
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if tcp.write_all(&data).await.is_err() { break; }
                        if tcp.flush().await.is_err() { break; }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
        }
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
        let (kill_tx, kill_rx) = tokio_mpsc::unbounded_channel::<()>();
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
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let mut tunnels = report.lock().unwrap();
                    if let Some(t) = tunnels.get_mut(&spawn_id) {
                        t.info.status = TunnelStatus::Error(format!("Runtime: {}", e));
                    }
                    return;
                }
            };

            {
                let mut tunnels = report.lock().unwrap();
                if let Some(t) = tunnels.get_mut(&spawn_id) {
                    t.info.status = TunnelStatus::Running;
                }
            }

            let result = rt.block_on(async {
                let mut kill_rx = kill_rx;
                match req.tunnel_type {
                    TunnelType::Local => run_local_tunnel(
                        &auth, &req.bind_addr, req.bind_port,
                        &req.target_host.unwrap_or_default(),
                        req.target_port.unwrap_or(0), &mut kill_rx,
                    ).await,
                    TunnelType::Remote => run_remote_tunnel(
                        &auth, &req.bind_addr, req.bind_port,
                        &req.target_host.unwrap_or_default(),
                        req.target_port.unwrap_or(0), &mut kill_rx,
                    ).await,
                    TunnelType::Dynamic => run_dynamic_tunnel(
                        &auth, &req.bind_addr, req.bind_port, &mut kill_rx,
                    ).await,
                }
            });

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

async fn run_local_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    target_host: &str,
    target_port: u16,
    kill_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
) -> Result<(), String> {
    let handle = Arc::new(tokio::sync::Mutex::new(
        connect_and_auth(auth, TunnelHandler).await?,
    ));

    let listener = TcpListener::bind(crate::netaddr::sock_addr(bind_addr, bind_port))
        .map_err(|e| format!("Bind {}:{}: {}", bind_addr, bind_port, e))?;
    listener.set_nonblocking(true)
        .map_err(|e| format!("Set nonblocking: {}", e))?;

    let target_host = target_host.to_string();

    loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        match listener.accept() {
            Ok((incoming, _)) => {
                let th = target_host.clone();
                let h = handle.lock().await;
                match h.channel_open_direct_tcpip(&th, target_port as u32, "", 0).await {
                    Ok(ch) => {
                        tokio::spawn(async move {
                            relay_between(incoming, ch).await;
                        });
                    }
                    Err(_) => continue,
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(e) => return Err(format!("Accept: {}", e)),
        }
    }
}

async fn run_remote_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    target_host: &str,
    target_port: u16,
    kill_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
) -> Result<(), String> {
    let (incoming_tx, mut incoming_rx) = tokio_mpsc::unbounded_channel();

    let handle = connect_and_auth(auth, RemoteTunnelHandler { incoming_tx }).await?;

    handle.tcpip_forward(bind_addr, bind_port as u32).await
        .map_err(|e| format!("Remote listen on {}:{}: {}", bind_addr, bind_port, e))?;

    loop {
        tokio::select! {
            _ = kill_rx.recv() => {
                return Ok(());
            }
            Some((channel, _connected_addr, _connected_port, _originator_addr, _originator_port)) = incoming_rx.recv() => {
                let remote = match TcpStream::connect(crate::netaddr::sock_addr(target_host, target_port)) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Connect to target {}:{}: {}", target_host, target_port, e);
                        continue;
                    }
                };
                tokio::spawn(async move {
                    relay_between(remote, channel).await;
                });
            }
        }
    }
}

async fn run_dynamic_tunnel(
    auth: &SshAuth,
    bind_addr: &str,
    bind_port: u16,
    kill_rx: &mut tokio_mpsc::UnboundedReceiver<()>,
) -> Result<(), String> {
    let handle = Arc::new(tokio::sync::Mutex::new(
        connect_and_auth(auth, TunnelHandler).await?,
    ));

    let listener = TcpListener::bind(crate::netaddr::sock_addr(bind_addr, bind_port))
        .map_err(|e| format!("Bind {}:{}: {}", bind_addr, bind_port, e))?;
    listener.set_nonblocking(true)
        .map_err(|e| format!("Set nonblocking: {}", e))?;

    loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        match listener.accept() {
            Ok((mut incoming, _)) => {
                let (target_host, target_port) = match socks5_handshake(&mut incoming) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let h = handle.lock().await;
                match h.channel_open_direct_tcpip(&target_host, target_port as u32, "", 0).await {
                    Ok(ch) => {
                        tokio::spawn(async move {
                            relay_between(incoming, ch).await;
                        });
                    }
                    Err(_) => continue,
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(e) => return Err(format!("Accept: {}", e)),
        }
    }
}

fn socks5_handshake(stream: &mut TcpStream) -> Result<(String, u16), String> {
    let mut buf = [0u8; 512];

    let n = std::io::Read::read(stream, &mut buf)
        .map_err(|e| format!("Read greeting: {}", e))?;
    if n < 3 || buf[0] != 0x05 {
        return Err("Not SOCKS5".to_string());
    }
    std::io::Write::write_all(stream, &[0x05, 0x00])
        .map_err(|e| format!("Write greeting: {}", e))?;

    let n = std::io::Read::read(stream, &mut buf)
        .map_err(|e| format!("Read request: {}", e))?;
    if n < 7 || buf[0] != 0x05 {
        return Err("Invalid request".to_string());
    }
    if buf[1] != 0x01 {
        let _ = std::io::Write::write_all(stream, &[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
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
            let _ = std::io::Write::write_all(stream, &[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            return Err(format!("Unsupported addr type {}", at));
        }
    };

    std::io::Write::write_all(stream, &[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .map_err(|e| format!("Write response: {}", e))?;

    Ok((host, port))
}
