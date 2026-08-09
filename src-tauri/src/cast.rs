//! Screen casting via the scrcpy-server standalone mode.
//!
//! The scrcpy server is pushed to the device and started with
//! `raw_stream=true`, which turns the video socket into a plain H.264
//! Annex-B byte stream (no codec meta, no frame headers). We connect over
//! `adb forward`, split the stream into NAL units, keep the latest frame and
//! the latest SPS/PPS-derived avcC config, and hand both to the frontend where
//! WebCodecs decodes and renders. Touch/keyboard input is injected back with
//! `adb shell input`, so no control socket is needed.

use std::collections::HashMap;
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use tauri::{AppHandle, Manager};

use crate::adb::ADB_PORT;

/// Must match the scrcpy-server jar version (the server refuses mismatches).
pub const SCRCPY_VERSION: &str = "4.1";
const REMOTE_JAR: &str = "/data/local/tmp/scrcpy-server.jar";

// One-shot diagnostic flags (reset on each `start`) so the "first ..." logs do
// not spam on a live stream.
static LOG_SPS: AtomicBool = AtomicBool::new(false);
static LOG_PPS: AtomicBool = AtomicBool::new(false);
static LOG_IDR: AtomicBool = AtomicBool::new(false);
static LOG_VCL: AtomicBool = AtomicBool::new(false);
static LOG_CONFIG: AtomicBool = AtomicBool::new(false);
static POLL_COUNT: AtomicU64 = AtomicU64::new(0);

fn hex_prefix(b: &[u8]) -> String {
    b.iter()
        .take(16)
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Default)]
pub struct CastState {
    sessions: Mutex<HashMap<String, Arc<CastSession>>>,
}

struct CastSession {
    stream: Mutex<Option<TcpStream>>,
    child: Mutex<Option<std::process::Child>>,
    local_port: u16,
    frame: Arc<Mutex<FrameSlot>>,
    killed: Arc<AtomicBool>,
}

#[derive(Clone)]
struct FrameSlot {
    seq: u64,
    key: bool,
    data: Vec<u8>,
    config: Option<Vec<u8>>,
    /// The most recent IDR frame (avc format) with its avcC. The controller
    /// keeps only the newest frame, so after a long delta-only stretch the
    /// frontend could otherwise wait ~10s (I-frame interval) before it gets a
    /// keyframe; on startup it must HAVE a keyframe to configure WebCodecs.
    last_key: Option<(Vec<u8>, Option<Vec<u8>>)>,
}

/// Locate the scrcpy-server jar: `AIDTERM_SCRCPY` env, bundled resource, or error.
/// Walk from `start` toward the filesystem root looking for `bin/scrcpy-server.jar`.
fn find_jar_upwards(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("bin").join("scrcpy-server.jar");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

pub fn scrcpy_jar(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("AIDTERM_SCRCPY") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        log::warn!("[cast] AIDTERM_SCRCPY set but not a file, ignoring: {}", p.display());
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("bin").join("scrcpy-server.jar");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    // Dev fallbacks: `npm run tauri dev` keeps the repo root as CWD, but the
    // process can also be started from a subdirectory or a built binary, so
    // walk up from both the CWD and the executable directory.
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = find_jar_upwards(&cwd) {
            return Ok(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(p) = find_jar_upwards(parent) {
                return Ok(p);
            }
        }
    }
    Err("scrcpy-server.jar not found. Run `npm run fetch-scrcpy`, or set AIDTERM_SCRCPY to the jar path.".to_string())
}

#[cfg(target_os = "windows")]
fn adb_cmd(app: &AppHandle) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(crate::adb::adb_path(app)?);
    cmd.arg("-P").arg(ADB_PORT);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    Ok(cmd)
}

#[cfg(not(target_os = "windows"))]
fn adb_cmd(app: &AppHandle) -> Result<Command, String> {
    let mut cmd = Command::new(crate::adb::adb_path(app)?);
    cmd.arg("-P").arg(ADB_PORT);
    Ok(cmd)
}

