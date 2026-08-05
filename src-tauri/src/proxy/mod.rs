use russh::client;
use russh::keys::*;
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream as TokioTcpStream;
use tokio_socks::tcp::socks5::Socks5Stream;

pub trait ProxyStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> ProxyStream for T {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProxyType {
    Http,
    Socks5,
    JumpHost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub id: String,
    pub name: String,
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
}

pub struct ProxyManager {
    pub proxies: Mutex<Vec<ProxyConfig>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self { proxies: Mutex::new(Vec::new()) }
    }

    pub fn list(&self) -> Vec<ProxyConfig> {
        self.proxies.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn get(&self, id: &str) -> Option<ProxyConfig> {
        self.proxies.lock().ok()?.iter().find(|p| p.id == id).cloned()
    }

    pub fn save(&self, config: ProxyConfig) {
        if let Ok(mut proxies) = self.proxies.lock() {
            if let Some(existing) = proxies.iter_mut().find(|p| p.id == config.id) {
                *existing = config;
            } else {
                proxies.push(config);
            }
        }
    }

    pub fn delete(&self, id: &str) {
        if let Ok(mut proxies) = self.proxies.lock() {
            proxies.retain(|p| p.id != id);
        }
    }
}



struct JumpHostHandler;
impl client::Handler for JumpHostHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn connect_async(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<    Box<dyn ProxyStream>, String> {
    match proxy.proxy_type {
        ProxyType::Http => {
            http_connect_async(&proxy.host, proxy.port, target_host, target_port).await
        }
        ProxyType::Socks5 => {
            socks5_connect_async(&proxy.host, proxy.port, target_host, target_port).await
        }
        ProxyType::JumpHost => {
            connect_via_jump_host_async(proxy, target_host, target_port).await
        }
    }
}

async fn http_connect_async(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<    Box<dyn ProxyStream>, String> {
    let addr = crate::netaddr::sock_addr(proxy_host, proxy_port);
    let mut stream = TokioTcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Proxy TCP connect: {}", e))?;

    let target = crate::netaddr::sock_addr(target_host, target_port);
    let connect_req = format!(
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
        target, target
    );
    stream
        .write_all(connect_req.as_bytes())
        .await
        .map_err(|e| format!("Proxy CONNECT write: {}", e))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Proxy CONNECT read: {}", e))?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if !response.contains("200 Connection established") {
        return Err(format!(
            "Proxy CONNECT failed: {}",
            response.lines().next().unwrap_or("?")
        ));
    }

    Ok(Box::new(stream))
}

async fn socks5_connect_async(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<    Box<dyn ProxyStream>, String> {
    let addr = crate::netaddr::sock_addr(proxy_host, proxy_port);
    let stream = TokioTcpStream::connect(&addr)
        .await
        .map_err(|e| format!("SOCKS5 TCP connect: {}", e))?;

    let socks_stream = Socks5Stream::connect_with_socket(stream, (target_host, target_port))
        .await
        .map_err(|e| format!("SOCKS5 connect: {}", e))?;

    Ok(Box::new(socks_stream))
}

async fn connect_via_jump_host_async(
    jump: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<    Box<dyn ProxyStream>, String> {
    let addr = crate::netaddr::sock_addr(&jump.host, jump.port);
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, &addr, JumpHostHandler)
        .await
        .map_err(|e| format!("Jump host connect: {}", e))?;

    let jump_user = jump.username.as_deref().unwrap_or("root");
    if let Some(ref key_path) = jump.private_key_path {
        let key = load_secret_key(key_path, None)
            .map_err(|e| format!("Jump host key load: {}", e))?;
        let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), None);
        let auth = handle
            .authenticate_publickey(jump_user, key_with_alg)
            .await
            .map_err(|e| format!("Jump host key auth: {}", e))?;
        if !auth.success() {
            return Err("Jump host key auth rejected".to_string());
        }
    } else {
        let auth = handle
            .authenticate_password(jump_user, jump.password.as_deref().unwrap_or(""))
            .await
            .map_err(|e| format!("Jump host password auth: {}", e))?;
        if !auth.success() {
            return Err("Jump host password auth rejected".to_string());
        }
    }

    let mut channel = handle
        .channel_open_direct_tcpip(target_host, target_port as u32, "", 0)
        .await
        .map_err(|e| format!("Direct TCP/IP channel: {}", e))?;

    let (user_end, mut relay_end) = tokio::io::duplex(65536);

    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            tokio::select! {
                result = relay_end.read(&mut buf) => {
                    match result {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if channel.data(&buf[..n]).await.is_err() { break; }
                        }
                    }
                }
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            if relay_end.write_all(&data).await.is_err() { break; }
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                        _ => {}
                    }
                }
            }
        }
    });

    Ok(Box::new(user_end))
}


