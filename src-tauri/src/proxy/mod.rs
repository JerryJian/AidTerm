use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

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

fn http_connect(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    let addr = format!("{}:{}", proxy_host, proxy_port);
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| format!("Invalid proxy addr: {}", addr))?,
        Duration::from_secs(10),
    )
    .map_err(|e| format!("Proxy TCP connect: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let connect_req = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n\r\n",
        target_host, target_port, target_host, target_port
    );
    stream
        .write_all(connect_req.as_bytes())
        .map_err(|e| format!("Proxy CONNECT write: {}", e))?;

    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("Proxy CONNECT read: {}", e))?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if !response.contains("200 Connection established") {
        return Err(format!("Proxy CONNECT failed: {}", response.lines().next().unwrap_or("?")));
    }

    stream.set_read_timeout(None).ok();
    stream.set_write_timeout(None).ok();
    Ok(stream)
}

fn socks5_connect(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    let addr = format!("{}:{}", proxy_host, proxy_port);
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| format!("Invalid proxy addr: {}", addr))?,
        Duration::from_secs(10),
    )
    .map_err(|e| format!("SOCKS5 TCP connect: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    // No auth
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .map_err(|e| format!("SOCKS5 handshake write: {}", e))?;
    let mut buf = [0u8; 2];
    stream
        .read_exact(&mut buf)
        .map_err(|e| format!("SOCKS5 handshake read: {}", e))?;
    if buf[0] != 0x05 || buf[1] != 0x00 {
        return Err(format!("SOCKS5 handshake failed: {:02x?}", &buf[..]));
    }

    // CONNECT command
    let target_is_ipv4 = target_host
        .chars()
        .all(|c| c == '.' || c.is_ascii_digit());
    let (atyp, addr_bytes): (u8, Vec<u8>) = if target_is_ipv4 {
        let parts: Vec<u8> = target_host
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        if parts.len() != 4 {
            return Err("Invalid IPv4 address".to_string());
        }
        (0x01, parts)
    } else {
        let domain = target_host.as_bytes();
        if domain.len() > 255 {
            return Err("Domain too long".to_string());
        }
        let mut v = vec![domain.len() as u8];
        v.extend_from_slice(domain);
        (0x03, v)
    };

    let mut msg = vec![0x05, 0x01, 0x00, atyp];
    msg.extend_from_slice(&addr_bytes);
    msg.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&msg)
        .map_err(|e| format!("SOCKS5 connect write: {}", e))?;

    // Read response: ver, rep, rsv, atyp, bind.addr, bind.port
    let mut resp_hdr = [0u8; 4];
    stream
        .read_exact(&mut resp_hdr)
        .map_err(|e| format!("SOCKS5 connect read header: {}", e))?;
    if resp_hdr[0] != 0x05 || resp_hdr[1] != 0x00 {
        let rep = resp_hdr[1];
        let err_msg = match rep {
            0x01 => "general SOCKS server failure",
            0x02 => "connection not allowed by ruleset",
            0x03 => "network unreachable",
            0x04 => "host unreachable",
            0x05 => "connection refused by destination",
            0x06 => "TTL expired",
            0x07 => "command not supported / protocol error",
            0x08 => "address type not supported",
            _ => "unknown error",
        };
        return Err(format!("SOCKS5 connect failed: {}", err_msg));
    }

    // Consume remaining bind address
    let atyp = resp_hdr[3];
    let addr_len: usize = match atyp {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).ok();
            len[0] as usize
        }
        _ => return Err("Unknown SOCKS5 address type".to_string()),
    };
    let mut _bind_addr = vec![0u8; addr_len + 2]; // addr + port
    stream.read_exact(&mut _bind_addr).ok();

    stream.set_read_timeout(None).ok();
    stream.set_write_timeout(None).ok();
    Ok(stream)
}

fn connect_via_jump_host(
    jump: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    let addr = format!("{}:{}", jump.host, jump.port);
    let tcp = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| format!("Invalid jump host: {}", addr))?,
        Duration::from_secs(10),
    )
    .map_err(|e| format!("Jump host TCP connect: {}", e))?;

    let mut sess = ssh2::Session::new().map_err(|e| format!("SSH session: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| format!("Jump host handshake: {}", e))?;

    let jump_user = jump.username.as_deref().unwrap_or("root");
    if let Some(ref key_path) = jump.private_key_path {
        sess.userauth_pubkey_file(jump_user, None, Path::new(key_path), None)
            .map_err(|e| format!("Jump host key auth: {}", e))?;
    } else {
        sess.userauth_password(jump_user, jump.password.as_deref().unwrap_or(""))
            .map_err(|e| format!("Jump host password auth: {}", e))?;
    }

    if !sess.authenticated() {
        return Err("Jump host auth failed".to_string());
    }

    sess.set_timeout(5000);

    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Local listener: {}", e))?;
    let local_port = listener.local_addr().map_err(|e| e.to_string())?.port();

    let target_host_owned = target_host.to_string();
    let jump_host_addr = addr.clone();
    let jump_username = jump.username.clone();
    let jump_password = jump.password.clone();
    let jump_key_path = jump.private_key_path.clone();

    std::thread::spawn(move || {
        let (mut accepted, _) = match listener.accept() {
            Ok(a) => a,
            Err(_) => return,
        };

        let tcp2 = match TcpStream::connect_timeout(
            &jump_host_addr.parse().unwrap(),
            Duration::from_secs(10),
        ) {
            Ok(t) => t,
            Err(_) => return,
        };
        let mut sess2 = match ssh2::Session::new() {
            Ok(s) => s,
            Err(_) => return,
        };
        sess2.set_tcp_stream(tcp2);
        if sess2.handshake().is_err() {
            return;
        }
        let ju = jump_username.as_deref().unwrap_or("root");
        if let Some(ref kp) = jump_key_path {
            if sess2
                .userauth_pubkey_file(ju, None, Path::new(kp), None)
                .is_err()
            {
                return;
            }
        } else {
            if sess2
                .userauth_password(ju, jump_password.as_deref().unwrap_or(""))
                .is_err()
            {
                return;
            }
        }
        if !sess2.authenticated() {
            return;
        }

        sess2.set_timeout(100);
        let mut channel = match sess2.channel_direct_tcpip(&target_host_owned, target_port, None) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut buf = [0u8; 65536];
        loop {
            match accepted.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if channel.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    if channel.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
            match channel.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if accepted.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    if accepted.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Give relay thread time to start accepting
    std::thread::sleep(Duration::from_millis(50));

    let target = TcpStream::connect(("127.0.0.1", local_port))
        .map_err(|e| format!("Local connect to relay: {}", e))?;

    Ok(target)
}

pub fn connect(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    match proxy.proxy_type {
        ProxyType::Http => {
            http_connect(&proxy.host, proxy.port, target_host, target_port)
        }
        ProxyType::Socks5 => {
            socks5_connect(&proxy.host, proxy.port, target_host, target_port)
        }
        ProxyType::JumpHost => {
            connect_via_jump_host(proxy, target_host, target_port)
        }
    }
}
