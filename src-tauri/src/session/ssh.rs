use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use russh::client;
use russh::ChannelMsg;
use russh::keys::*;
use tauri::{AppHandle, Emitter, Manager};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self as tokio_mpsc, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use crate::proxy;
use crate::zmodem;

type ExecResponse = oneshot::Sender<Result<String, String>>;

pub struct SshConnection {
    pub write_tx: UnboundedSender<String>,
    resize_tx: UnboundedSender<(u16, u16)>,
    kill_tx: Option<UnboundedSender<()>>,
    exec_tx: Option<UnboundedSender<(String, ExecResponse)>>,
    handle: Option<JoinHandle<()>>,
}

struct SshHandler;

impl client::Handler for SshHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
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

        let (write_tx, write_rx) = tokio_mpsc::unbounded_channel();
        let (resize_tx, resize_rx) = tokio_mpsc::unbounded_channel();
        let (kill_tx, kill_rx) = tokio_mpsc::unbounded_channel();
        let (exec_tx, exec_rx) = tokio_mpsc::unbounded_channel();

        let session_id = id.clone();

        if let Some(proxy_config) = proxy_config {
            // Proxy path — keep using ssh2 (sync, unchanged)
            let (sync_write_tx, sync_write_rx): (Sender<String>, _) = mpsc::channel();
            let (sync_resize_tx, sync_resize_rx): (Sender<(u16, u16)>, _) = mpsc::channel();
            let (sync_kill_tx, sync_kill_rx): (Sender<()>, _) = mpsc::channel();
            let (sync_exec_tx, sync_exec_rx): (Sender<(String, oneshot::Sender<Result<String, String>>)>, _) = mpsc::channel();

            // Bridge the tokio channels to std channels
            let write_tx2 = write_tx.clone();
            let resize_tx2 = resize_tx.clone();
            let exec_tx2 = exec_tx.clone();
            std::thread::spawn(move || {
                let rt = match Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("[ssh] Failed to create tokio runtime: {}", e);
                        return;
                    }
                };
                rt.block_on(async {
                    let mut write_rx = write_rx;
                    let mut resize_rx = resize_rx;
                    let mut kill_rx = kill_rx;
                    let mut exec_rx = exec_rx;
                    loop {
                        tokio::select! {
                            Some(data) = write_rx.recv() => {
                                let _ = sync_write_tx.send(data);
                            }
                            Some((r, c)) = resize_rx.recv() => {
                                let _ = sync_resize_tx.send((r, c));
                            }
                            Some((cmd, resp)) = exec_rx.recv() => {
                                let _ = sync_exec_tx.send((cmd, resp));
                            }
                            _ = kill_rx.recv() => {
                                let _ = sync_kill_tx.send(());
                                break;
                            }
                            else => break,
                        }
                    }
                });
            });

            let handle = std::thread::spawn(move || {
                let result = Self::run_session_via_proxy(
                    &addr, &username, &password, private_key_path,
                    &proxy_config, rows, cols, agent_forwarding, x11_forwarding,
                    sync_write_rx, sync_resize_rx, sync_kill_rx, sync_exec_rx,
                    &app_handle, &session_id,
                );
                if let Err(e) = result {
                    let _ = app_handle.emit("terminal-output", serde_json::json!({
                        "session_id": session_id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                    }));
                    let _ = app_handle.emit("session-status", serde_json::json!({
                        "session_id": session_id, "status": "disconnected", "error": e,
                    }));
                }
            });

            Ok(Self {
                write_tx: write_tx2, resize_tx: resize_tx2,
                kill_tx: Some(kill_tx), exec_tx: Some(exec_tx2),
                handle: Some(handle),
            })
        } else {
            let handle = std::thread::spawn(move || {
                let rt = match Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("[ssh] Failed to create tokio runtime: {}", e);
                        return;
                    }
                };
                rt.block_on(async {
                    let result = Self::run_session_async(
                        &addr, &username, &password, private_key_path,
                        rows, cols, write_rx, resize_rx, kill_rx, exec_rx,
                        &app_handle, &session_id,
                    ).await;
                    if let Err(e) = result {
                        log::error!("[ssh] Session error ({}): {}", session_id, e);
                        let _ = app_handle.emit("terminal-output", serde_json::json!({
                            "session_id": session_id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                        }));
                        let _ = app_handle.emit("session-status", serde_json::json!({
                            "session_id": session_id, "status": "disconnected", "error": e,
                        }));
                    }
                });
            });

            Ok(Self {
                write_tx, resize_tx,
                kill_tx: Some(kill_tx), exec_tx: Some(exec_tx),
                handle: Some(handle),
            })
        }
    }

    async fn authenticate(
        handle: &mut client::Handle<SshHandler>,
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

    async fn run_session_async(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        rows: u16,
        cols: u16,
        mut write_rx: UnboundedReceiver<String>,
        mut resize_rx: UnboundedReceiver<(u16, u16)>,
        mut kill_rx: UnboundedReceiver<()>,
        mut exec_rx: UnboundedReceiver<(String, ExecResponse)>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let config = Arc::new(client::Config::default());
        let mut handle = client::connect(config, addr, SshHandler).await
            .map_err(|e| format!("SSH connect failed: {}", e))?;

        Self::authenticate(&mut handle, username, password, private_key_path.as_deref()).await?;

        let mut channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open session channel: {}", e))?;

        channel.request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[]).await
            .map_err(|e| format!("PTY request failed: {}", e))?;

        channel.request_shell(true).await
            .map_err(|e| format!("Shell request failed: {}", e))?;

        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": session_id, "status": "connected",
        }));

        let mut emit_count = 0u64;
        let mut emit_bytes = 0u64;

        loop {
            tokio::select! {
                Some(data) = write_rx.recv() => {
                    let _ = channel.data_bytes(data).await;
                }
                Some((r, c)) = resize_rx.recv() => {
                    let _ = channel.window_change(c as u32, r as u32, 0, 0).await;
                }
                Some((cmd, resp)) = exec_rx.recv() => {
                    let result = Self::exec_on_handle(&handle, &cmd).await;
                    let _ = resp.send(result);
                }
                _ = kill_rx.recv() => {
                    break;
                }
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            let raw = data.to_vec();
                            emit_count += 1;
                            emit_bytes += raw.len() as u64;
                            if emit_count <= 5 || emit_count % 100 == 0 {
                                log::info!("[ssh-event-loop] emit #{} {}B (total {}B) session={}", emit_count, raw.len(), emit_bytes, session_id);
                            }

                            // Zmodem detection — async path: skip zmodem_loop for now
                            if zmodem::detect_init(&raw) {
                                let _ = app_handle.emit("zmodem-start", serde_json::json!({
                                    "session_id": session_id,
                                }));
                                let data_str = String::from_utf8_lossy(&raw);
                                let _ = app_handle.emit("terminal-output", serde_json::json!({
                                    "session_id": session_id, "data": data_str,
                                }));
                                let _ = app_handle.emit("zmodem-end", serde_json::json!({
                                    "session_id": session_id,
                                }));
                                continue;
                            }

                            let data_str = String::from_utf8_lossy(&raw);
                            let _ = app_handle.emit("terminal-output", serde_json::json!({
                                "session_id": session_id, "data": data_str,
                            }));
                        }
                        Some(ChannelMsg::ExtendedData { ext: 1, data }) => {
                            let data_str = String::from_utf8_lossy(&data);
                            let _ = app_handle.emit("terminal-output", serde_json::json!({
                                "session_id": session_id, "data": data_str,
                            }));
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        let _ = channel.close().await;
        Ok(())
    }

    async fn exec_on_handle(
        handle: &client::Handle<SshHandler>,
        command: &str,
    ) -> Result<String, String> {
        let mut channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open exec channel: {}", e))?;
        channel.exec(true, command).await
            .map_err(|e| format!("Exec request failed: {}", e))?;

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
        String::from_utf8(output).map_err(|e| format!("UTF-8 error: {}", e))
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
        exec_rx: Receiver<(String, oneshot::Sender<Result<String, String>>)>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let target_host = addr.rsplitn(2, ':').nth(1).unwrap_or(addr);
        let target_port: u16 = addr.rsplitn(2, ':').next().unwrap_or("22").parse().unwrap_or(22);

        let stream = proxy::connect(proxy_config, target_host, target_port)?;

        let mut sess = ssh2::Session::new().map_err(|e| format!("SSH2 session: {}", e))?;
        sess.set_tcp_stream(stream);
        sess.handshake().map_err(|e| format!("SSH2 handshake: {}", e))?;

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

        let mut channel = sess.channel_session()
            .map_err(|e| format!("SSH2 channel: {}", e))?;

        let _ = x11_forwarding;

        channel.request_pty_size(cols as u32, rows as u32, None, None)
            .map_err(|e| format!("SSH2 PTY: {}", e))?;
        channel.shell()
            .map_err(|e| format!("SSH2 shell: {}", e))?;

        sess.set_blocking(false);

        let channel_ref = RefCell::new(channel);

        let write_channel = |data: &[u8]| -> Result<(), String> {
            channel_ref.borrow_mut().write_all(data).map_err(|e| format!("SSH2 write: {}", e))
        };
        let read_channel = || -> Result<Vec<u8>, String> {
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
        let do_resize = |r: u16, c: u16| -> Result<(), String> {
            channel_ref.borrow_mut().request_pty_size(c as u32, r as u32, None, None)
                .map_err(|e| format!("SSH2 resize: {}", e))
        };

        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": session_id, "status": "connected",
        }));

        let result = Self::event_loop(
            &write_rx, &resize_rx, &kill_rx, &exec_rx, &sess,
            app_handle, session_id, &write_channel, &read_channel, &do_resize,
        );

        let mut channel = channel_ref.into_inner();
        let _ = channel.close();
        let _ = channel.wait_close();
        result
    }

    fn event_loop(
        write_rx: &Receiver<String>,
        resize_rx: &Receiver<(u16, u16)>,
        kill_rx: &Receiver<()>,
        exec_rx: &Receiver<(String, oneshot::Sender<Result<String, String>>)>,
        sess: &ssh2::Session,
        app_handle: &AppHandle,
        session_id: &str,
        write_shell: impl Fn(&[u8]) -> Result<(), String>,
        read_shell: impl Fn() -> Result<Vec<u8>, String>,
        do_resize: impl Fn(u16, u16) -> Result<(), String>,
    ) -> Result<(), String> {
        let write_shell = write_shell;
        let read_shell = read_shell;
        let do_resize = do_resize;
        let mut emit_count = 0u64;
        let mut emit_bytes = 0u64;
        loop {
            if kill_rx.try_recv().is_ok() {
                break;
            }

            // Handle exec requests
            while let Ok((cmd, resp)) = exec_rx.try_recv() {
                let result = Self::exec_on_session(sess, &cmd);
                let _ = resp.send(result);
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
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }

                    emit_count += 1;
                    emit_bytes += data.len() as u64;
                    if emit_count <= 5 || emit_count % 100 == 0 {
                        log::info!("[ssh-event-loop] emit #{} {}B (total {}B) session={}", emit_count, data.len(), emit_bytes, session_id);
                    }

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

    fn exec_on_session(
        sess: &ssh2::Session,
        command: &str,
    ) -> Result<String, String> {
        let mut channel = sess.channel_session()
            .map_err(|e| format!("SSH2 exec channel: {}", e))?;
        channel.exec(command)
            .map_err(|e| format!("SSH2 exec: {}", e))?;

        let mut output = Vec::new();
        let mut buf = [0u8; 65536];
        loop {
            match channel.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => return Err(format!("SSH2 read: {}", e)),
            }
        }
        let _ = channel.close();
        let _ = channel.wait_close();
        String::from_utf8(output).map_err(|e| format!("UTF-8 error: {}", e))
    }

    pub fn exec_tx(&self) -> Option<UnboundedSender<(String, ExecResponse)>> {
        self.exec_tx.clone()
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
