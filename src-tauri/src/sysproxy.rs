//! Build reqwest clients that fall back to the OS-level proxy when no proxy
//! environment variables are set.
//!
//! reqwest only reads `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` (and the
//! lowercase variants) by default. Apps launched by double-click do not
//! inherit those from a shell, so the system proxy configured in Windows
//! "Internet Settings" (the registry values Clash / v2rayN / etc. write when
//! "System Proxy" is toggled on) is otherwise never used — requests to
//! external services then fail with "error sending request for url (…)",
//! while LAN services (Ollama etc.) keep working. This module plugs that gap.

use std::time::Duration;

/// True when the process environment carries an explicit proxy selection.
fn env_proxy_present() -> bool {
    ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

/// Windows system proxy from `HKCU\...\Internet Settings`. Returns a proxy
/// URL like `http://127.0.0.1:7890` plus the `ProxyOverride` bypass list.
#[cfg(target_os = "windows")]
fn system_proxy() -> Option<(String, String)> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(SETTINGS).ok()?;
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return None;
    }
    let server: String = key.get_value("ProxyServer").ok()?;
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    // `ProxyServer` is either a bare `host:port` or a per-protocol list like
    // `http=127.0.0.1:7890;https=127.0.0.1:7890;socks=127.0.0.1:7891`.
    // reqwest only needs the http entry: HTTPS is tunneled through it with
    // CONNECT, and SOCKS entries are deliberately ignored here.
    let addr = if server.contains('=') {
        server
            .split(';')
            .map(|pair| pair.split_once('='))
            .find(|kv| kv.is_some_and(|(proto, _)| proto.eq_ignore_ascii_case("http")))
            .and_then(|kv| kv)
            .map(|(_, addr)| addr.trim())
            .unwrap_or_else(|| {
                server
                    .split(';')
                    .next()
                    .and_then(|s| s.split_once('='))
                    .map(|(_, a)| a.trim())
                    .unwrap_or(server)
            })
    } else {
        server
    };
    if addr.is_empty() {
        return None;
    }
    let url = if addr.contains("://") { addr.to_string() } else { format!("http://{addr}") };

    // Bypass list like `localhost;127.*;*.local;<local>`. `<local>` is
    // registry-speak for "no proxy for LAN" and has no reqwest equivalent,
    // so it is dropped; the rest is translated to the comma-separated
    // format reqwest's NoProxy expects (wildcards `*` are supported).
    let override_list: String = key.get_value("ProxyOverride").unwrap_or_default();
    let bypass: String = override_list
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty() && *e != "<local>" && *e != "*.local")
        .collect::<Vec<_>>()
        .join(",");

    Some((url, bypass))
}

#[cfg(not(target_os = "windows"))]
fn system_proxy() -> Option<(String, String)> {
    None
}

/// Build an HTTP client that uses the proxy environment variables when set,
/// falling back to the Windows system proxy otherwise. `timeout` of `None`
/// keeps reqwest's default (no request timeout).
pub fn build_proxied_client(timeout: Option<Duration>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder();
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }

    if !env_proxy_present() {
        if let Some((url, bypass)) = system_proxy() {
            let mut no_proxy = String::from("localhost,127.0.0.1,::1");
            if !bypass.is_empty() {
                no_proxy.push(',');
                no_proxy.push_str(&bypass);
            }
            let proxy = reqwest::Proxy::all(&url)
                .map_err(|e| format!("Invalid system proxy {url}: {e}"))?
                .no_proxy(reqwest::NoProxy::from_string(&no_proxy));
            builder = builder.proxy(proxy);
            log::info!("[sysproxy] using system proxy {url}, bypass: {no_proxy}");
        }
    }

    builder.build().map_err(|e| format!("Failed to build HTTP client: {e}"))
}