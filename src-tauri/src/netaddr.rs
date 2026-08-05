pub fn sock_addr(host: &str, port: u16) -> String {
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if host.contains(':') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    }
}

pub fn split_host_port(addr: &str) -> (String, u16) {
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((host, port_str)) = rest.split_once("]:") {
            if let Ok(port) = port_str.parse::<u16>() {
                return (host.to_string(), port);
            }
        }
    }
    match addr.rsplit_once(':') {
        Some((host, port_str)) => match port_str.parse::<u16>() {
            Ok(port) => (host.to_string(), port),
            Err(_) => (addr.to_string(), 22),
        },
        None => (addr.to_string(), 22),
    }
}