fn is_vcl(t: u8) -> bool {
    (1..=5).contains(&t)
}

/// Strip emulation-prevention bytes (0x03 injected after two zeros) so slice
/// headers can be parsed bit-exactly. `nal` starts AFTER the 1-byte NAL type
/// header? No — it starts AT it; callers pass the NAL payload including the
/// type byte and we skip it below.
fn nal_rbsp(nal_no_header: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal_no_header.len());
    let mut zeros = 0u8;
    for &b in nal_no_header {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        out.push(b);
        zeros = if b == 0 { zeros + 1 } else { 0 };
    }
    out
}

fn bit_at(rbsp: &[u8], bit: usize) -> bool {
    (rbsp[bit / 8] >> (7 - (bit % 8))) & 1 == 1
}

/// True when the VCL NAL is the *first* slice of an access unit, i.e. its
/// slice header has `first_mb_in_slice == 0` (the first Exp-Golomb field after
/// the NAL type). A single AVC picture can be split across several VCL NALs
/// (multi-slice encoding, very common for IDR frames), and feeding the second
/// half of an IDR as an independent "key frame" to WebCodecs fails with
/// `was marked as type 'key' but wasn't a key frame`. Merging slices by
/// first-mb detection is the same trick scrcpy uses.
fn au_slice_boundary(nal_payload: &[u8]) -> bool {
    let rbsp = nal_rbsp(nal_payload);
    if rbsp.len() < 2 {
        // Weird NAL, be conservative: treat it as an AU start so no data is
        // ever swallowed into a bigger frame (worst case: a rare split).
        return true;
    }
    // Skip the 1-byte NAL type header, then decode the first Exp-Golomb
    // (ue(v)) value: first_mb_in_slice == 0 iff the ue code starts with a '1'
    // right away (leading zeros == 0).
    let mut bit = 8usize;
    let nbits = rbsp.len() * 8;
    let mut zeros = 0usize;
    while bit < nbits && !bit_at(&rbsp, bit) {
        zeros += 1;
        bit += 1;
    }
    // The mandatory terminating '1' should always exist; guard against a
    // truncated payload by conservatively accepting the boundary.
    if bit >= nbits {
        return true;
    }
    zeros == 0
}

