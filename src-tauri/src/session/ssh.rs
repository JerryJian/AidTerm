use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;
use ssh_rs::ssh;
use ssh_rs::TerminalSize;
use tauri::{AppHandle, Emitter, Manager};
use crate::proxy;
use crate::zmodem;

pub struct SshConnection {
    pub write_tx: Sender<String>,
    resize_tx: Sender<(u16, u16)>,
    kill_tx: Option<Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

fn zmodem_loop(
    write_rx: &Receiver<String>,
    kill_rx: &Receiver<()>,
    app_handle: &AppHandle,
    session_id: &str,
    buf: &mut Vec<u8>,
    mut write_shell: impl FnMut(&[u8]) -> Result<(), String>,
    mut read_shell: impl FnMut() -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    // Wait for frontend response (poll)
    let action = loop {
        if kill_rx.try_recv().is_ok() {
            return Ok(());
        }
        if let Some(state) = app_handle.try_state::<zmodem::ZmodemState>() {
            if let Ok(responses) = state.responses.lock() {
                if let Some(response) = responses.get(session_id) {
                    if let Some(path) = response {
                        break Some(path.clone());
                    }
                }
            }
        }
        while let Ok(w) = write_rx.try_recv() {
            write_shell(w.as_bytes())?;
        }
        match read_shell() {
            Ok(more) if !more.is_empty() => buf.extend_from_slice(&more),
            Ok(_) => {}
            Err(e) => return Err(e),
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    if let Some(ref path) = action {
        let mut idle = 0u32;
        loop {
            if kill_rx.try_recv().is_ok() {
                return Ok(());
            }
            while let Ok(w) = write_rx.try_recv() {
                let _ = write_shell(w.as_bytes());
            }
            match read_shell() {
                Ok(more) if !more.is_empty() => {
                    buf.extend_from_slice(&more);
                    idle = 0;
                }
                Ok(_) | Err(_) => {
                    idle += 1;
                    if idle > 50 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }

        if let Err(e) = zmodem::save_to_file(path, buf) {
            let _ = app_handle.emit("zmodem-end", serde_json::json!({
                "session_id": session_id, "error": e,
            }));
        } else {
            let _ = app_handle.emit("zmodem-end", serde_json::json!({
                "session_id": session_id,
            }));
        }
    } else {
        let _ = app_handle.emit("zmodem-end", serde_json::json!({
            "session_id": session_id,
        }));
    }

    if let Some(state) = app_handle.try_state::<zmodem::ZmodemState>() {
        let _ = state.responses.lock().map(|mut m| m.remove(session_id));
    }

    Ok(())
}

impl SshConnection {
    pub fn connect(
        id: String,
        host: String,
        port: u16,
        username: String,
        password: String,
        private_key_path: Option<String>,
        proxy_config: Option<proxy::ProxyConfig>,
        rows: u16,
        cols: u16,
        agent_forwarding: bool,
        x11_forwarding: bool,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let addr = format!("{}:{}", host, port);

        let (write_tx, write_rx): (Sender<String>, _) = mpsc::channel();
        let (resize_tx, resize_rx): (Sender<(u16, u16)>, _) = mpsc::channel();
        let (kill_tx, kill_rx): (Sender<()>, _) = mpsc::channel();

        let handle = if let Some(proxy_config) = proxy_config {
            std::thread::spawn(move || {
                let result = Self::run_session_via_proxy(
                    &addr, &username, &password, private_key_path,
                    &proxy_config, rows, cols, agent_forwarding, x11_forwarding,
                    write_rx, resize_rx, kill_rx, &app_handle, &id,
                );
                if let Err(e) = result {
                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                    }));
                }
            })
        } else {
            std::thread::spawn(move || {
                let result = Self::run_session(
                    &addr, &username, &password, private_key_path, rows, cols,
                    write_rx, resize_rx, kill_rx, &app_handle, &id,
                );
                if let Err(e) = result {
                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                    }));
                }
            })
        };

