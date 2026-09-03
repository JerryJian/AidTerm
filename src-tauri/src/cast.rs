//! Screen casting via the scrcpy-server standalone mode.
//!
//! This is a faithful translation of the official scrcpy v4.1 wire protocol
//! (server -> client video socket), not a hand-rolled NAL parser. The video
//! socket carries a 1-byte dummy byte (send_dummy_byte), then the 64-byte
//! device name (send_device_meta), then a 4-byte codec id (send_stream_meta,
//! e.g. 0x68323634 "h264"), then a 12-byte session header (MSB set, then
//! width/height), then a loop of packets each carrying a 12-byte header (8-byte
//! PTS/flags + 4-byte packet size) followed by the raw MediaCodec output,
//! which is already in avc format (4-byte length-prefixed NALs).
//!
//! Config packets (FLAG_CONFIG, PTS = AV_NOPTS_VALUE) are stored and prepended
//! to the next media packet, exactly like scrcpy's packet_merger.c; the config
//! packet also becomes the avcC `description` handed to WebCodecs (recorder.c
//! uses it as extradata). Packets with the SESSION flag (byte0 MSB) carry a new
//! session header (orientation/size change) and are otherwise ignored.
//!
//! The stream stays in avc format end to end, so the frontend feeds the chunks
//! straight into a WebCodecs VideoDecoder configured with the avcC description.
//! Touch/keyboard input is injected back with `adb shell input`.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use tauri::{AppHandle, Manager};

/// Must match the scrcpy-server jar version (the server refuses mismatches).
pub const SCRCPY_VERSION: &str = "4.1";
const REMOTE_JAR: &str = "/data/local/tmp/scrcpy-server.jar";

// One-shot diagnostic flags (reset on each `start`) so the "first ..." logs do
// not spam on a live stream.
static LOG_DEVICE: AtomicBool = AtomicBool::new(false);
static LOG_SESSION: AtomicBool = AtomicBool::new(false);
static LOG_CONFIG: AtomicBool = AtomicBool::new(false);

// Official packet protocol constants (scrcpy v4.1, app/demuxer.c + Streamer.java).
const PACKET_HEADER_SIZE: usize = 12;
const PACKET_FLAG_SESSION: u64 = 1 << 63;
const PACKET_FLAG_CONFIG: u64 = 1 << 62;
const PACKET_FLAG_KEY_FRAME: u64 = 1 << 61;
const PACKET_PTS_MASK: u64 = PACKET_FLAG_KEY_FRAME - 1;
const DEVICE_NAME_FIELD_LENGTH: usize = 64;
const CODEC_ID_H264: u32 = 0x68323634; // "h264" in ASCII

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
    stream_ended: Arc<AtomicBool>,
}

#[derive(Clone)]
struct Frame {
    seq: u64,
    key: bool,
    data: Vec<u8>,
    config: Option<Vec<u8>>,
}

#[derive(Clone)]
struct FrameSlot {
    /// Latest emitted seq (diagnostics + fallback).
    seq: u64,
    key: bool,
    config: Option<Vec<u8>>,
    /// Bounded FIFO of frames awaiting consumption by the frontend. The
    /// frontend pops in decode order; a `need_key` poll drains non-key frames
    /// until the next keyframe (they cannot decode without a reference chain).
    buf: VecDeque<Frame>,
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
    let (bin, port) = crate::adb::adb_path(app)?;
    let mut cmd = Command::new(bin);
    cmd.arg("-P").arg(port);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    Ok(cmd)
}

#[cfg(not(target_os = "windows"))]
fn adb_cmd(app: &AppHandle) -> Result<Command, String> {
    let (bin, port) = crate::adb::adb_path(app)?;
    let mut cmd = Command::new(bin);
    cmd.arg("-P").arg(port);
    Ok(cmd)
}

/// Detect Annex-B byte stream format: NAL units separated by 00 00 01 (or
/// 00 00 00 01) start codes, as produced by some C2 encoders (e.g. Qualcomm
/// `c2.qti.avc.encoder` on HUAWEI NCO-AL00). avc format packets start with a
/// 4-byte big-endian NAL length instead, which cannot equal 00 00 01/00 00 00 01.
fn is_annexb(data: &[u8]) -> bool {
    data.len() >= 4
        && data[0] == 0
        && data[1] == 0
        && (data[2] == 1 || (data[2] == 0 && data[3] == 1))
}

