use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use futures::future::BoxFuture;
use russh::client;
use russh::ChannelMsg;
use russh::keys::*;
use tauri::{AppHandle, Emitter, Manager};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self as tokio_mpsc, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use crate::proxy;
use crate::zmodem;
use crate::session::{Connection, Capability};

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

#[allow(dead_code)]
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
        _agent_forwarding: bool,
        _x11_forwarding: bool,
        app_handle: AppHandle,
    ) -> Result<Self, String> {
        let addr = crate::netaddr::sock_addr(&host, port);

        let (write_tx, write_rx) = tokio_mpsc::unbounded_channel();
        let (resize_tx, resize_rx) = tokio_mpsc::unbounded_channel();
        let (kill_tx, kill_rx) = tokio_mpsc::unbounded_channel();
        let (exec_tx, exec_rx) = tokio_mpsc::unbounded_channel();

        let session_id = id.clone();

        if let Some(proxy_config) = proxy_config {
            let handle = std::thread::spawn(move || {
                let rt = match Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("[ssh] Failed to create tokio runtime: {}", e);
                        return;
                    }
                };
                rt.block_on(async {
                    let result = Self::run_session_via_proxy_async(
                        &addr, &username, &password, private_key_path,
                        &proxy_config, rows, cols,
                        write_rx, resize_rx, kill_rx, exec_rx,
                        &app_handle, &session_id,
                    ).await;
                    let err = result.err();
                    if let Some(e) = &err {
                        log::error!("[ssh] Session error ({}): {}", session_id, e);
                        let _ = app_handle.emit("terminal-output", serde_json::json!({
                            "session_id": session_id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                        }));
                    }
                    let _ = app_handle.emit("session-status", serde_json::json!({
                        "session_id": session_id, "status": "disconnected", "error": err,
                    }));
                });
            });

            Ok(Self {
                write_tx, resize_tx,
                kill_tx: Some(kill_tx), exec_tx: Some(exec_tx),
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
                    let err = result.err();
                    if let Some(e) = &err {
                        log::error!("[ssh] Session error ({}): {}", session_id, e);
                        let _ = app_handle.emit("terminal-output", serde_json::json!({
                            "session_id": session_id, "data": format!("\r\n[SSH Error: {}]\r\n", e),
                        }));
                    }
                    let _ = app_handle.emit("session-status", serde_json::json!({
                        "session_id": session_id, "status": "disconnected", "error": err,
                    }));
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
        write_rx: UnboundedReceiver<String>,
        resize_rx: UnboundedReceiver<(u16, u16)>,
        kill_rx: UnboundedReceiver<()>,
        exec_rx: UnboundedReceiver<(String, ExecResponse)>,
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

        Self::run_event_loop(
            write_rx, resize_rx, kill_rx, exec_rx,
            &mut channel, &handle, app_handle, session_id,
        ).await?;

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

    async fn run_session_via_proxy_async(
        addr: &str,
        username: &str,
        password: &str,
        private_key_path: Option<String>,
        proxy_config: &proxy::ProxyConfig,
        rows: u16,
        cols: u16,
        write_rx: UnboundedReceiver<String>,
        resize_rx: UnboundedReceiver<(u16, u16)>,
        kill_rx: UnboundedReceiver<()>,
        exec_rx: UnboundedReceiver<(String, ExecResponse)>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        let (target_host, target_port) = crate::netaddr::split_host_port(addr);
        let stream = proxy::connect_async(proxy_config, &target_host, target_port).await?;

        let config = Arc::new(client::Config::default());
        let mut handle = client::connect_stream(config, stream, SshHandler).await
            .map_err(|e| format!("SSH connect via proxy failed: {}", e))?;

        Self::authenticate(&mut handle, username, password, private_key_path.as_deref()).await?;

        let mut channel = handle.channel_open_session().await
            .map_err(|e| format!("Failed to open session channel: {}", e))?;

        channel.request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[]).await
            .map_err(|e| format!("PTY request failed: {}", e))?;

        channel.request_shell(true).await
            .map_err(|e| format!("Shell request failed: {}", e))?;

        // Reuse the same event loop code as run_session_async
        let _ = app_handle.emit("session-status", serde_json::json!({
            "session_id": session_id, "status": "connected",
        }));

        Self::run_event_loop(
            write_rx, resize_rx, kill_rx, exec_rx,
            &mut channel, &handle, app_handle, session_id,
        ).await?;

        let _ = channel.close().await;
        Ok(())
    }

    async fn run_event_loop(
        mut write_rx: UnboundedReceiver<String>,
        mut resize_rx: UnboundedReceiver<(u16, u16)>,
        mut kill_rx: UnboundedReceiver<()>,
        mut exec_rx: UnboundedReceiver<(String, ExecResponse)>,
        channel: &mut russh::Channel<russh::client::Msg>,
        handle: &client::Handle<SshHandler>,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<(), String> {
        use russh::ChannelMsg;

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
                    let result = Self::exec_on_handle(handle, &cmd).await;
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

impl Connection for SshConnection {
    fn write(&mut self, data: &str) -> Result<(), String> {
        SshConnection::write(self, data)
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.resize(rows, cols)
    }

    fn kill(&mut self) {
        self.kill()
    }

    fn exec(&self, cmd: &str) -> BoxFuture<'static, Result<String, String>> {
        let exec_tx = self.exec_tx.clone();
        let cmd = cmd.to_string();
        Box::pin(async move {
            let tx = exec_tx.ok_or("SSH exec unavailable")?;
            let (resp_tx, resp_rx) = oneshot::channel();
            tx.send((cmd, resp_tx)).map_err(|e| format!("Exec send: {}", e))?;
            resp_rx.await.map_err(|e| format!("Exec recv: {}", e))?
        })
    }

    fn capabilities(&self) -> &'static [Capability] {
        crate::session::CAP_FILE_TUNNEL_EXEC_ZMODEM
    }
}