/// Locate the next Annex-B start code at or after `pos`.
fn find_start_code(buf: &[u8], pos: usize) -> Option<(usize, usize)> {
    let mut i = pos;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            return Some((i, 3));
        }
        if i + 4 <= buf.len() && buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 0 && buf[i + 3] == 1 {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

/// Convert an Annex-B NAL stream (start-code delimited) into the avc format
/// (4-byte big-endian length prefix per NAL) that WebCodecs expects when it is
/// configured with an avcC `description`. `frame` is already trimmed of the
/// trailing zero bytes that belong to the next start code (see `feed`), so the
/// NAL bodies are clean. Emulation-prevention guarantees no false start codes
/// inside NAL payloads.
fn annexb_to_avc(frame: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(frame.len() + frame.len() / 4);
    let mut pos = 0;
    while let Some((sc, sc_len)) = find_start_code(frame, pos) {
        let nal_start = sc + sc_len;
        let next = find_start_code(frame, nal_start);
        let nal_end = next.map(|(ns, _)| ns).unwrap_or(frame.len());
        if nal_end > nal_start {
            out.extend_from_slice(&((nal_end - nal_start) as u32).to_be_bytes());
            out.extend_from_slice(&frame[nal_start..nal_end]);
        }
        pos = nal_end;
    }
    out
}

/// Feed newly arrived bytes into the NAL splitter.
///
/// `pending` is the partially accumulated stream, `cur` the frame being built
/// (Annex-B), `sps`/`pps` hold the latest parameter sets. When a new VCL NAL
/// starts, the previous frame is emitted into `slot`. A NAL is only processed
/// once the start code of the *next* NAL is visible, so partial reads (TCP
/// fragmentation) can never truncate it.
fn feed(
    pending: &mut Vec<u8>,
    data: &[u8],
    cur: &mut Vec<u8>,
    has_vcl: &mut bool,
    sps: &mut Option<Vec<u8>>,
    pps: &mut Option<Vec<u8>>,
    slot: &Mutex<FrameSlot>,
) {
    pending.extend_from_slice(data);
    loop {
        // Collect every start code currently visible in the buffer.
        let mut codes: Vec<(usize, usize)> = Vec::new();
        let mut p = 0usize;
        while let Some((sc, l)) = find_start_code(pending, p) {
            codes.push((sc, l));
            p = sc + l;
        }
        if codes.len() < 2 {
            return; // need the next start code to delimit the first NAL
        }
        let boundary = codes.last().unwrap().0; // everything before it is complete
        for w in codes.windows(2) {
            let (sc, sc_len) = w[0];
            let next_sc = w[1].0;
            let nal_start = sc + sc_len;
            if nal_start >= next_sc {
                continue;
            }
            let mut body_end = next_sc;
            while body_end > nal_start && pending[body_end - 1] == 0 {
                body_end -= 1; // trailing zeros belong to the next start code
            }
            if body_end <= nal_start {
                continue;
            }
            let nal_type = pending[nal_start] & 0x1F;
            if is_vcl(nal_type) {
                if !LOG_VCL.swap(true, Ordering::Relaxed) {
                    log::info!("[cast] first VCL NAL, type {nal_type}");
                }
                if nal_type == 5 && !LOG_IDR.swap(true, Ordering::Relaxed) {
                    log::info!("[cast] first IDR (keyframe) NAL captured");
                }
                // A new access unit starts at a slice with first_mb_in_slice==0.
                // Everything accumulated so far (SPS/PPS + slices of the
                // previous picture) is a complete frame; emit it NOW so a
                // multi-slice picture never leaks into the next frame.
                let au_start = au_slice_boundary(&pending[nal_start..body_end]);
                if *has_vcl && au_start {
                    emit(cur, sps.as_deref(), pps.as_deref(), slot);
                    cur.clear();
                }
                cur.extend_from_slice(&pending[sc..body_end]);
                *has_vcl = true;
            } else {
                match nal_type {
                    7 => {
                        if !LOG_SPS.swap(true, Ordering::Relaxed) {
                            log::info!(
                                "[cast] first SPS captured, {} bytes",
                                body_end - nal_start - 1
                            );
                        }
                        *sps = Some(pending[nal_start + 1..body_end].to_vec());
                    }
                    8 => {
                        if !LOG_PPS.swap(true, Ordering::Relaxed) {
                            log::info!(
                                "[cast] first PPS captured, {} bytes",
                                body_end - nal_start - 1
                            );
                        }
                        *pps = Some(pending[nal_start + 1..body_end].to_vec());
                    }
                    _ => {}
                }
                cur.extend_from_slice(&pending[sc..body_end]);
            }
        }
        pending.drain(..boundary);
    }
}

fn emit(cur: &[u8], sps: Option<&[u8]>, pps: Option<&[u8]>, slot: &Mutex<FrameSlot>) {
    let mut s = slot.lock().unwrap();
    s.seq += 1;
    if s.seq == 1 {
        log::info!("[cast] first frame emitted, {} bytes", cur.len());
    }
    // The frontend configures WebCodecs with the avcC `description`, so chunks
    // must be in avc format (4-byte length-prefixed NALs), not Annex-B.
    let frame = annexb_to_avc(cur);
    s.data = frame;
    // The first VCL NAL in the frame carries the IDR marker.
    s.key = has_idr(cur);
    if let (Some(sps), Some(pps)) = (sps, pps) {
        if let Some(cfg) = build_avcc(sps, pps) {
            if !LOG_CONFIG.swap(true, Ordering::Relaxed) {
                log::info!("[cast] avcC built from SPS/PPS: {}", hex_prefix(&cfg));
            }
            s.config = Some(cfg);
        }
    }
    // Keep the newest IDR frame around: the frontend must start decoding with
    // a keyframe (WebCodecs rejects delta chunks after configure()), but the
    // device may produce deltas for seconds before the next I-frame arrives.
    // NOTE: must run AFTER s.config is set, or the cached keyframe ships with
    // config=None and the frontend cannot (re)configure.
    if s.key {
        s.last_key = Some((s.data.clone(), s.config.clone()));
    }
    if s.seq % 100 == 0 {
        log::info!(
            "[cast] frame seq={} key={} bytes={} config={}",
            s.seq,
            s.key,
            s.data.len(),
            s.config.is_some()
        );
    }
}

fn has_idr(frame: &[u8]) -> bool {
    let mut pos = 0;
    while let Some((sc, sc_len)) = find_start_code(frame, pos) {
        let i = sc + sc_len;
        if i < frame.len() && (frame[i] & 0x1F) == 5 {
            return true;
        }
        pos = i;
    }
    false
}

/// Build an avcC record from raw SPS/PPS payloads (without NAL header bytes).
fn build_avcc(sps: &[u8], pps: &[u8]) -> Option<Vec<u8>> {
    if sps.len() < 4 {
        return None;
    }
    let mut out = Vec::with_capacity(sps.len() + pps.len() + 11);
    out.push(0x01); // configurationVersion
    out.push(sps[0]); // profile_idc
    out.push(sps[1]); // profile_compatibility
    out.push(sps[2]); // level_idc
    out.push(0xFF); // reserved(6) + lengthSizeMinusOne(3)
    out.push(0xE1); // reserved(3) + numOfSPS(5)
    out.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    out.extend_from_slice(sps);
    out.push(0x01); // numOfPPS
    out.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    out.extend_from_slice(pps);
    Some(out)
}

fn spawn_reader(
    stream: TcpStream,
    initial: Vec<u8>,
    slot: Arc<Mutex<FrameSlot>>,
    killed: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        // `initial` is the chunk already consumed by the connect probe; it
        // usually carries the first SPS/PPS/IDR, so seed the parser with it.
        let mut pending: Vec<u8> = initial;
        let mut cur: Vec<u8> = Vec::new();
        let mut has_vcl = false;
        let mut sps: Option<Vec<u8>> = None;
        let mut pps: Option<Vec<u8>> = None;
        let mut tmp = [0u8; 32768];
        let mut reader = match stream.try_clone() {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut first_read = true;
        let mut reads: u64 = 0;
        let mut bytes: u64 = 0;
        let mut errors: u64 = 0;
        log::info!("[cast] reader started");
        loop {
            if killed.load(Ordering::Relaxed) {
                break;
            }
            let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));
            match reader.read(&mut tmp) {
                Ok(0) => {
                    log::info!("[cast] reader EOF after {reads} reads / {bytes} bytes");
                    break;
                }
                Ok(n) => {
                    reads += 1;
                    bytes += n as u64;
                    if first_read {
                        log::info!(
                            "[cast] reader first read: {n} bytes, head: {}",
                            hex_prefix(&tmp[..n.min(16)])
                        );
                        first_read = false;
                    }
                    feed(&mut pending, &tmp[..n], &mut cur, &mut has_vcl, &mut sps, &mut pps, &slot);
                    if pending.len() > 8 * 1024 * 1024 {
                        pending.clear();
                    }
                }
                Err(e) => {
                    if killed.load(Ordering::Relaxed) {
                        break;
                    }
                    errors += 1;
                    if errors <= 5 || errors % 25 == 0 {
                        log::info!(
                            "[cast] reader read error #{errors}: {e} (after {reads} reads / {bytes} bytes)"
                        );
                    }
                }
            }
        }
    });
}