        Ok(Self { write_tx, resize_tx, kill_tx: Some(kill_tx), handle: Some(handle) })
    }

    fn run_session(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        rows: u16,
        cols: u16,
        write_rx: Receiver<String>,
        resize_rx: Receiver<(u16, u16)>,
        kill_rx: Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let mut builder = ssh::create_session()
            .username(username)
            .timeout(Some(Duration::from_secs(10)));

        if let Some(ref key_path) = private_key_path {
            builder = builder.private_key_path(key_path);
        }

        builder = builder.password(password);

        let mut session = builder.connect(addr)
            .map_err(|e| format!("SSH connect failed: {}", e))?
            .run_local();

        let shell = session
            .open_shell_terminal(TerminalSize::from(cols as u32, rows as u32))
            .map_err(|e| format!("SSH shell open failed: {}", e))?;

        session.set_timeout(Some(Duration::from_millis(30)));

        let shell_ref = RefCell::new(shell);
        let mut write_shell = |data: &[u8]| -> Result<(), String> {
            shell_ref.borrow_mut().write(data).map_err(|e| format!("SSH write error: {}", e))
        };
        let mut read_shell = || -> Result<Vec<u8>, String> {
            shell_ref.borrow_mut().read().map_err(|e| e.to_string())
        };
        let mut do_resize = |r: u16, c: u16| -> Result<(), String> {
            // ssh-rs 0.3 does not expose a public resize API on LocalShell/ChannelShell.
            // Initial terminal dimensions are set at open_shell_terminal() call above.
            let _ = (r, c);
            Ok(())
        };

        Self::event_loop(&write_rx, &resize_rx, &kill_rx, app_handle, session_id, &mut write_shell, &mut read_shell, &mut do_resize)
    }

    fn run_session_via_proxy(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        proxy_config: &proxy::ProxyConfig,
        rows: u16,
        cols: u16,
        agent_forwarding: bool,
        x11_forwarding: bool,
        write_rx: Receiver<String>,
        resize_rx: Receiver<(u16, u16)>,
        kill_rx: Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        // Parse target addr
        let target_host = addr.rsplitn(2, ':').nth(1).unwrap_or(addr);
        let target_port: u16 = addr.rsplitn(2, ':').next().unwrap_or("22").parse().unwrap_or(22);

        // Connect through proxy
        let stream = proxy::connect(proxy_config, target_host, target_port)?;

        // Use ssh2 for the SSH session
        let mut sess = ssh2::Session::new().map_err(|e| format!("SSH2 session: {}", e))?;
        sess.set_tcp_stream(stream);
        sess.handshake().map_err(|e| format!("SSH2 handshake: {}", e))?;

        // Agent forwarding (available in libssh2, ssh2 crate may expose via raw sys)
        let _ = agent_forwarding;

        if let Some(ref key_path) = private_key_path {
            sess.userauth_pubkey_file(username, None, std::path::Path::new(key_path), None)
                .map_err(|e| format!("SSH2 key auth: {}", e))?;
        } else {
            sess.userauth_password(username, password)
                .map_err(|e| format!("SSH2 password auth: {}", e))?;
        }

        if !sess.authenticated() {
            return Err("SSH2 authentication failed".to_string());
        }

        // Open shell with PTY
        let mut channel = sess.channel_session()
            .map_err(|e| format!("SSH2 channel: {}", e))?;

        // X11 forwarding (available in libssh2, ssh2 crate may expose via raw sys)
        let _ = x11_forwarding;

        channel.request_pty_size(cols as u32, rows as u32, None, None)
            .map_err(|e| format!("SSH2 PTY: {}", e))?;
        channel.shell()
            .map_err(|e| format!("SSH2 shell: {}", e))?;

        // Non-blocking for event loop
        sess.set_blocking(false);

        let channel_ref = RefCell::new(channel);
        let mut write_channel = |data: &[u8]| -> Result<(), String> {
            channel_ref.borrow_mut().write_all(data).map_err(|e| format!("SSH2 write: {}", e))
        };
        let mut read_channel = || -> Result<Vec<u8>, String> {
            let mut buf = vec![0u8; 65536];
            match channel_ref.borrow_mut().read(&mut buf) {
                Ok(0) => Ok(Vec::new()),
                Ok(n) => {
                    buf.truncate(n);
                    Ok(buf)
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(Vec::new()),
                Err(e) => Err(e.to_string()),
            }
        };
        let mut do_resize = |r: u16, c: u16| -> Result<(), String> {
            channel_ref.borrow_mut().request_pty_size(c as u32, r as u32, None, None)
                .map_err(|e| format!("SSH2 resize: {}", e))
        };

        let result = Self::event_loop(&write_rx, &resize_rx, &kill_rx, app_handle, session_id, &mut write_channel, &mut read_channel, &mut do_resize);

        let mut channel = channel_ref.into_inner();
        let _ = channel.close();
        let _ = channel.wait_close();
        result
    }

    fn event_loop(
        write_rx: &Receiver<String>,
        resize_rx: &Receiver<(u16, u16)>,
        kill_rx: &Receiver<()>,
        app_handle: &AppHandle,
        session_id: &str,
        write_shell: &mut impl FnMut(&[u8]) -> Result<(), String>,
        read_shell: &mut impl FnMut() -> Result<Vec<u8>, String>,
        do_resize: &mut impl FnMut(u16, u16) -> Result<(), String>,
    ) -> Result<(), String> {
        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            while let Ok((r, c)) = resize_rx.try_recv() {
                do_resize(r, c)?;
            }

            while let Ok(data) = write_rx.try_recv() {
                write_shell(data.as_bytes())?;
            }

            match read_shell() {
                Ok(data) => {
                    if data.is_empty() {
                        // non-blocking: no data, sleep and continue
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }

                    // Zmodem detection
                    if zmodem::detect_init(&data) {
                        let _ = app_handle.emit("zmodem-start", serde_json::json!({
                            "session_id": session_id,
                        }));

                        let mut buf: Vec<u8> = data.to_vec();

                        let mut z_write = |d: &[u8]| write_shell(d);
                        let mut z_read = || read_shell();

                        let _ = zmodem_loop(
                            write_rx, kill_rx, app_handle, session_id,
                            &mut buf, &mut z_write, &mut z_read,
                        );

                        continue;
                    }

                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": session_id, "data": String::from_utf8_lossy(&data),
                    }));
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    pub fn write(&self, data: &str) -> Result<(), String> {
        self.write_tx.send(data.to_string()).map_err(|e| e.to_string())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.resize_tx.send((rows, cols)).map_err(|e| e.to_string())
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