/// Convert an Annex-B byte stream (00 00 00 01 / 00 00 01 start codes) into
/// avc format (4-byte big-endian length-prefixed NALs). Bytes before the first
/// start code are dropped. A NAL runs from its start code to the next start
/// code or to the end of the buffer; trailing zero bytes of the final NAL are
/// trimmed (a complete NAL can never end in 0x00 — its rbsp_trailing_bits end
/// with a 1). Returns None if no NAL is found.
fn annexb_to_avc(data: &[u8]) -> Option<Vec<u8>> {
    const PREFIX_4: &[u8] = &[0x00, 0x00, 0x00, 0x01];
    const PREFIX_3: &[u8] = &[0x00, 0x00, 0x01];
    let is_start = |i: usize| data[i..i + 3] == *PREFIX_3 || data[i..i + 4] == *PREFIX_4;
    let mut i = 0usize;
    // skip bytes until the first start code
    while i + 3 < data.len() && !is_start(i) {
        i += 1;
    }
    let mut out = Vec::with_capacity(data.len() / 2);
    while i + 3 < data.len() {
        // consume the start code (3 or 4 bytes)
        if data[i..i + 4] == *PREFIX_4 {
            i += 4;
        } else {
            i += 3;
        }
        let start = i;
        // scan to the next start code or EOF
        let mut end = data.len();
        while i + 3 < data.len() {
            if is_start(i) {
                end = i;
                break;
            }
            i += 1;
        }
        let mut nal = &data[start..end];
        if end == data.len() {
            // final NAL runs to EOF: trim trailing zero padding
            while nal.last() == Some(&0x00) {
                nal = &nal[..nal.len() - 1];
            }
        }
        if nal.is_empty() {
            i = end;
            continue;
        }
        out.extend_from_slice(&(nal.len() as u32).to_be_bytes());
        out.extend_from_slice(nal);
        i = end;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Return the packet payload in avc format: Annex-B input is converted to
/// length-prefixed NALs, avc input is returned unchanged. The frontend feeds
/// WebCodecs (with the avcC description), which expects consistent avc chunks.
fn normalize_avc(data: &[u8]) -> Vec<u8> {
    if is_annexb(data) {
        annexb_to_avc(data).unwrap_or_else(|| data.to_vec())
    } else {
        data.to_vec()
    }
}

/// Build an avcC record from raw SPS/PPS payloads (without NAL header bytes).
///
/// The avcC SPS/PPS NAL units MUST include their 1-byte NAL header (0x67/0x68).
/// Chromium's WebCodecs path (description present) converts every avc chunk
/// back to Annex-B via `H264ToAnnexBBitstreamConverter`, which writes the
/// avcC parameter sets verbatim after a start code and then parses their first
/// byte as the NAL type. Without the header, an injected SPS reads as a
/// non-IDR VCL slice (its profile_idc byte, e.g. 0x64, is type 4) and
/// `AnalyzeAnnexB` marks the chunk as "not a key frame", producing
/// `An EncodedVideoChunk was marked as type 'key' but wasn't a key frame`.
fn build_avcc(sps: &[u8], pps: &[u8]) -> Option<Vec<u8>> {
    if sps.len() < 4 {
        return None;
    }
    let mut out = Vec::with_capacity(sps.len() + pps.len() + 13);
    out.push(0x01); // configurationVersion
    out.push(sps[0]); // profile_idc
    out.push(sps[1]); // profile_compatibility
    out.push(sps[2]); // level_idc
    out.push(0xFF); // reserved(6) + lengthSizeMinusOne(3)
    out.push(0xE1); // reserved(3) + numOfSPS(5)
    out.extend_from_slice(&((sps.len() + 1) as u16).to_be_bytes());
    out.push(0x67); // NAL header for SPS
    out.extend_from_slice(sps);
    out.push(0x01); // numOfPPS
    out.extend_from_slice(&((pps.len() + 1) as u16).to_be_bytes());
    out.push(0x68); // NAL header for PPS
    out.extend_from_slice(pps);
    Some(out)
}

/// Build the avcC `description` from a MediaCodec CODEC_CONFIG packet. Some
/// encoders emit Annex-B start codes (00 00 00 01) instead of avc length
/// prefixes, so detect and normalize that first. This mirrors recorder.c,
/// which uses the first config packet as extradata.
fn build_avcc_from_config(config: &[u8]) -> Option<Vec<u8>> {
    let config = if is_annexb(config) {
        annexb_to_avc(config)?
    } else {
        config.to_vec()
    };
    let mut off = 0usize;
    let mut sps: Option<&[u8]> = None;
    let mut pps: Option<&[u8]> = None;
    while off + 4 <= config.len() {
        let len = u32::from_be_bytes(config[off..off + 4].try_into().ok()?) as usize;
        if len == 0 || off + 4 + len > config.len() {
            break;
        }
        let nal = &config[off + 4..off + 4 + len];
        match nal.first().map(|b| b & 0x1F) {
            Some(7) if sps.is_none() => sps = Some(&nal[1..]),
            Some(8) if pps.is_none() => pps = Some(&nal[1..]),
            _ => {}
        }
        off += 4 + len;
    }
    build_avcc(sps?, pps?)
}

/// Buffered byte stream over the video socket, equivalent to `net_recv_all`.
struct Demuxer {
    buf: Vec<u8>,
    pos: usize,
}

impl Demuxer {
    fn new(initial: Vec<u8>) -> Self {
        Demuxer { buf: initial, pos: 0 }
    }

    fn feed(&mut self, data: &[u8]) {
        // Compact before appending so a large consumed prefix cannot leak.
        if self.pos > 1 << 20 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(data);
    }

    fn avail(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Non-destructive look at the next `n` bytes (they must be available).
    fn peek(&self, n: usize) -> &[u8] {
        &self.buf[self.pos..self.pos + n]
    }

    fn take(&mut self, n: usize) -> &[u8] {
        let end = self.pos + n;
        debug_assert!(end <= self.buf.len());
        let s = &self.buf[self.pos..end];
        self.pos = end;
        s
    }
}

enum Phase {
    /// 1-byte dummy byte written by the server on accept (send_dummy_byte).
    DummyByte,
    DeviceMeta,
    CodecId,
    Session,
    Packets,
}

struct StreamState {
    phase: Phase,
    /// Latest avcC built from the most recent CODEC_CONFIG packet (or from
    /// inline SPS/PPS in a keyframe, as a fallback for encoders that don't
    /// send a separate config packet). Handed to WebCodecs as `description`.
    config: Option<Vec<u8>>,
    /// Diagnostics for the "session update but no media" restart loop: the
    /// server writes a session header on every encoder start, so repeated ones
    /// mean the device's capture/encoder is being reset (DisplayMonitor reset
    /// or encode-error retry) and never delivers a frame.
    session_updates: u64,
    last_session_at: Option<Instant>,
}

impl Default for StreamState {
    fn default() -> Self {
        StreamState {
            phase: Phase::DummyByte,
            config: None,
            session_updates: 0,
            last_session_at: None,
        }
    }
}

enum DemuxStatus {
    /// Not enough bytes yet; call again after more data is fed.
    NeedData,
    /// Stream explicitly disabled by the device (codec id 0).
    Disabled,
    /// Unrecoverable protocol error.
    Error,
}

/// Consume as many complete protocol elements as `demux` currently holds.
/// Mirrors demuxer.c `run_demuxer()` + `packet_merger.c`.
fn demux(demux: &mut Demuxer, st: &mut StreamState, slot: &Mutex<FrameSlot>) -> DemuxStatus {
    loop {
        match st.phase {
            Phase::DummyByte => {
                if demux.avail() < 1 {
                    return DemuxStatus::NeedData;
                }
                demux.take(1);
                st.phase = Phase::DeviceMeta;
            }
            Phase::DeviceMeta => {
                if demux.avail() < DEVICE_NAME_FIELD_LENGTH {
                    return DemuxStatus::NeedData;
                }
                let meta = demux.take(DEVICE_NAME_FIELD_LENGTH);
                let name: String = meta
                    .iter()
                    .take_while(|&&b| b != 0)
                    .map(|&b| b as char)
                    .collect();
                if !LOG_DEVICE.swap(true, Ordering::Relaxed) {
                    log::info!("[cast] device: {name}");
                }
                st.phase = Phase::CodecId;
            }
            Phase::CodecId => {
                if demux.avail() < 4 {
                    return DemuxStatus::NeedData;
                }
                let id = u32::from_be_bytes(demux.take(4).try_into().unwrap());
                if id == 0 {
                    log::warn!("[cast] stream explicitly disabled by the device");
                    return DemuxStatus::Disabled;
                }
                if id == 1 {
                    log::error!("[cast] stream configuration error on the device");
                    return DemuxStatus::Error;
                }
                if id != CODEC_ID_H264 {
                    log::error!("[cast] unsupported codec id 0x{id:08x} (only h264 is wired up)");
                    return DemuxStatus::Error;
                }
                st.phase = Phase::Session;
            }
            Phase::Session => {
                if demux.avail() < PACKET_HEADER_SIZE {
                    return DemuxStatus::NeedData;
                }
                let h = demux.take(PACKET_HEADER_SIZE);
                if h[0] & 0x80 == 0 {
                    log::error!("[cast] unexpected packet (not a session header)");
                    return DemuxStatus::Error;
                }
                let width = u32::from_be_bytes(h[4..8].try_into().unwrap());
                let height = u32::from_be_bytes(h[8..12].try_into().unwrap());
                if width == 0 || height == 0 {
                    log::error!("[cast] invalid session video size: {width}x{height}");
                    return DemuxStatus::Error;
                }
                if !LOG_SESSION.swap(true, Ordering::Relaxed) {
                    log::info!("[cast] session {width}x{height}");
                }
                st.phase = Phase::Packets;
            }
            Phase::Packets => {
                if demux.avail() < PACKET_HEADER_SIZE {
                    return DemuxStatus::NeedData;
                }
                // Peek (not consume) the header first: a media/config packet is
                // only fully readable when header + payload are both buffered.
                // Consuming the header on a partial payload would leave `pos`
                // inside the payload on resume and misparse every following
                // packet as a fake "session update".
                let mut hbuf = [0u8; PACKET_HEADER_SIZE];
                hbuf.copy_from_slice(demux.peek(PACKET_HEADER_SIZE));
                let h = &hbuf[..];
                let pts_flags = u64::from_be_bytes(h[..8].try_into().unwrap());
                let len = u32::from_be_bytes(h[8..12].try_into().unwrap());
                if pts_flags & PACKET_FLAG_SESSION != 0 {
                    // Session packet (orientation/size change): 12-byte header
                    // only, no payload. Mirrors scrcpy demuxer.c which just
                    // parses the session and notifies sinks.
                    demux.take(PACKET_HEADER_SIZE);
                    let width = u32::from_be_bytes(h[4..8].try_into().unwrap());
                    let height = u32::from_be_bytes(h[8..12].try_into().unwrap());
                    let now = Instant::now();
                    let gap = st
                        .last_session_at
                        .map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
                    st.last_session_at = Some(now);
                    st.session_updates += 1;
                    if st.session_updates <= 5 || st.session_updates.is_multiple_of(25) {
                        log::info!(
                            "[cast] session update {width}x{height} (#{}) gap {:.0}ms",
                            st.session_updates,
                            gap.as_millis()
                        );
                    }
                    if st.session_updates == 25 {
                        log::warn!(
                            "[cast] 25 session updates with no media frame yet — capture/encoder reset loop on the device; capture `adb logcat -s scrcpy` during casting for the server-side error"
                        );
                    }
                    continue;
                }
                if len == 0 {
                    log::error!("[cast] invalid packet length: 0");
                    return DemuxStatus::Error;
                }
                // Atomic consume: do not advance past the header until the whole
                // packet (header + payload) is buffered.
                if demux.avail() < PACKET_HEADER_SIZE + len as usize {
                    return DemuxStatus::NeedData;
                }
                demux.take(PACKET_HEADER_SIZE);
                let raw = demux.take(len as usize);
                if pts_flags & PACKET_FLAG_CONFIG != 0 {
                    // Config packet (SPS/PPS, pts == AV_NOPTS_VALUE).
                    // scrcpy merges it into the next media packet (packet_merger.c)
                    // because FFmpeg expects SPS/PPS inline in the bitstream.
                    // WebCodecs is different: it wants the avcC as `description`
                    // (passed to configure()) and the chunk data to be the raw
                    // frame. So we just build the avcC and store it — no merge.
                    if let Some(avcc) = build_avcc_from_config(raw) {
                        if !LOG_CONFIG.swap(true, Ordering::Relaxed) {
                            log::info!("[cast] avcC built from config packet: {}", hex_prefix(&avcc));
                        }
                        st.config = Some(avcc);
                    } else {
                        log::warn!("[cast] config packet did not yield avcC (no SPS/PPS found)");
                    }
                    continue;
                }
                // Media packet. The frame data is the raw MediaCodec output
                // (avc or Annex-B, normalized to avc). Unlike scrcpy we do NOT
                // prepend the config packet — WebCodecs gets it as `description`.
                let key = pts_flags & PACKET_FLAG_KEY_FRAME != 0;
                let pts = pts_flags & PACKET_PTS_MASK;
                let data = normalize_avc(raw);
                // Fallback: some encoders never send a separate CODEC_CONFIG
                // packet and inline SPS/PPS in every IDR. Without an avcC the
                // frontend cannot configure the decoder, so extract one from
                // the keyframe's inline parameter sets.
                let config = st.config.clone().or_else(|| {
                    if key {
                        if let Some(avcc) = build_avcc_from_config(raw) {
                            log::info!("[cast] avcC built from inline keyframe SPS/PPS: {}", hex_prefix(&avcc));
                            st.config = Some(avcc.clone());
                            return Some(avcc);
                        }
                    }
                    None
                });
                emit(&data, key, pts, config, slot);
            }
        }
    }
}

fn emit(frame: &[u8], key: bool, _pts: u64, config: Option<Vec<u8>>, slot: &Mutex<FrameSlot>) {
    let mut s = slot.lock().unwrap();
    s.seq += 1;
    let seq = s.seq;
    s.key = key;
    s.config = config.clone();
    // Push to the bounded FIFO. The frontend pops in decode order.
    s.buf.push_back(Frame { seq, key, data: frame.to_vec(), config });
    // Cap the backlog (~10s at 30fps). If the frontend falls this far behind,
    // drop the oldest frames — stale deltas can't decode without their
    // reference chain anyway.
    while s.buf.len() > 300 {
        s.buf.pop_front();
    }
}

fn spawn_reader(
    stream: TcpStream,
    initial: Vec<u8>,
    slot: Arc<Mutex<FrameSlot>>,
    killed: Arc<AtomicBool>,
    stream_ended: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        // `initial` is the chunk already consumed by the connect probe. It
        // starts with the dummy byte, then the device meta + codec id + session
        // header + first packets. If the probe timed out instead
        // (committed_on_timeout), `initial` is empty and the same bytes arrive
        // from the socket; both cases start at the DummyByte phase.
        let mut reader = match stream.try_clone() {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut demuxer = Demuxer::new(initial);
        let mut state = StreamState::default();
        let mut tmp = [0u8; 32768];
        let mut reads: u64 = 0;
        let mut bytes: u64 = 0;
        let mut errors: u64 = 0;
        let mut timeouts: u64 = 0;
        let mut ended_logged = false;
        loop {
            if killed.load(Ordering::Relaxed) {
                break;
            }
            // Drain everything currently buffered before blocking on the socket
            // (a single `demux` call consumes every complete protocol element).
            match demux(&mut demuxer, &mut state, &slot) {
                DemuxStatus::NeedData => {}
                DemuxStatus::Disabled => {
                    log::warn!("[cast] stream disabled by device");
                    stream_ended.store(true, Ordering::Relaxed);
                    return;
                }
                DemuxStatus::Error => {
                    log::error!("[cast] demux error, stopping reader");
                    stream_ended.store(true, Ordering::Relaxed);
                    return;
                }
            }
            if killed.load(Ordering::Relaxed) {
                break;
            }
            let _ = reader.set_read_timeout(Some(Duration::from_millis(1000)));
            match reader.read(&mut tmp) {
                Ok(0) => {
                    log::info!("[cast] reader EOF after {reads} reads / {bytes} bytes");
                    stream_ended.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(n) => {
                    reads += 1;
                    bytes += n as u64;
                    timeouts = 0;
                    demuxer.feed(&tmp[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {
                    timeouts += 1;
                    // Log periodic timeout warnings (every 5th timeout = 5s)
                    if timeouts == 5 || (timeouts > 5 && timeouts.is_multiple_of(30)) {
                        log::info!(
                            "[cast] reader timeout #{timeouts}: no data for {:.0}s (reads {reads} / {bytes} bytes)",
                            timeouts as f64
                        );
                    }
                    // After 120 consecutive timeouts (~2 minutes) with no data,
                    // the stream is truly dead — mark it ended. Short bursts of
                    // timeouts are normal (screen static, encoder pauses).
                    if timeouts >= 120 && !ended_logged {
                        log::warn!(
                            "[cast] reader timed out after {timeouts} consecutive timeouts (~2 min), marking stream ended"
                        );
                        stream_ended.store(true, Ordering::Relaxed);
                        ended_logged = true;
                    }
                }
                Err(e) => {
                    if killed.load(Ordering::Relaxed) {
                        break;
                    }
                    errors += 1;
                    if errors <= 5 || errors.is_multiple_of(25) {
                        log::info!(
                            "[cast] reader I/O error #{errors}: {e} (after {reads} reads / {bytes} bytes)"
                        );
                    }
                    // On persistent I/O errors, mark the stream as ended
                    // so the frontend can detect disconnection.
                    if errors >= 5 && !ended_logged {
                        log::warn!(
                            "[cast] reader stopped after {errors} I/O errors, marking stream ended"
                        );
                        stream_ended.store(true, Ordering::Relaxed);
                        ended_logged = true;
                    }
                }
            }
        }
        // Ensure stream_ended is set on any exit path
        stream_ended.store(true, Ordering::Relaxed);
    });
}

/// Start casting a device: push the jar, forward a port, launch the server.
pub fn start(app: &AppHandle, serial: &str, max_size: u32) -> Result<CastStartInfo, String> {
    let state = app.state::<CastState>();
    let mut sessions = state.sessions.lock().unwrap();
    {
        let s = sessions.get(serial).cloned();
        if let Some(s) = s {
            // If session is alive AND stream is still active, reuse it.
            // `stream_ended` is set when the reader thread exits, so if the
            // stream has ended we must reap and restart (even if `killed`
            // wasn't explicitly set by stop()).
            if !s.killed.load(Ordering::Relaxed) && !s.stream_ended.load(Ordering::Relaxed) {
                // Re-query the real screen size so the frontend's input
                // mapping stays correct after a reconnect.
                let (w, h) = screen_size(app, serial).unzip();
                return Ok(CastStartInfo { port: s.local_port, width: w, height: h });
            }
        }
    }
    // Reap any leftover session (killed, or stream ended).
    sessions.remove(serial);

    // Query the real display resolution up front (cheap, one adb call) so the
    // frontend can map click positions to the device's actual coordinate space
    // even when max_size downscales the streamed video.
    let (real_w, real_h) = screen_size(app, serial).unzip();

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
    //    Server params mirror the official client's for this option set
    //    (video-only, tunnel forward). All the meta options stay at their
    //    defaults (send_dummy_byte/send_device_meta/send_stream_meta/
    //    send_frame_meta = true) so the stream is a proper packet stream.
    let mut shell = adb_cmd(app)?;
    let mut args: Vec<String> = vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        format!("CLASSPATH={}", REMOTE_JAR),
        "app_process".into(),
        "/".into(),
        "com.genymobile.scrcpy.Server".into(),
        SCRCPY_VERSION.into(),
        format!("scid={scid:08x}"),
        "tunnel_forward=true".into(),
        "audio=false".into(),
        "control=false".into(),
        "cleanup=false".into(),
        "max_fps=30".into(),
    ];
    if max_size > 0 {
        args.push(format!("max_size={max_size}"));
    }
    let mut child = shell
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start scrcpy server failed: {}", e))?;
    log::info!("[cast] scrcpy server launched: scid {scid:08x} max_size {max_size}");
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
    //    (control/audio are disabled), and `send_dummy_byte` (default true)
    //    makes it write a single 0x00 byte right after accepting. If our read
    //    times out (500ms) the connection is still alive — the device-side adb
    //    handshake is just slow — and the server may ALREADY have accepted it.
    //    Dropping such a connection kills the stream (`Screen streaming
    //    stopped`) and the server exits, so every later probe EOFs. Therefore:
    //    only clean EOF (Ok(0), socket not created yet) triggers a retry; a
    //    read timeout commits the connection and the reader waits for bytes.
    let mut stream: Option<TcpStream> = None;
    let mut first_chunk: Vec<u8> = Vec::new();
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
                if s.set_read_timeout(Some(Duration::from_millis(500))).is_err() {
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
    if !first_chunk.is_empty() {
        log::info!("[cast] connected to port {local_port} scid {scid:08x} (live stream)");
    } else {
        log::info!("[cast] connected to port {local_port} scid {scid:08x} (committed on timeout, waiting for stream)");
    }

    let reader_stream = stream.try_clone().map_err(|e| e.to_string())?;
    let stream_ended = Arc::new(AtomicBool::new(false));
    let session = Arc::new(CastSession {
        stream: Mutex::new(Some(stream)),
        child: Mutex::new(Some(child)),
        local_port,
        frame: Arc::new(Mutex::new(FrameSlot { seq: 0, key: false, config: None, buf: VecDeque::new() })),
        killed: Arc::new(AtomicBool::new(false)),
        stream_ended: stream_ended.clone(),
    });
    let slot = session.frame.clone();
    let killed = session.killed.clone();
    LOG_DEVICE.store(false, Ordering::Relaxed);
    LOG_SESSION.store(false, Ordering::Relaxed);
    LOG_CONFIG.store(false, Ordering::Relaxed);
    spawn_reader(reader_stream, first_chunk, slot, killed, stream_ended);
    sessions.insert(serial.to_string(), session);
    Ok(CastStartInfo { port: local_port, width: real_w, height: real_h })
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

/// Pop the next frame for the frontend, in decode order.
///
/// * Normal poll: pop the oldest frame from the FIFO. This matches scrcpy's
///   demuxer→decoder pipeline: frames are consumed in order, never replayed.
/// * `need_key` (decoder lost): pop and DISCARD every non-key frame until the
///   next keyframe — deltas without their reference chain are undecodable, so
///   there is no point keeping them. This is the WebCodecs equivalent of
///   scrcpy waiting for the next IDR after a decoder reset.
fn serve_next(f: &mut FrameSlot, need_key: bool) -> Option<Frame> {
    if need_key {
        while let Some(front) = f.buf.front() {
            if front.key {
                return f.buf.pop_front();
            }
            f.buf.pop_front();
        }
        return None;
    }
    f.buf.pop_front()
}

pub fn frame(
    app: &AppHandle,
    serial: &str,
    need_key: Option<bool>,
    seen_seq: Option<u64>,
) -> Option<(u64, bool, String, Option<String>)> {
    let _ = seen_seq; // pop semantics: no GC by seen_seq needed
    let state = app.state::<CastState>();
    let sessions = state.sessions.lock().unwrap();
    let s = sessions.get(serial)?;
    let mut f = s.frame.lock().unwrap();
    let stream_ended = s.stream_ended.load(Ordering::Relaxed);
    if let Some(frame) = serve_next(&mut f, need_key.unwrap_or(false)) {
        return Some((
            frame.seq,
            frame.key,
            B64.encode(&frame.data),
            frame.config.as_ref().map(|c| B64.encode(c)),
        ));
    }
    // No frame available right now. If the stream ended, signal disconnect.
    // Otherwise return seq=0 so the frontend knows "no new frame, keep polling"
    // without mistaking it for a disconnect.
    if stream_ended {
        return None;
    }
    Some((0, false, String::new(), None))
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

/// Result of `cast_start`: the forwarded local port plus the device's real
/// display resolution (native orientation, usually portrait).
///
/// `adb shell input tap` operates in the device's REAL screen coordinate space,
/// but the streamed video is downscaled by scrcpy's `max_size`. The frontend
/// must therefore map click positions to the real resolution, not the video
/// resolution — otherwise taps land in the top-left quadrant of the screen.
/// `width`/`height` are `None` when `wm size` could not be parsed (the frontend
/// then falls back to the video size, which is only correct without scaling).
#[derive(serde::Serialize)]
pub struct CastStartInfo {
    pub port: u16,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Parse `adb shell wm size` output, e.g.:
///   Physical size: 1080x2400
///   Override size: 1440x3200
/// Prefers Override (the effective resolution the input subsystem uses) over
/// Physical. Returns the size in the device's native orientation.
fn parse_wm_size(text: &str) -> Option<(u32, u32)> {
    let mut physical: Option<(u32, u32)> = None;
    let mut override_sz: Option<(u32, u32)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Physical size:") {
            physical = parse_size_pair(rest);
        } else if let Some(rest) = line.strip_prefix("Override size:") {
            override_sz = parse_size_pair(rest);
        }
    }
    override_sz.or(physical)
}

fn parse_size_pair(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once('x')?;
    let w: u32 = w.trim().parse().ok()?;
    let h: u32 = h.trim().parse().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

/// Query the device's real display resolution via `adb shell wm size`.
fn screen_size(app: &AppHandle, serial: &str) -> Option<(u32, u32)> {
    let out = adb_cmd(app)
        .ok()?
        .args(["-s", serial, "shell", "wm", "size"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_wm_size(&text)
}

fn pick_local_port() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    20000 + (t.subsec_nanos() % 20000) as u16
}

fn rand_31bit() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    ((t.as_nanos() % 0x7FFF_FFFF) as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The avcC SPS/PPS NAL units must carry their 1-byte NAL header (0x67/0x68).
    /// Chromium's `H264ToAnnexBBitstreamConverter` re-inserts these param sets
    /// verbatim after a start code and parses the first byte as the NAL type;
    /// without the header an injected SPS reads as NAL type 4 (a non-IDR VCL),
    /// so the keyframe check `AnalyzeAnnexB` rejects the chunk.
    #[test]
    fn build_avcc_includes_nal_type_bytes() {
        let sps: Vec<u8> = vec![0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0];
        let pps: Vec<u8> = vec![0xEB, 0x01, 0x10];
        let cfg = build_avcc(&sps, &pps).expect("build_avcc should succeed");
        assert_eq!(cfg[0], 0x01); // configurationVersion
        assert_eq!(&cfg[1..4], &[0x64, 0x00, 0x1F]); // profile/compat/level
        assert_eq!(cfg[4], 0xFF); // lengthSizeMinusOne = 3
        assert_eq!(cfg[5], 0xE1); // numOfSPS = 1
        let sps_len = u16::from_be_bytes([cfg[6], cfg[7]]) as usize;
        assert_eq!(sps_len, sps.len() + 1, "SPS entry length includes NAL header");
        assert_eq!(cfg[8], 0x67, "SPS entry must start with the 0x67 NAL header");
        assert_eq!(&cfg[9..9 + sps.len()], &sps[..]);
        let o = 9 + sps.len();
        assert_eq!(cfg[o], 0x01); // numOfPPS = 1
        let pps_len = u16::from_be_bytes([cfg[o + 1], cfg[o + 2]]) as usize;
        assert_eq!(pps_len, pps.len() + 1, "PPS entry length includes NAL header");
        assert_eq!(cfg[o + 3], 0x68, "PPS entry must start with the 0x68 NAL header");
        assert_eq!(&cfg[o + 4..o + 4 + pps.len()], &pps[..]);
    }

    /// A MediaCodec CODEC_CONFIG packet is avc format (length-prefixed NALs
    /// with their own 0x67/0x68 headers). The resulting avcC must equal the
    /// hand-built one, with the NAL headers included.
    #[test]
    fn build_avcc_from_config_matches_handbuilt() {
        let sps: Vec<u8> = vec![0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0];
        let pps: Vec<u8> = vec![0xEB, 0x01, 0x10];
        let mut config: Vec<u8> = Vec::new();
        for (header, body) in [(0x67u8, &sps[..]), (0x68, &pps[..])] {
            let mut nal = Vec::with_capacity(1 + body.len());
            nal.push(header);
            nal.extend_from_slice(body);
            config.extend_from_slice(&(nal.len() as u32).to_be_bytes());
            config.extend_from_slice(&nal);
        }
        let avcc = build_avcc_from_config(&config).expect("build_avcc_from_config should succeed");
        let expected = build_avcc(&sps, &pps).expect("build_avcc should succeed");
        assert_eq!(avcc, expected);
    }

    /// NAL-prefix helpers for building a MediaCodec-style avc packet.
    fn nal_avc(buf: &mut Vec<u8>, nal: &[u8]) {
        buf.extend_from_slice(&(nal.len() as u32).to_be_bytes());
        buf.extend_from_slice(nal);
    }

    /// Build a full protocol stream and feed it through `demux`.
    /// Returns the resulting FrameSlot.
    fn run_demux(stream: &[u8]) -> FrameSlot {
        let slot = Arc::new(Mutex::new(FrameSlot {
            seq: 0,
            key: false,
            config: None,
            buf: VecDeque::new(),
        }));
        let mut demuxer = Demuxer::new(stream.to_vec());
        let mut state = StreamState::default();
        // Feed byte-by-byte (worse than any real socket fragmentation) to prove
        // the parser only consumes complete protocol elements.
        for &b in stream {
            demuxer.feed(&[b]);
            let _ = demux(&mut demuxer, &mut state, &slot);
        }
        // Drain whatever the last byte completed.
        let _ = demux(&mut demuxer, &mut state, &slot);
        let guard = slot.lock().unwrap();
        let out = guard.clone();
        drop(guard);
        out
    }

    fn make_stream(with_media: bool) -> Vec<u8> {
        let mut out = Vec::new();
        // dummy byte (already consumed by the probe in real flow)
        out.push(0x00);
        // device meta: 64 bytes, name "test" + zeros
        out.extend_from_slice(b"test");
        out.resize(1 + 64, 0);
        // codec id h264
        out.extend_from_slice(&CODEC_ID_H264.to_be_bytes());
        // session header: 0x80 00 00 00 | width 1920 | height 1080
        out.extend_from_slice(&[0x80, 0x00, 0x00, 0x00]);
        out.extend_from_slice(&1920u32.to_be_bytes());
        out.extend_from_slice(&1080u32.to_be_bytes());
        if with_media {
            // config packet: SPS + PPS
            let mut config = Vec::new();
            nal_avc(&mut config, &[0x67, 0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0]);
            nal_avc(&mut config, &[0x68, 0xEB, 0x01, 0x10]);
            out.extend_from_slice(&PACKET_FLAG_CONFIG.to_be_bytes());
            out.extend_from_slice(&(config.len() as u32).to_be_bytes());
            out.extend_from_slice(&config);
            // media keyframe packet: single IDR slice (merged config = SPS+PPS+IDR)
            let mut media = Vec::new();
            nal_avc(&mut media, &[0x65, 0x88, 0x84, 0x00]);
            out.extend_from_slice(&(PACKET_FLAG_KEY_FRAME | 12345).to_be_bytes());
            out.extend_from_slice(&(media.len() as u32).to_be_bytes());
            out.extend_from_slice(&media);
        }
        out
    }

    #[test]
    fn demux_parses_meta_codec_session_and_builds_avcc() {
        let stream = make_stream(true);
        let slot = run_demux(&stream);
        assert_eq!(slot.seq, 1, "exactly one media frame emitted");
        assert!(slot.key, "first media packet carries FLAG_KEY_FRAME");
        assert!(slot.config.is_some(), "avcC built from the config packet");
        // WebCodecs gets avcC as `description` and the chunk data is the raw
        // frame (NOT config-merged, unlike scrcpy/FFmpeg). So the frame data
        // is just the IDR slice, without SPS/PPS.
        let frame = &slot.buf.front().expect("one frame buffered").data;
        let expected_idr = {
            let mut c = Vec::new();
            nal_avc(&mut c, &[0x65, 0x88, 0x84, 0x00]);
            c
        };
        assert_eq!(*frame, expected_idr, "frame data is the raw media packet (no config prefix)");
        // the single NAL in the frame is the IDR slice (type 5)
        let off = 0usize;
        assert_eq!(frame[off + 4] & 0x1F, 5, "first (and only) NAL is an IDR slice");
    }

    #[test]
    fn demux_delta_frame_is_raw_media() {
        let mut stream = make_stream(true);
        // a second media packet (delta) — the frame data must be just the raw
        // media bytes, no config prefix (config is only ever in `description`).
        let mut media = Vec::new();
        nal_avc(&mut media, &[0x41, 0x9A, 0x02]);
        stream.extend_from_slice(&(500u64).to_be_bytes());
        stream.extend_from_slice(&(media.len() as u32).to_be_bytes());
        stream.extend_from_slice(&media);

        let slot = run_demux(&stream);
        assert_eq!(slot.seq, 2);
        assert!(!slot.key, "second packet has no FLAG_KEY_FRAME");
        // delta frame data == the raw media packet
        let frame2 = &slot.buf.back().expect("two frames buffered").data;
        let mut expected = Vec::new();
        nal_avc(&mut expected, &[0x41, 0x9A, 0x02]);
        assert_eq!(*frame2, expected);
    }

    #[test]
    fn demux_ignores_session_updates_and_keeps_streaming() {
        let mut stream = make_stream(true);
        // interleave a session update (orientation change) between frames
        stream.extend_from_slice(&[0x80, 0x00, 0x00, 0x00]);
        stream.extend_from_slice(&1080u32.to_be_bytes());
        stream.extend_from_slice(&1920u32.to_be_bytes());
        let mut media = Vec::new();
        nal_avc(&mut media, &[0x41, 0x9A, 0x02]);
        stream.extend_from_slice(&(600u64).to_be_bytes());
        stream.extend_from_slice(&(media.len() as u32).to_be_bytes());
        stream.extend_from_slice(&media);

        let slot = run_demux(&stream);
        assert_eq!(slot.seq, 2, "session packet emits no frame and does not stop");
        assert!(!slot.key);
    }

    /// Regression for the live-cast wedge on Android 12 (HUAWEI NCO-AL00): a
    /// media payload (e.g. 38 KB IDR) arrives across many adb reads, so the
    /// 12-byte header is buffered long before the payload. The parser must NOT
    /// consume the header until header + payload are both available; otherwise
    /// it resumes mid-payload and reads media bytes as fake session packets.
    #[test]
    fn demux_partial_payload_resumes_without_misparsing() {
        // handshake only (no config/media yet), then a config packet and one
        // large media packet that straddles several feed() calls
        let mut stream = make_stream(false);
        let mut config = Vec::new();
        nal_avc(&mut config, &[0x67, 0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0]);
        nal_avc(&mut config, &[0x68, 0xEB, 0x01, 0x10]);
        stream.extend_from_slice(&PACKET_FLAG_CONFIG.to_be_bytes());
        stream.extend_from_slice(&(config.len() as u32).to_be_bytes());
        stream.extend_from_slice(&config);

        let mut media = Vec::with_capacity(4096);
        for i in 0..256u32 {
            let nal: Vec<u8> = (0..16u8).map(|_| i as u8).collect();
            nal_avc(&mut media, &nal);
        }
        stream.extend_from_slice(&(PACKET_FLAG_KEY_FRAME | 99).to_be_bytes());
        stream.extend_from_slice(&(media.len() as u32).to_be_bytes());
        stream.extend_from_slice(&media);

        let slot = Arc::new(Mutex::new(FrameSlot {
            seq: 0,
            key: false,
            config: None,
            buf: VecDeque::new(),
        }));
        let mut demuxer = Demuxer::new(Vec::new());
        let mut state = StreamState::default();
        // feed in realistic network-sized chunks (header and payload straddle
        // chunk boundaries) rather than byte-by-byte
        for chunk in stream.chunks(4096) {
            demuxer.feed(chunk);
            let _ = demux(&mut demuxer, &mut state, &slot);
        }
        let _ = demux(&mut demuxer, &mut state, &slot);

        let guard = slot.lock().unwrap();
        assert_eq!(guard.seq, 1, "the large media frame emits exactly once");
        assert!(guard.key, "large packet carries FLAG_KEY_FRAME");
        assert!(guard.config.is_some(), "avcC built from config packet");
        // frame data is the raw media payload (no config prefix)
        let frame = &guard.buf.front().expect("one frame").data;
        assert_eq!(*frame, media, "frame data is the raw media payload, intact");
        drop(guard);
    }

    /// Some C2 encoders (e.g. c2.qti.avc.encoder) emit Annex-B start codes
    /// (00 00 00 01) in both the CODEC_CONFIG and media packets. The demuxer
    /// must normalize each to avc length-prefixed NALs, which is what WebCodecs
    /// (with an avcC description) expects.
    #[test]
    fn demux_normalizes_annexb_to_avc() {
        let mut stream = make_stream(false);
        // Annex-B config: 00 00 00 01 SPS | 00 00 00 01 PPS
        let mut config_annexb = Vec::new();
        config_annexb.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        config_annexb.extend_from_slice(&[0x67, 0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0]);
        config_annexb.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        config_annexb.extend_from_slice(&[0x68, 0xEB, 0x01, 0x10]);
        stream.extend_from_slice(&PACKET_FLAG_CONFIG.to_be_bytes());
        stream.extend_from_slice(&(config_annexb.len() as u32).to_be_bytes());
        stream.extend_from_slice(&config_annexb);
        // Annex-B IDR media packet (final NAL, no trailing start code needed)
        let media_annexb = [0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0xA0];
        stream.extend_from_slice(&(PACKET_FLAG_KEY_FRAME | 7).to_be_bytes());
        stream.extend_from_slice(&(media_annexb.len() as u32).to_be_bytes());
        stream.extend_from_slice(&media_annexb);

        let slot = run_demux(&stream);
        assert_eq!(slot.seq, 1);
        assert!(slot.key);
        assert!(
            slot.config.is_some(),
            "avcC must be built from Annex-B config too"
        );
        // frame data is the raw media packet normalized to avc (IDR only, no
        // config prefix — config lives in `description`).
        let frame = &slot.buf.front().expect("one frame").data;
        let mut expected = Vec::new();
        nal_avc(&mut expected, &[0x65, 0x88, 0x84, 0xA0]);
        assert_eq!(*frame, expected, "Annex-B media frame converted to avc");
        // sanity: no Annex-B start codes remain
        assert!(
            !frame.windows(4).any(|w| w == [0x00, 0x00, 0x00, 0x01]),
            "no Annex-B start codes may remain"
        );
    }

    #[test]
    fn annexb_to_avc_converts_both_start_code_lengths() {
        // 4-byte and 3-byte start codes mixed, plus leading garbage and a
        // trailing zero-byte pad that must be dropped
        let input: &[u8] = &[
            0xFF, 0x00, // leading junk before the first start code
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0,
            0x00, 0x00, 0x01, 0x68, 0xEB, 0x01, 0x10,
            0x00, // trailing padding (must not be read as a NAL)
        ];
        let avc = annexb_to_avc(input).expect("conversion should succeed");
        let mut expected = Vec::new();
        nal_avc(&mut expected, &[0x67, 0x64, 0x00, 0x1F, 0xAC, 0xB2, 0x00, 0xA0]);
        nal_avc(&mut expected, &[0x68, 0xEB, 0x01, 0x10]);
        assert_eq!(avc, expected);
        // leading junk means the very first bytes are not a start code
        let clean: &[u8] = &input[2..];
        assert!(is_annexb(clean), "detector recognizes Annex-B");
        assert!(!is_annexb(&avc), "converted output is avc, not Annex-B");
    }

    #[test]
    fn demux_rejects_non_h264_codec() {
        let mut stream = make_stream(false);
        // overwrite the codec id with AV1
        let av1 = 0x00617631u32;
        stream[1 + 64..1 + 64 + 4].copy_from_slice(&av1.to_be_bytes());
        let slot = Arc::new(Mutex::new(FrameSlot {
            seq: 0,
            key: false,
            config: None,
            buf: VecDeque::new(),
        }));
        let mut demuxer = Demuxer::new(stream);
        let mut state = StreamState::default();
        assert!(matches!(
            demux(&mut demuxer, &mut state, &slot),
            DemuxStatus::Error
        ));
    }

    #[test]
    fn demux_missing_session_header_is_error() {
        let mut stream = make_stream(false);
        // corrupt the session header MSB
        stream[1 + 64 + 4] = 0x00;
        let slot = Arc::new(Mutex::new(FrameSlot {
            seq: 0,
            key: false,
            config: None,
            buf: VecDeque::new(),
        }));
        let mut demuxer = Demuxer::new(stream);
        let mut state = StreamState::default();
        assert!(matches!(
            demux(&mut demuxer, &mut state, &slot),
            DemuxStatus::Error
        ));
    }

    /// End-to-end probe against a live device using the REAL reader/demuxer
    /// code. Mirrors `start()`'s orchestration but with a local FrameSlot (no
    /// Tauri AppHandle needed), so a failure reproduces exactly what the app
    /// does. Marked `#[ignore]` because it needs a connected adb device.
    #[test]
    #[ignore]
    fn e2e_live_stream_parses_and_emits_frames() {
        let _ = env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .try_init();
        let adb = std::env::var("AIDTERM_ADB").unwrap_or_else(|_| "../bin/adb.exe".to_string());
        if !std::path::Path::new(&adb).is_file() {
            panic!("adb not found at {adb} (set AIDTERM_ADB)");
        }
        // All adb sources talk to the shared default 5037 server.
        let port = crate::adb::ADB_PORT;
        let jar = std::env::var("AIDTERM_SCRCPY")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("../bin/scrcpy-server.jar"));
        assert!(jar.is_file(), "jar missing: {}", jar.display());

        let out = Command::new(&adb).args(["-P", port, "devices"]).output().expect("adb devices");
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
            let o = Command::new(&adb).args(["-P", port, "-s", &serial]).args(args).output().unwrap();
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        // cleanup stale
        let _ = run(&["forward", "--remove-all"]);
        // Clear device logcat so the post-run dump below only shows our run.
        // The server's own diagnostics go to logcat (tag "scrcpy") via
        // android.util.Log, NOT to the adb shell stdout/stderr pipes.
        let _ = run(&["logcat", "-c"]);

        // 1. push
        let push = run(&["push", jar.to_str().unwrap(), REMOTE_JAR]);
        println!("[test] push: {push}");
        // 2. forward
        let port = pick_local_port();
        let scid = rand_31bit();
        let la = format!("localabstract:scrcpy_{:08x}", scid);
        println!("[test] forward tcp:{port} -> {la}");
        run(&["forward", &format!("tcp:{port}"), &la]);
        // 3. launch server (same params as start(): no raw_stream, meta on)
        let mut child = Command::new(&adb)
            .arg("-P")
            .arg(port.to_string())
            .arg("-s")
            .arg(&serial)
            .arg("shell")
            .arg(format!("CLASSPATH={REMOTE_JAR}"))
            .args([
                "app_process", "/", "com.genymobile.scrcpy.Server", SCRCPY_VERSION,
                "tunnel_forward=true", "audio=false", "control=false", "cleanup=false",
                "max_size=1280", "max_fps=30",
                &format!("scid={scid:08x}"),
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
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                if s.set_read_timeout(Some(Duration::from_millis(500))).is_err() { drop(s); continue; }
                let mut r = s.try_clone().unwrap();
                match r.read(&mut probe) {
                    Ok(0) => { eofs += 1; drop(s); }
                    Ok(n) => { first_chunk = probe[..n].to_vec(); stream = Some(s); break; }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => { stream = Some(s); break; }
                    Err(_) => { drop(s); }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        let s = match stream {
            Some(s) => s,
            None => { println!("[test] NEVER LIVE attempts={attempts} eofs={eofs}"); let _ = child.kill(); let _ = child.wait(); let _ = run(&["forward", "--remove", &format!("tcp:{port}")]); panic!("no live stream"); }
        };
        println!("[test] LIVE attempts={attempts} eofs={eofs} first_chunk={} bytes: {}", first_chunk.len(), hex_prefix(&first_chunk));

        // 5. reader + slot
        let slot = Arc::new(Mutex::new(FrameSlot { seq: 0, key: false, config: None, buf: VecDeque::new() }));
        let killed = Arc::new(AtomicBool::new(false));
        let stream_ended = Arc::new(AtomicBool::new(false));
        spawn_reader(s.try_clone().unwrap(), first_chunk, slot.clone(), killed.clone(), stream_ended.clone());

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
                let head_data: &[u8] = f.buf.back().map(|fr| &fr.data[..]).unwrap_or(&[]);
                let data_len = head_data.len();
                println!("[test] frame seq={} key={} bytes={} config={:?} head={}", f.seq, f.key, data_len, cfg, hex_prefix(&head_data[..data_len.min(24)]));
                if f.seq == 1 && !head_data.is_empty() {
                    let d = head_data;
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
        let _ = child.wait();
        let _ = run(&["forward", "--remove", &format!("tcp:{port}")]);
        // Dump the server's logcat: the decisive evidence for a capture/encoder
        // reset loop ("Capture/encoding error: ...", "DisplayMonitor ...").
        let logcat = run(&["logcat", "-d", "-s", "scrcpy"]);
        println!("[test] scrcpy logcat:\n{}", logcat.trim());
        killed.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(last_seq > 0, "NO FRAMES EMITTED - parser never produced a frame");
    }

    /// Build a FrameSlot with a synthetic GOP: key at seq 1, deltas 2..=n.
    fn make_gop(n: u64) -> FrameSlot {
        let mut slot = FrameSlot {
            seq: n,
            key: n == 1,
            config: None,
            buf: VecDeque::new(),
        };
        for i in 1..=n {
            let key = i == 1;
            let data = vec![i as u8];
            slot.buf.push_back(Frame { seq: i, key, data, config: None });
        }
        slot
    }

    /// Decode-order FIFO: deltas must be delivered in order so the reference
    /// chain stays complete for WebCodecs.
    #[test]
    fn serve_next_delivers_gop_in_decode_order() {
        let mut slot = make_gop(5);
        let mut seqs = Vec::new();
        while let Some(fr) = serve_next(&mut slot, false) {
            if fr.seq > 1 {
                assert!(!fr.key, "only the first frame of a GOP may be a key");
            }
            seqs.push((fr.seq, fr.data[0]));
        }
        assert_eq!(seqs, vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);
    }

    /// need_key drains non-key frames and returns the next keyframe. With pop
    /// semantics this is the WebCodecs equivalent of waiting for the next IDR
    /// after a decoder reset — stale deltas are discarded (undecodable anyway).
    #[test]
    fn serve_next_need_key_skips_deltas_to_keyframe() {
        // buffer: delta 1, delta 2, key 3, delta 4
        let mut slot = FrameSlot { seq: 4, key: false, config: None, buf: VecDeque::new() };
        slot.buf.push_back(Frame { seq: 1, key: false, data: vec![1], config: None });
        slot.buf.push_back(Frame { seq: 2, key: false, data: vec![2], config: None });
        slot.buf.push_back(Frame { seq: 3, key: true, data: vec![3], config: None });
        slot.buf.push_back(Frame { seq: 4, key: false, data: vec![4], config: None });

        // need_key: pop & discard deltas 1,2, return key 3
        let fr = serve_next(&mut slot, true).expect("must find key 3");
        assert_eq!((fr.seq, fr.key, fr.data), (3, true, vec![3]), "need_key skips deltas 1,2 to key 3");
        assert_eq!(slot.buf.len(), 1, "deltas 1,2 discarded, key 3 popped, delta 4 remains");

        // then continue in decode order: delta 4
        let fr2 = serve_next(&mut slot, false).expect("delta 4");
        assert_eq!((fr2.seq, fr2.key), (4, false));
        assert!(serve_next(&mut slot, false).is_none(), "buffer drained");
    }

    /// If need_key is requested but no keyframe remains in the buffer, return
    /// None (the frontend will keep polling until the next IDR arrives).
    #[test]
    fn serve_next_need_key_returns_none_when_no_keyframe() {
        let mut slot = make_gop(3); // key 1, deltas 2,3
        // drain everything
        while serve_next(&mut slot, false).is_some() {}
        // buffer empty, need_key finds nothing
        assert!(serve_next(&mut slot, true).is_none());

        // push only deltas
        slot.buf.push_back(Frame { seq: 4, key: false, data: vec![4], config: None });
        slot.buf.push_back(Frame { seq: 5, key: false, data: vec![5], config: None });
        assert!(serve_next(&mut slot, true).is_none(), "no keyframe → None (deltas discarded)");
        assert!(slot.buf.is_empty(), "non-key frames drained");
    }

    /// `wm size` parsing: prefer Override (the effective resolution the input
    /// subsystem uses) over Physical, and tolerate extra whitespace. This is
    /// the value that feeds the frontend's click→device coordinate mapping, so
    /// a parse regression would silently break pointer alignment.
    #[test]
    fn parse_wm_size_prefers_override_over_physical() {
        let out = "Physical size: 1080x2400\nOverride size: 1440x3200\n";
        assert_eq!(parse_wm_size(out), Some((1440, 3200)));
    }

    #[test]
    fn parse_wm_size_falls_back_to_physical() {
        let out = "Physical size: 1080x2400\n";
        assert_eq!(parse_wm_size(out), Some((1080, 2400)));
    }

    #[test]
    fn parse_wm_size_tolerates_extra_whitespace() {
        let out = "Physical size:   720x1280 \n";
        assert_eq!(parse_wm_size(out), Some((720, 1280)));
    }

    #[test]
    fn parse_wm_size_returns_none_on_garbage() {
        assert_eq!(parse_wm_size("not a wm size output"), None);
        assert_eq!(parse_wm_size("Physical size: 0x0"), None);
        assert_eq!(parse_wm_size("Physical size: abc"), None);
    }
}