/// Start casting a device: push the jar, forward a port, launch the server.
pub fn start(app: &AppHandle, serial: &str, max_size: u32) -> Result<u16, String> {
    let state = app.state::<CastState>();
    let mut sessions = state.sessions.lock().unwrap();
    {
        let s = sessions.get(serial).cloned();
        if let Some(s) = s {
            if !s.killed.load(Ordering::Relaxed) {
                return Ok(s.local_port);
            }
        }
    }
    // Reap any killed leftover session (its scrcpy process is already stopped).
    sessions.remove(serial);

    let jar = scrcpy_jar(app)?;
    let local_port = pick_local_port();
    log::info!("[cast] jar: {}", jar.display());

    // 1. Push the server jar onto the device.
    let push = adb_cmd(app)?
        .args(["-s", serial, "push"])
        .arg(&jar)
        .arg(REMOTE_JAR)
        .output()
        .map_err(|e| format!("adb push failed: {}", e))?;
    if !push.status.success() {
        return Err(format!(
            "adb push failed: {}",
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }
    log::info!("[cast] pushed jar to {REMOTE_JAR}");

    // 2. Forward a local TCP port to the device's localabstract socket.
    //    The server derives its socket name as `scrcpy_%08x` from scid, so both
    //    must be written as zero-padded lowercase hex (no 0x prefix: the server
    //    parses scid with Integer.parseInt(value, 16) which rejects "0x").
    let scid = rand_31bit();
    let localabstract = format!("localabstract:scrcpy_{:08x}", scid);
    let fwd = adb_cmd(app)?
        .args(["-s", serial, "forward", &format!("tcp:{}", local_port), &localabstract])
        .output()
        .map_err(|e| format!("adb forward failed: {}", e))?;
    if !fwd.status.success() {
        return Err(format!(
            "adb forward failed: {}",
            String::from_utf8_lossy(&fwd.stderr).trim()
        ));
    }
    log::info!("[cast] forward tcp:{local_port} -> {localabstract}");

    // 3. Launch the server; it streams once we connect to the forwarded port.
    //    send_dummy_byte=true makes the server write a single 0x00 byte on the
    //    video socket right after it accepts our connection, so we can detect a
    //    real (accepted) connection before the encoder produces the first frame.
    let size = max_size.to_string();
    let mut shell = adb_cmd(app)?;
    let mut child = shell
        .args([
            "-s", serial, "shell",
            &format!("CLASSPATH={}", REMOTE_JAR),
            "app_process", "/", "com.genymobile.scrcpy.Server", SCRCPY_VERSION,
            "tunnel_forward=true", "audio=false", "control=false", "cleanup=false",
            "raw_stream=true", "send_dummy_byte=true", "video_codec_options=i-frame-interval=1",
            &format!("max_size={}", size),
            &format!("scid={:x}", scid),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start scrcpy server failed: {}", e))?;
    log::info!("[cast] scrcpy server launched: scid {scid:x} max_size {size}");
    // Surface the server's own logs (and crash output) so startup problems on
    // a device are visible instead of being swallowed by the null pipe.
    if let (Some(mut out), Some(mut err)) = (child.stdout.take(), child.stderr.take()) {
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                match out.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&buf[..n]);
                        for line in s.lines() {
                            log::info!("[cast][srv] {line}");
                        }
                    }
                }
            }
        });
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                match err.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&buf[..n]);
                        for line in s.lines() {
                            log::warn!("[cast][srv-err] {line}");
                        }
                    }
                }
            }
        });
    }

    // 4. Probe-connect. adb accepts the host-side TCP connection immediately,
    //    even before the device socket exists, and such a connection promptly
    //    EOFs (observed: `reader EOF after 0 reads` on attempt 0). So a
    //    successful connect is NOT enough — we read a chunk to confirm the
    //    server is actually streaming before handing the stream to the reader.
    //
    //    IMPORTANT: the scrcpy server accepts exactly ONE video connection
    //    (control/audio are disabled), and `send_dummy_byte=true` makes it
    //    write a single 0x00 byte right after accepting. If our read times out
    //    (300ms) the connection is still alive — the device-side adb handshake
    //    is just slow — and the server may ALREADY have accepted it. Dropping
    //    such a connection kills the stream (`Screen streaming stopped`) and the
    //    server exits, so every later probe EOFs. Therefore: only clean EOF
    //    (Ok(0), socket not created yet) triggers a retry; a read timeout
    //    commits the connection and the reader waits for the first bytes.
    let mut stream: Option<TcpStream> = None;
    let mut first_chunk: Vec<u8> = Vec::new();
    let mut committed_on_timeout = false;
    let mut probe = [0u8; 65536];
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut attempts: u32 = 0;
    while std::time::Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = adb_cmd(app)
                .map(|mut c| c.args(["forward", "--remove", &format!("tcp:{}", local_port)]).output());
            return Err("scrcpy server exited early".to_string());
        }
        attempts += 1;
        match TcpStream::connect(("127.0.0.1", local_port)) {
            Ok(s) => {
                if s.set_read_timeout(Some(Duration::from_millis(300))).is_err() {
                    drop(s);
                    continue;
                }
                let mut r = s.try_clone().map_err(|e| e.to_string())?;
                match r.read(&mut probe) {
                    Ok(0) => {
                        log::info!(
                            "[cast] probe attempt {attempts}: connected but EOF (device not ready), retry"
                        );
                        drop(s);
                    }
                    Ok(n) => {
                        first_chunk = probe[..n].to_vec();
                        stream = Some(s);
                        log::info!(
                            "[cast] probe attempt {attempts}: LIVE, {n} bytes first, head: {}",
                            hex_prefix(&probe[..n.min(16)])
                        );
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {
                        committed_on_timeout = true;
                        stream = Some(s);
                        log::info!(
                            "[cast] probe attempt {attempts}: connected, no data within 300ms — committing (server will accept + stream shortly)"
                        );
                        break;
                    }
                    Err(e) => {
                        log::info!(
                            "[cast] probe attempt {attempts}: connected but read error ({e}), retry"
                        );
                        drop(s);
                    }
                }
            }
            Err(e) => {
                log::info!("[cast] probe attempt {attempts}: connect failed ({e}), retry");
                thread::sleep(Duration::from_millis(100));
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let stream = match stream {
        Some(s) => s,
        None => {
            // Kill the server and remove the forward so a failed start does not
            // leak a zombie server/socket that poisons the next attempt.
            let _ = child.kill();
            let _ = child.wait();
            let _ = adb_cmd(app)
                .map(|mut c| c.args(["forward", "--remove", &format!("tcp:{}", local_port)]).output());
            return Err("scrcpy server did not deliver a live stream within 30s".to_string());
        }
    };
    if committed_on_timeout {
        log::info!("[cast] connected to port {local_port} scid {scid:x} (committed on timeout, waiting for stream)");
    } else {
        log::info!("[cast] connected to port {local_port} scid {scid:x} (live stream)");
    }

    let reader_stream = stream.try_clone().map_err(|e| e.to_string())?;
    let session = Arc::new(CastSession {
        stream: Mutex::new(Some(stream)),
        child: Mutex::new(Some(child)),
        local_port,
        frame: Arc::new(Mutex::new(FrameSlot { seq: 0, key: false, data: Vec::new(), config: None, last_key: None })),
        killed: Arc::new(AtomicBool::new(false)),
    });
    let slot = session.frame.clone();
    let killed = session.killed.clone();
    LOG_SPS.store(false, Ordering::Relaxed);
    LOG_PPS.store(false, Ordering::Relaxed);
    LOG_IDR.store(false, Ordering::Relaxed);
    LOG_VCL.store(false, Ordering::Relaxed);
    LOG_CONFIG.store(false, Ordering::Relaxed);
    spawn_reader(reader_stream, first_chunk, slot, killed);
    sessions.insert(serial.to_string(), session);
    Ok(local_port)
}

pub fn stop(app: &AppHandle, serial: &str) {
    let state = app.state::<CastState>();
    let mut sessions = state.sessions.lock().unwrap();
    if let Some(s) = sessions.remove(serial) {
        s.killed.store(true, Ordering::Relaxed);
        if let Some(stream) = s.stream.lock().unwrap().take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        if let Some(mut child) = s.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = adb_cmd(app)
            .map(|mut c| c.args(["forward", "--remove", &format!("tcp:{}", s.local_port)]).output());
    }
}

pub fn frame(
    app: &AppHandle,
    serial: &str,
    need_key: Option<bool>,
) -> Option<(u64, bool, String, Option<String>)> {
    let state = app.state::<CastState>();
    let sessions = state.sessions.lock().unwrap();
    let s = sessions.get(serial)?;
    let f = s.frame.lock().unwrap();
    let n = POLL_COUNT.fetch_add(1, Ordering::Relaxed);
    if n % 100 == 0 {
        log::info!("[cast] frame() poll #{n}, seq={}", f.seq);
    }
    if need_key.unwrap_or(false) && !f.key {
        // The frontend needs an IDR to (re)configure the decoder. Serve the
        // cached keyframe but report the CURRENT seq so the frontend's
        // seen-seq check keeps working (same seq = already served = skip).
        if let Some((data, config)) = &f.last_key {
            return Some((
                f.seq,
                true,
                B64.encode(data),
                config.as_ref().map(|c| B64.encode(c)),
            ));
        }
    }
    Some((
        f.seq,
        f.key,
        B64.encode(&f.data),
        f.config.as_ref().map(|c| B64.encode(c)),
    ))
}

pub fn input(app: &AppHandle, serial: &str, cmd: &str) -> Result<(), String> {
    let args: Vec<&str> = cmd.split_whitespace().collect();
    let mut full = vec!["-s", serial, "shell", "input"];
    full.extend_from_slice(&args);
    let output = adb_cmd(app)?
        .args(&full)
        .output()
        .map_err(|e| format!("adb input failed: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "adb input failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn pick_local_port() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let base = 20000 + (t.subsec_nanos() % 20000) as u16;
    base
}

fn rand_31bit() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    ((t.as_nanos() % 0x7FFF_FFFF) as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end probe against a live device using the REAL parser/reader
    /// code. Mirrors `start()`'s orchestration but with a local FrameSlot (no
    /// Tauri AppHandle needed), so a failure reproduces exactly what the app
    /// does. Marked `#[ignore]` because it needs a connected adb device.
    #[test]
    #[ignore]
    fn e2e_live_stream_parses_and_emits_frames() {
        let adb = std::env::var("AIDTERM_ADB").unwrap_or_else(|_| "../bin/adb.exe".to_string());
        if !std::path::Path::new(&adb).is_file() {
            panic!("adb not found at {adb} (set AIDTERM_ADB)");
        }
        let jar = std::env::var("AIDTERM_SCRCPY")
            .map(|p| std::path::PathBuf::from(p))
            .unwrap_or_else(|_| std::path::PathBuf::from("../bin/scrcpy-server.jar"));
        assert!(jar.is_file(), "jar missing: {}", jar.display());

        let out = Command::new(&adb).args(["-P", ADB_PORT, "devices"]).output().expect("adb devices");
        let text = String::from_utf8_lossy(&out.stdout);
        let serial = std::env::var("AIDTERM_TEST_SERIAL").unwrap_or_else(|_| {
            text.lines()
                .skip(1)
                .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
                .find(|s| !s.is_empty() && !s.starts_with('*'))
                .expect("no device found")
        });
        println!("[test] device: {serial}");

        let run = |args: &[&str]| -> String {
            let o = Command::new(&adb).args(["-P", ADB_PORT, "-s", &serial]).args(args).output().unwrap();
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        // cleanup stale
        let _ = run(&["forward", "--remove-all"]);

        // 1. push
        let push = run(&["push", jar.to_str().unwrap(), REMOTE_JAR]);
        println!("[test] push: {push}");
        // 2. forward
        let port = pick_local_port();
        let scid = rand_31bit();
        let la = format!("localabstract:scrcpy_{:08x}", scid);
        println!("[test] forward tcp:{port} -> {la}");
        run(&["forward", &format!("tcp:{port}"), &la]);
        // 3. launch server
        let mut child = Command::new(&adb)
            .args(["-P", ADB_PORT, "-s", &serial, "shell"])
            .arg(format!("CLASSPATH={REMOTE_JAR}"))
            .args([
                "app_process", "/", "com.genymobile.scrcpy.Server", SCRCPY_VERSION,
                "tunnel_forward=true", "audio=false", "control=false", "cleanup=false",
                "raw_stream=true", "send_dummy_byte=true", "video_codec_options=i-frame-interval=1", "max_size=1280",
                &format!("scid={scid:x}"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn server");
        let _ = child.stderr.take().map(|mut e| thread::spawn(move || { let mut b = [0u8; 1024]; while let Ok(n) = e.read(&mut b) { if n == 0 { break } let s = String::from_utf8_lossy(&b[..n]); for l in s.lines() { println!("[srv] {l}"); } } }));

        // 4. probe-commit (identical to start())
        let mut stream: Option<TcpStream> = None;
        let mut first_chunk: Vec<u8> = Vec::new();
        let mut probe = [0u8; 65536];
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut attempts = 0u32;
        let mut eofs = 0u32;
        while std::time::Instant::now() < deadline {
            attempts += 1;
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => {
                    if s.set_read_timeout(Some(Duration::from_millis(300))).is_err() { drop(s); continue; }
                    let mut r = s.try_clone().unwrap();
                    match r.read(&mut probe) {
                        Ok(0) => { eofs += 1; drop(s); }
                        Ok(n) => { first_chunk = probe[..n].to_vec(); stream = Some(s); break; }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => { stream = Some(s); break; }
                        Err(_) => { drop(s); }
                    }
                }
                Err(_) => {}
            }
            thread::sleep(Duration::from_millis(100));
        }
        let s = match stream {
            Some(s) => s,
            None => { println!("[test] NEVER LIVE attempts={attempts} eofs={eofs}"); let _ = child.kill(); let _ = run(&["forward", "--remove", &format!("tcp:{port}")]); panic!("no live stream"); }
        };
        println!("[test] LIVE attempts={attempts} eofs={eofs} first_chunk={} bytes: {}", first_chunk.len(), hex_prefix(&first_chunk));

        // 5. reader + slot
        let slot = Arc::new(Mutex::new(FrameSlot { seq: 0, key: false, data: Vec::new(), config: None, last_key: None }));
        let killed = Arc::new(AtomicBool::new(false));
        spawn_reader(s.try_clone().unwrap(), first_chunk, slot.clone(), killed.clone());

        let mut last_seq = 0u64;
        let start = std::time::Instant::now();
        let mut seen = 0u64;
        while std::time::Instant::now() - start < Duration::from_secs(10) && seen < 8 {
            thread::sleep(Duration::from_millis(200));
            let f = slot.lock().unwrap();
            if f.seq > last_seq {
                last_seq = f.seq;
                seen += 1;
                let cfg = f.config.as_ref().map(|c| hex_prefix(&c[..c.len().min(16)]));
                println!("[test] frame seq={} key={} bytes={} config={:?} head={}", f.seq, f.key, f.data.len(), cfg, hex_prefix(&f.data[..f.data.len().min(24)]));
                if f.seq == 1 && !f.data.is_empty() {
                    let d = &f.data;
                    let mut off = 0usize;
                    let mut nals = 0usize;
                    while off + 4 <= d.len() {
                        let len = u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]]) as usize;
                        if len == 0 || off + 4 + len > d.len() { break; }
                        nals += 1;
                        println!("[test]   avc NAL#{nals} len={len} type={} head={}", d[off + 4] & 0x1F, hex_prefix(&d[off + 4..(off + 4 + len.min(10))]));
                        off += 4 + len;
                    }
                    println!("[test]   frame1 parsed {nals} NALs / {off} bytes of {}", d.len());
                }
            }
        }
        println!("[test] final seq={last_seq} (expect > 0)");
        let _ = child.kill();
        let _ = run(&["forward", "--remove", &format!("tcp:{port}")]);
        killed.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(last_seq > 0, "NO FRAMES EMITTED - parser never produced a frame");
    }
}
