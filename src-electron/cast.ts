// Screen casting via the scrcpy-server standalone mode.
//
// This is a faithful Node/TypeScript port of the Tauri backend
// (`src-tauri/src/cast.rs`), which in turn is a faithful translation of the
// official scrcpy v4.1 wire protocol (server -> client video socket). The
// video socket carries a 1-byte dummy byte, then the 64-byte device name, then
// a 4-byte codec id (0x68323634 "h264"), then a 12-byte session header (MSB
// set, then width/height), then a loop of packets each carrying a 12-byte
// header (8-byte PTS/flags + 4-byte packet size) followed by the raw
// MediaCodec output (avc format: 4-byte length-prefixed NALs).
//
// Config packets (FLAG_CONFIG) are parsed into an avcC `description` handed to
// WebCodecs, exactly like recorder.c uses the first config packet as
// extradata. The stream stays in avc format end to end; the frontend feeds the
// chunks straight into a WebCodecs VideoDecoder configured with the avcC.
// Touch/keyboard input is injected back with `adb shell input`.
//
// The Electron renderer runs Chromium, so WebCodecs (VideoDecoder) is
// available there too — CastPanel is no longer Tauri-only.

import * as net from 'net'
import * as path from 'path'
import * as fs from 'fs'
import { execFile, spawn } from 'child_process'
import type { ChildProcess } from 'child_process'
import type { Readable } from 'stream'

/// Must match the scrcpy-server jar version (the server refuses mismatches).
export const SCRCPY_VERSION = '4.1'
const REMOTE_JAR = '/data/local/tmp/scrcpy-server.jar'

// Official packet protocol constants (scrcpy v4.1, app/demuxer.c + Streamer.java).
const PACKET_HEADER_SIZE = 12
const PACKET_FLAG_SESSION = 1n << 63n
const PACKET_FLAG_CONFIG = 1n << 62n
const PACKET_FLAG_KEY_FRAME = 1n << 61n
const DEVICE_NAME_FIELD_LENGTH = 64
const CODEC_ID_H264 = 0x68323634 // "h264" in ASCII

export interface CastStartInfo {
  port: number
  width: number | null
  height: number | null
}

interface CastFrame {
  seq: number
  key: boolean
  data: Buffer
  config: Buffer | null
}

/**
 * Frame pushed to the renderer. The backend demuxes once and hands each frame
 * to the renderer as a binary `ArrayBuffer` (structured-cloned across the
 * MessageChannel) — no base64 anywhere. `config` is the current avcC whenever
 * one exists; the frontend compares it byte-wise and only reconfigures on
 * change, so steady-state frames trigger no decoder work.
 */
export type PushFrame =
  | { type: 'frame'; seq: number; key: boolean; data: ArrayBuffer; config: ArrayBuffer | null }
  | { type: 'disconnect' }

/**
 * Push sink for the frame stream. `main.ts` wraps an Electron
 * `MessagePortMain` (created via `MessageChannelMain`) around this interface;
 * `cast.ts` stays Electron-free so the demux logic remains portable/testable.
 */
export interface PushSink {
  post(msg: PushFrame): void
  close(): void
}

export class FrameSlot {
  seq = 0
  key = false
  config: Buffer | null = null
  buf: CastFrame[] = []
}
interface CastSession {
  socket: net.Socket | null
  child: ChildProcess | null
  localPort: number
  adbPath: string
  adbPort: string
  frame: FrameSlot
  sink: PushSink | null
  killed: boolean
  streamEnded: boolean
  forwardRemoved: boolean
}

const sessions = new Map<string, CastSession>()

// Push channels registered by the renderer (serial → sink). A channel can be
// opened before or after `start` — `start` attaches the pending sink to the new
// session, and `openPush` attaches to an existing live session (reuse path).
const sinks = new Map<string, PushSink>()

export function openPush(serial: string, sink: PushSink): void {
  const old = sinks.get(serial)
  if (old && old !== sink) closeSink(old)
  sinks.set(serial, sink)
  const s = sessions.get(serial)
  if (s) {
    closeSink(s.sink)
    s.sink = sink
    // Frames buffered before the sink attached were never delivered to anyone
    // (push mode doesn't poll) — drain them so no stale GOP lingers.
    s.frame.buf = []
  }
}

export function closePush(serial: string): void {
  const old = sinks.get(serial)
  if (old) closeSink(old)
  sinks.delete(serial)
  const s = sessions.get(serial)
  if (s) {
    closeSink(s.sink)
    s.sink = null
  }
}

function closeSink(sink: PushSink | null): void {
  if (!sink) return
  try {
    sink.close()
  } catch {
    /* already closed */
  }
}

/// Close every push channel (window closed, renderer gone). Sessions are left
/// running — the reader keeps demuxing into the FIFO until `stop`.
export function closeAllPushes(): void {
  for (const [serial, sink] of sinks) {
    closeSink(sink)
    sinks.delete(serial)
    const s = sessions.get(serial)
    if (s) s.sink = null
  }
}

/// Signal a hard stream end to the renderer over the push channel, then tear
/// the channel down. Called from the reader end/error paths and `stop`.
function pushDisconnect(s: CastSession): void {
  if (!s.sink) return
  try {
    s.sink.post({ type: 'disconnect' })
  } catch {
    /* channel already closed */
  }
  closeSink(s.sink)
  s.sink = null
}

/// Buffer → exact-sized ArrayBuffer. `Buffer` may be a view (subarray) into a
/// larger pool; slicing here prevents a structured clone from copying bytes
/// outside the frame.
function toArrayBuffer(b: Buffer): ArrayBuffer {
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer
}

// One-shot diagnostic flags (reset on each `start`) so the "first ..." logs do
// not spam on a live stream.
let logDevice = false
let logSession = false
let logConfig = false

function hexPrefix(b: Buffer): string {
  const parts: string[] = []
  for (let i = 0; i < Math.min(16, b.length); i++) {
    parts.push(b[i].toString(16).padStart(2, '0').toUpperCase())
  }
  return parts.join(' ')
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Run adb (with the resolved `-P <port>`) and return its stdout. */
function execAdb(adb: string, port: string, args: string[], timeoutMs = 30000): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    execFile(
      adb,
      ['-P', port, ...args],
      { timeout: timeoutMs, windowsHide: true, maxBuffer: 64 * 1024 * 1024 },
      (err, stdout, stderr) => {
        if (err) {
          const detail = (stderr && stderr.toString().trim()) || (err as Error).message
          reject(new Error(detail || String(err)))
        } else {
          resolve(Buffer.isBuffer(stdout) ? stdout : Buffer.from(stdout))
        }
      }
    )
  })
}

function removeForward(adb: string, port: string, localPort: number): void {
  execAdb(adb, port, ['forward', '--remove', `tcp:${localPort}`]).catch((e) =>
    console.warn('[cast] forward --remove failed:', e)
  )
}

/**
 * Locate the scrcpy-server jar: `AIDTERM_SCRCPY` env, bundled resource, or
 * error. Walk from `start` toward the filesystem root looking for
 * `bin/scrcpy-server.jar` (dev fallback; packaged builds use resources/bin).
 */
function findJarUpwards(start: string): string | null {
  let dir = start
  for (;;) {
    const candidate = path.join(dir, 'bin', 'scrcpy-server.jar')
    if (fs.existsSync(candidate)) return candidate
    const parent = path.dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

function findJar(): string {
  const env = process.env.AIDTERM_SCRCPY
  if (env && fs.existsSync(env)) return env
  const bundled = path.join(process.resourcesPath, 'bin', 'scrcpy-server.jar')
  if (fs.existsSync(bundled)) return bundled
  for (const start of [process.cwd(), path.dirname(process.execPath)]) {
    const found = findJarUpwards(start)
    if (found) return found
  }
  throw new Error('scrcpy-server.jar not found. Run `npm run fetch-scrcpy`, or set AIDTERM_SCRCPY to the jar path.')
}

/// Detect Annex-B byte stream format: NAL units separated by 00 00 01 (or
/// 00 00 00 01) start codes, as produced by some C2 encoders (e.g. Qualcomm
/// `c2.qti.avc.encoder` on HUAWEI NCO-AL00). avc format packets start with a
/// 4-byte big-endian NAL length instead, which cannot equal 00 00 01/00 00 00 01.
export function isAnnexb(data: Buffer): boolean {
  return (
    data.length >= 4 &&
    data[0] === 0 &&
    data[1] === 0 &&
    (data[2] === 1 || (data[2] === 0 && data[3] === 1))
  )
}

function isStartCode(data: Buffer, i: number): boolean {
  if (i + 3 > data.length) return false
  if (data[i] !== 0 || data[i + 1] !== 0) return false
  if (data[i + 2] === 1) return true
  return i + 4 <= data.length && data[i + 2] === 0 && data[i + 3] === 1
}

/**
 * Convert an Annex-B byte stream (00 00 00 01 / 00 00 01 start codes) into
 * avc format (4-byte big-endian length-prefixed NALs). Bytes before the first
 * start code are dropped. A NAL runs from its start code to the next start
 * code or to the end of the buffer; trailing zero bytes of the final NAL are
 * trimmed (a complete NAL can never end in 0x00 — its rbsp_trailing_bits end
 * with a 1). Returns null if no NAL is found.
 */
export function annexbToAvc(data: Buffer): Buffer | null {
  const parts: Buffer[] = []
  let i = 0
  // skip bytes until the first start code
  while (i + 3 < data.length && !isStartCode(data, i)) i++
  while (i + 3 < data.length) {
    // consume the start code (3 or 4 bytes)
    if (i + 4 <= data.length && data[i] === 0 && data[i + 1] === 0 && data[i + 2] === 0 && data[i + 3] === 1) {
      i += 4
    } else {
      i += 3
    }
    const start = i
    // scan to the next start code or EOF
    let end = data.length
    while (i + 3 < data.length) {
      if (isStartCode(data, i)) {
        end = i
        break
      }
      i++
    }
    let nal = data.subarray(start, end)
    if (end === data.length) {
      // final NAL runs to EOF: trim trailing zero padding
      while (nal.length > 0 && nal[nal.length - 1] === 0) {
        nal = nal.subarray(0, nal.length - 1)
      }
    }
    if (nal.length === 0) {
      i = end
      continue
    }
    const lenBuf = Buffer.alloc(4)
    lenBuf.writeUInt32BE(nal.length, 0)
    parts.push(lenBuf, nal)
    i = end
  }
  if (parts.length === 0) return null
  return Buffer.concat(parts)
}

/// Return the packet payload in avc format: Annex-B input is converted to
/// length-prefixed NALs, avc input is returned unchanged. The frontend feeds
/// WebCodecs (with the avcC description), which expects consistent avc chunks.
export function normalizeAvc(data: Buffer): Buffer {
  if (isAnnexb(data)) {
    return annexbToAvc(data) ?? Buffer.from(data)
  }
  return Buffer.from(data)
}

/**
 * Build an avcC record from raw SPS/PPS payloads (without NAL header bytes).
 *
 * The avcC SPS/PPS NAL units MUST include their 1-byte NAL header (0x67/0x68).
 * Chromium's WebCodecs path (description present) converts every avc chunk
 * back to Annex-B via `H264ToAnnexBBitstreamConverter`, which writes the
 * avcC parameter sets verbatim after a start code and then parses their first
 * byte as the NAL type. Without the header, an injected SPS reads as a
 * non-IDR VCL slice (its profile_idc byte, e.g. 0x64, is type 4) and
 * `AnalyzeAnnexB` marks the chunk as "not a key frame", producing
 * `An EncodedVideoChunk was marked as type 'key' but wasn't a key frame`.
 */
export function buildAvcc(sps: Buffer, pps: Buffer): Buffer | null {
  if (sps.length < 4) return null
  const out = Buffer.allocUnsafe(sps.length + pps.length + 13)
  let o = 0
  out[o++] = 0x01 // configurationVersion
  out[o++] = sps[0] // profile_idc
  out[o++] = sps[1] // profile_compatibility
  out[o++] = sps[2] // level_idc
  out[o++] = 0xff // reserved(6) + lengthSizeMinusOne(3)
  out[o++] = 0xe1 // reserved(3) + numOfSPS(5)
  out.writeUInt16BE(sps.length + 1, o)
  o += 2
  out[o++] = 0x67 // NAL header for SPS
  sps.copy(out, o)
  o += sps.length
  out[o++] = 0x01 // numOfPPS
  out.writeUInt16BE(pps.length + 1, o)
  o += 2
  out[o++] = 0x68 // NAL header for PPS
  pps.copy(out, o)
  o += pps.length
  return out.subarray(0, o)
}

/// Build the avcC `description` from a MediaCodec CODEC_CONFIG packet. Some
/// encoders emit Annex-B start codes (00 00 00 01) instead of avc length
/// prefixes, so detect and normalize that first. This mirrors recorder.c,
/// which uses the first config packet as extradata.
export function buildAvccFromConfig(config: Buffer): Buffer | null {
  const c = isAnnexb(config) ? annexbToAvc(config) : config
  if (!c) return null
  let off = 0
  let sps: Buffer | null = null
  let pps: Buffer | null = null
  while (off + 4 <= c.length) {
    const len = c.readUInt32BE(off)
    if (len === 0 || off + 4 + len > c.length) break
    const nal = c.subarray(off + 4, off + 4 + len)
    const type = (nal[0] ?? 0) & 0x1f
    if (type === 7 && sps === null) sps = nal.subarray(1)
    else if (type === 8 && pps === null) pps = nal.subarray(1)
    off += 4 + len
  }
  if (sps === null || pps === null) return null
  return buildAvcc(sps, pps)
}

/// Buffered byte stream over the video socket, equivalent to `net_recv_all`.
export class Demuxer {
  private buf: Buffer
  private pos = 0

  constructor(initial: Buffer) {
    this.buf = initial
  }

  feed(data: Buffer): void {
    // Compact before appending so a large consumed prefix cannot leak.
    if (this.pos > 1 << 20) {
      this.buf = this.buf.subarray(this.pos)
      this.pos = 0
    }
    this.buf = Buffer.concat([this.buf, data])
  }

  avail(): number {
    return this.buf.length - this.pos
  }

  /// Non-destructive look at the next `n` bytes (they must be available).
  peek(n: number): Buffer {
    return this.buf.subarray(this.pos, this.pos + n)
  }

  take(n: number): Buffer {
    const s = this.buf.subarray(this.pos, this.pos + n)
    this.pos += n
    return s
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

export class StreamState {
  phase = Phase.DummyByte
  /// Latest avcC built from the most recent CODEC_CONFIG packet (or from
  /// inline SPS/PPS in a keyframe, as a fallback for encoders that don't
  /// send a separate config packet). Handed to WebCodecs as `description`.
  config: Buffer | null = null
  /// Diagnostics for the "session update but no media" restart loop.
  sessionUpdates = 0
  lastSessionAt: number | null = null
}

type DemuxStatus = 'need-data' | 'disabled' | 'error'

/// Consume as many complete protocol elements as `demux` currently holds.
/// Mirrors demuxer.c `run_demuxer()` + `packet_merger.c`.
export function demux(d: Demuxer, st: StreamState, slot: FrameSlot, sink: PushSink | null = null): DemuxStatus {
  for (;;) {
    switch (st.phase) {
      case Phase.DummyByte: {
        if (d.avail() < 1) return 'need-data'
        d.take(1)
        st.phase = Phase.DeviceMeta
        break
      }
      case Phase.DeviceMeta: {
        if (d.avail() < DEVICE_NAME_FIELD_LENGTH) return 'need-data'
        const meta = d.take(DEVICE_NAME_FIELD_LENGTH)
        const nul = meta.indexOf(0)
        const name = meta.subarray(0, nul === -1 ? meta.length : nul).toString('utf8')
        if (!logDevice) {
          logDevice = true
          console.log('[cast] device:', name)
        }
        st.phase = Phase.CodecId
        break
      }
      case Phase.CodecId: {
        if (d.avail() < 4) return 'need-data'
        const id = d.take(4).readUInt32BE(0)
        if (id === 0) {
          console.warn('[cast] stream explicitly disabled by the device')
          return 'disabled'
        }
        if (id === 1) {
          console.error('[cast] stream configuration error on the device')
          return 'error'
        }
        if (id !== CODEC_ID_H264) {
          console.error(`[cast] unsupported codec id 0x${id.toString(16).padStart(8, '0')} (only h264 is wired up)`)
          return 'error'
        }
        st.phase = Phase.Session
        break
      }
      case Phase.Session: {
        if (d.avail() < PACKET_HEADER_SIZE) return 'need-data'
        const h = d.take(PACKET_HEADER_SIZE)
        if ((h[0] & 0x80) === 0) {
          console.error('[cast] unexpected packet (not a session header)')
          return 'error'
        }
        const width = h.readUInt32BE(4)
        const height = h.readUInt32BE(8)
        if (width === 0 || height === 0) {
          console.error(`[cast] invalid session video size: ${width}x${height}`)
          return 'error'
        }
        if (!logSession) {
          logSession = true
          console.log(`[cast] session ${width}x${height}`)
        }
        st.phase = Phase.Packets
        break
      }
      case Phase.Packets: {
        if (d.avail() < PACKET_HEADER_SIZE) return 'need-data'
        // Peek (not consume) the header first: a media/config packet is only
        // fully readable when header + payload are both buffered.
        const h = d.peek(PACKET_HEADER_SIZE)
        const ptsFlags = h.readBigUInt64BE(0)
        const len = h.readUInt32BE(8)
        if ((ptsFlags & PACKET_FLAG_SESSION) !== 0n) {
          // Session packet (orientation/size change): 12-byte header only, no
          // payload. Mirrors scrcpy demuxer.c which just parses the session.
          d.take(PACKET_HEADER_SIZE)
          const width = h.readUInt32BE(4)
          const height = h.readUInt32BE(8)
          const now = Date.now()
          const gap = st.lastSessionAt === null ? 0 : now - st.lastSessionAt
          st.lastSessionAt = now
          st.sessionUpdates += 1
          if (st.sessionUpdates <= 5 || st.sessionUpdates % 25 === 0) {
            console.log(`[cast] session update ${width}x${height} (#${st.sessionUpdates}) gap ${gap}ms`)
          }
          if (st.sessionUpdates === 25) {
            console.warn('[cast] 25 session updates with no media frame yet — capture/encoder reset loop on the device; capture `adb logcat -s scrcpy` during casting for the server-side error')
          }
          break
        }
        if (len === 0) {
          console.error('[cast] invalid packet length: 0')
          return 'error'
        }
        // Atomic consume: do not advance past the header until the whole
        // packet (header + payload) is buffered.
        if (d.avail() < PACKET_HEADER_SIZE + len) return 'need-data'
        d.take(PACKET_HEADER_SIZE)
        const raw = d.take(len)
        if ((ptsFlags & PACKET_FLAG_CONFIG) !== 0n) {
          // Config packet (SPS/PPS, pts == AV_NOPTS_VALUE). WebCodecs wants the
          // avcC as `description` (passed to configure()); the chunk data is
          // the raw frame, so no merge like scrcpy/FFmpeg.
          const avcc = buildAvccFromConfig(raw)
          if (avcc) {
            if (!logConfig) {
              logConfig = true
              console.log(`[cast] avcC built from config packet: ${hexPrefix(avcc)}`)
            }
            st.config = avcc
          } else {
            console.warn('[cast] config packet did not yield avcC (no SPS/PPS found)')
          }
          break
        }
        // Media packet. The frame data is the raw MediaCodec output (avc or
        // Annex-B, normalized to avc). Unlike scrcpy we do NOT prepend the
        // config packet — WebCodecs gets it as `description`.
        const key = (ptsFlags & PACKET_FLAG_KEY_FRAME) !== 0n
        const data = normalizeAvc(raw)
        // Fallback: some encoders never send a separate CODEC_CONFIG packet and
        // inline SPS/PPS in every IDR. Without an avcC the frontend cannot
        // configure the decoder, so extract one from the keyframe.
        let config = st.config
        if (!config && key) {
          const avcc = buildAvccFromConfig(raw)
          if (avcc) {
            console.log(`[cast] avcC built from inline keyframe SPS/PPS: ${hexPrefix(avcc)}`)
            st.config = avcc
            config = avcc
          }
        }
        emit(data, key, config, slot, sink)
        break
      }
    }
  }
}

function emit(frame: Buffer, key: boolean, config: Buffer | null, slot: FrameSlot, sink: PushSink | null): void {
  slot.seq += 1
  const seq = slot.seq
  slot.key = key
  slot.config = config ? Buffer.from(config) : null
  if (!sink) {
    // Push to the bounded FIFO. The frontend pops in decode order.
    slot.buf.push({ seq, key, data: frame, config: config ? Buffer.from(config) : null })
    // Cap the backlog (~10s at 30fps). If the frontend falls this far behind,
    // drop the oldest frames — stale deltas can't decode without their reference
    // chain anyway.
    while (slot.buf.length > 300) slot.buf.shift()
    return
  }
  // Push mode: hand the frame straight to the renderer as binary ArrayBuffers
  // (structured-cloned across the MessageChannel). No base64, no FIFO backlog.
  sink.post({
    type: 'frame',
    seq,
    key,
    data: toArrayBuffer(frame),
    config: config ? toArrayBuffer(config) : null,
  })
}

export function serveNext(f: FrameSlot, needKey: boolean): CastFrame | null {
  if (needKey) {
    while (f.buf.length > 0) {
      const front = f.buf[0]
      if (front.key) return f.buf.shift()!
      f.buf.shift()
    }
    return null
  }
  return f.buf.shift() ?? null
}

/**
 * Pop the next frame for the frontend, in decode order.
 *
 * * Normal poll: pop the oldest frame from the FIFO. Frames are consumed in
 *   order, never replayed (matches scrcpy's demuxer→decoder pipeline).
 * * `need_key` (decoder lost): pop and DISCARD every non-key frame until the
 *   next keyframe — deltas without their reference chain are undecodable.
 */
function demuxAll(d: Demuxer, st: StreamState, session: CastSession): boolean {
  for (;;) {
    if (session.killed) return true
    const status = demux(d, st, session.frame, session.sink)
    if (status === 'need-data') return true
    if (status === 'disabled') {
      console.warn('[cast] stream disabled by device')
      session.streamEnded = true
      return false
    }
    if (status === 'error') {
      console.error('[cast] demux error, stopping reader')
      session.streamEnded = true
      return false
    }
  }
}

function cleanupSession(s: CastSession): void {
  if (s.socket) {
    s.socket.destroy()
    s.socket = null
  }
  if (s.child && s.child.exitCode === null) {
    s.child.kill()
  }
  s.child = null
  if (!s.forwardRemoved) {
    s.forwardRemoved = true
    removeForward(s.adbPath, s.adbPort, s.localPort)
  }
}

function startReader(socket: net.Socket, firstChunk: Buffer, session: CastSession): void {
  const demuxer = new Demuxer(firstChunk)
  const state = new StreamState()
  let reads = 0
  let bytes = 0
  let errors = 0
  let timeouts = 0
  let endedLogged = false

  const endStream = (): void => {
    if (session.killed) return
    if (!session.streamEnded) {
      console.log(`[cast] reader ended after ${reads} reads / ${bytes} bytes`)
    }
    session.streamEnded = true
    pushDisconnect(session)
    if (session.socket === socket) session.socket = null
    socket.destroy()
    cleanupSession(session)
  }

  socket.setTimeout(1000)
  socket.on('data', (chunk: Buffer) => {
    if (session.killed) return
    reads++
    bytes += chunk.length
    timeouts = 0
    demuxer.feed(chunk)
    if (!demuxAll(demuxer, state, session)) {
      endStream()
      return
    }
  })
  socket.on('timeout', () => {
    if (session.killed) return
    timeouts++
    // Log periodic timeout warnings (every 5th timeout = 5s)
    if (timeouts === 5 || (timeouts > 5 && timeouts % 30 === 0)) {
      console.log(`[cast] reader timeout #${timeouts}: no data for ${timeouts}s (reads ${reads} / ${bytes} bytes)`)
    }
    // After 120 consecutive timeouts (~2 minutes) with no data, the stream is
    // truly dead — mark it ended. Short bursts of timeouts are normal (screen
    // static, encoder pauses).
    if (timeouts >= 120 && !endedLogged) {
      console.warn('[cast] reader timed out after 120 consecutive timeouts (~2 min), marking stream ended')
      session.streamEnded = true
      endedLogged = true
    }
    socket.setTimeout(1000)
  })
  socket.on('end', () => endStream())
  socket.on('error', (e: Error) => {
    if (session.killed) return
    errors++
    if (errors <= 5 || errors % 25 === 0) {
      console.log(`[cast] reader I/O error #${errors}: ${e.message} (after ${reads} reads / ${bytes} bytes)`)
    }
    // On persistent I/O errors, mark the stream as ended so the frontend can
    // detect disconnection.
    if (errors >= 5 && !endedLogged) {
      console.warn('[cast] reader stopped after 5 I/O errors, marking stream ended')
      session.streamEnded = true
      endedLogged = true
    }
  })
  socket.on('close', () => endStream())

  // Drain whatever the probe chunk already contains before the socket flows.
  demuxAll(demuxer, state, session)
  socket.resume()
}

/**
 * Probe-connect to the forwarded port. adb accepts the host-side TCP connection
 * immediately, even before the device socket exists, and such a connection
 * promptly EOFs. So a successful connect is NOT enough — we read a chunk to
 * confirm the server is actually streaming before handing the stream to the
 * reader.
 *
 * IMPORTANT: the scrcpy server accepts exactly ONE video connection, and
 * `send_dummy_byte` (default true) makes it write a single 0x00 byte right
 * after accepting. If our read times out (500ms) the connection is still alive
 * — the device-side adb handshake is just slow — and the server may ALREADY
 * have accepted it. Dropping such a connection kills the stream, so every
 * later probe EOFs. Therefore: only clean EOF (socket not created yet) triggers
 * a retry; a read timeout commits the connection and the reader waits for bytes.
 */
type ProbeResult =
  | { kind: 'live'; socket: net.Socket; firstChunk: Buffer }
  | { kind: 'commit'; socket: net.Socket; firstChunk: Buffer }
  | { kind: 'eof' }
  | { kind: 'err' }

function attemptProbe(port: number): Promise<ProbeResult> {
  return new Promise((resolve) => {
    const socket = net.connect({ host: '127.0.0.1', port })
    let settled = false
    const settle = (kind: 'live' | 'commit', firstChunk: Buffer): void => {
      if (settled) return
      settled = true
      socket.setTimeout(0)
      socket.pause()
      socket.removeAllListeners('data')
      socket.removeAllListeners('end')
      socket.removeAllListeners('timeout')
      socket.removeAllListeners('connect')
      // Keep a no-op error handler attached so a stray socket error between the
      // probe and the reader wiring cannot crash the process (uncaught 'error').
      socket.on('error', () => {})
      resolve({ kind, socket, firstChunk })
    }
    const fail = (kind: 'eof' | 'err'): void => {
      if (settled) return
      settled = true
      socket.removeAllListeners('error')
      socket.removeAllListeners('end')
      socket.removeAllListeners('timeout')
      socket.removeAllListeners('connect')
      socket.destroy()
      resolve({ kind })
    }
    socket.once('connect', () => {
      socket.setTimeout(500)
    })
    socket.on('data', (chunk: Buffer) => {
      settle('live', chunk)
    })
    socket.on('timeout', () => {
      settle('commit', Buffer.alloc(0))
    })
    socket.on('end', () => fail('eof'))
    socket.on('error', () => fail('err'))
  })
}

async function probeConnect(
  localPort: number,
  child: ChildProcess,
  adb: string,
  adbPort: string
): Promise<{ socket: net.Socket; firstChunk: Buffer }> {
  const deadline = Date.now() + 30000
  let attempts = 0
  for (;;) {
    if (Date.now() >= deadline) break
    if (child.exitCode !== null || child.signalCode !== null) {
      // scrcpy server exited early
      if (child.exitCode === null) child.kill()
      removeForward(adb, adbPort, localPort)
      throw new Error('scrcpy server exited early')
    }
    attempts += 1
    const r = await attemptProbe(localPort)
    if (r.kind === 'live' || r.kind === 'commit') {
      if (r.firstChunk.length > 0) {
        console.log(
          `[cast] probe attempt ${attempts}: LIVE, ${r.firstChunk.length} bytes first, head: ${hexPrefix(r.firstChunk.subarray(0, Math.min(16, r.firstChunk.length)))}`
        )
      } else {
        console.log(`[cast] probe attempt ${attempts}: connected, no data within 500ms — committing (server will accept + stream shortly)`)
      }
      return { socket: r.socket, firstChunk: r.firstChunk }
    }
    console.log(
      r.kind === 'eof'
        ? `[cast] probe attempt ${attempts}: connected but EOF (device not ready), retry`
        : `[cast] probe attempt ${attempts}: connect failed, retry`
    )
    await sleep(100)
  }
  // Kill the server and remove the forward so a failed start does not leak a
  // zombie server/socket that poisons the next attempt.
  child.kill()
  removeForward(adb, adbPort, localPort)
  throw new Error('scrcpy server did not deliver a live stream within 30s')
}

/// Query the device's real display resolution via `adb shell wm size`.
async function screenSize(adb: string, port: string, serial: string): Promise<[number, number] | null> {
  try {
    const out = await execAdb(adb, port, ['-s', serial, 'shell', 'wm', 'size'])
    return parseWmSize(out.toString('utf8'))
  } catch {
    return null
  }
}

/// Parse `adb shell wm size` output. Prefers Override (the effective
/// resolution the input subsystem uses) over Physical.
export function parseWmSize(text: string): [number, number] | null {
  let physical: [number, number] | null = null
  let overrideSz: [number, number] | null = null
  for (const line of text.split('\n')) {
    const l = line.trim()
    if (l.startsWith('Physical size:')) {
      physical = parseSizePair(l.slice('Physical size:'.length))
    } else if (l.startsWith('Override size:')) {
      overrideSz = parseSizePair(l.slice('Override size:'.length))
    }
  }
  return overrideSz ?? physical
}

function parseSizePair(s: string): [number, number] | null {
  const parts = s.trim().split('x')
  if (parts.length !== 2) return null
  const w = parseInt(parts[0].trim(), 10)
  const h = parseInt(parts[1].trim(), 10)
  if (Number.isNaN(w) || Number.isNaN(h) || w === 0 || h === 0) return null
  return [w, h]
}

function pickLocalPort(): number {
  return 20000 + Number(process.hrtime.bigint() % 20000n)
}

function rand31bit(): number {
  return Math.max(Number(process.hrtime.bigint() % BigInt(0x7fffffff)), 1)
}

function pipeChildLog(child: ChildProcess): void {
  const pipe = (stream: Readable | null, level: 'info' | 'warn', tag: string): void => {
    if (!stream) return
    let pending = ''
    stream.on('data', (chunk: Buffer) => {
      pending += chunk.toString('utf8')
      let idx: number
      while ((idx = pending.indexOf('\n')) >= 0) {
        const line = pending.slice(0, idx).replace(/\r$/, '')
        pending = pending.slice(idx + 1)
        if (line) {
          if (level === 'info') console.log(`[cast][srv${tag}] ${line}`)
          else console.warn(`[cast][srv${tag}] ${line}`)
        }
      }
    })
    stream.on('end', () => {
      if (pending.trim()) {
        if (level === 'info') console.log(`[cast][srv${tag}] ${pending.trim()}`)
        else console.warn(`[cast][srv${tag}] ${pending.trim()}`)
      }
    })
  }
  pipe(child.stdout, 'info', '')
  pipe(child.stderr, 'warn', '-err')
}

/// Start casting a device: push the jar, forward a port, launch the server.
export async function start(adbPath: string, adbPort: string, serial: string, maxSize: number): Promise<CastStartInfo> {
  const existing = sessions.get(serial)
  if (existing && !existing.killed && !existing.streamEnded) {
    // If session is alive AND stream is still active, reuse it. Re-query the
    // real screen size so the frontend's input mapping stays correct after a
    // reconnect.
    const size = await screenSize(adbPath, adbPort, serial)
    return { port: existing.localPort, width: size ? size[0] : null, height: size ? size[1] : null }
  }
  // Reap any leftover session (killed, or stream ended).
  const old = sessions.get(serial)
  if (old) {
    old.killed = true
    old.streamEnded = true
    cleanupSession(old)
    sessions.delete(serial)
  }

  // Query the real display resolution up front so the frontend can map click
  // positions to the device's actual coordinate space even when max_size
  // downscales the streamed video.
  const realSize = await screenSize(adbPath, adbPort, serial)

  const jar = findJar()
  const localPort = pickLocalPort()
  console.log('[cast] jar:', jar)

  // 1. Push the server jar onto the device.
  try {
    await execAdb(adbPath, adbPort, ['-s', serial, 'push', jar, REMOTE_JAR], 60000)
  } catch (e) {
    throw new Error(`adb push failed: ${(e as Error).message}`)
  }
  console.log(`[cast] pushed jar to ${REMOTE_JAR}`)

  // 2. Forward a local TCP port to the device's localabstract socket. The
  //    server derives its socket name as `scrcpy_%08x` from scid, so both must
  //    be written as zero-padded lowercase hex (no 0x prefix: the server parses
  //    scid with Integer.parseInt(value, 16) which rejects "0x").
  const scid = rand31bit()
  const scidHex = scid.toString(16).padStart(8, '0')
  const localabstract = `localabstract:scrcpy_${scidHex}`
  try {
    await execAdb(adbPath, adbPort, ['-s', serial, 'forward', `tcp:${localPort}`, localabstract])
  } catch (e) {
    throw new Error(`adb forward failed: ${(e as Error).message}`)
  }
  console.log(`[cast] forward tcp:${localPort} -> ${localabstract}`)

  // 3. Launch the server; it streams once we connect to the forwarded port.
  //    Server params mirror the official client's for this option set
  //    (video-only, tunnel forward), meta options all at defaults.
  const serverArgs = [
    '-P', adbPort, '-s', serial, 'shell',
    `CLASSPATH=${REMOTE_JAR}`,
    'app_process', '/', 'com.genymobile.scrcpy.Server', SCRCPY_VERSION,
    `scid=${scidHex}`,
    'tunnel_forward=true', 'audio=false', 'control=false', 'cleanup=false',
    'max_fps=30',
  ]
  if (maxSize > 0) serverArgs.push(`max_size=${maxSize}`)
  const child = spawn(adbPath, serverArgs, { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true })
  console.log(`[cast] scrcpy server launched: scid ${scidHex} max_size ${maxSize}`)
  // Surface the server's own logs (and crash output) so startup problems on a
  // device are visible instead of being swallowed.
  pipeChildLog(child)

  // 4. Probe-connect (see attemptProbe/probeConnect).
  const { socket, firstChunk } = await probeConnect(localPort, child, adbPath, adbPort)

  const session: CastSession = {
    socket,
    child,
    localPort,
    adbPath,
    adbPort,
    frame: new FrameSlot(),
    sink: sinks.get(serial) ?? null,
    killed: false,
    streamEnded: false,
    forwardRemoved: false,
  }
  sessions.set(serial, session)
  logDevice = false
  logSession = false
  logConfig = false
  startReader(socket, firstChunk, session)
  return { port: localPort, width: realSize ? realSize[0] : null, height: realSize ? realSize[1] : null }
}

export function stop(serial: string): void {
  const s = sessions.get(serial)
  if (!s) return
  s.killed = true
  s.streamEnded = true
  pushDisconnect(s)
  cleanupSession(s)
  sessions.delete(serial)
}

export function frame(serial: string, needKey: boolean): [number, boolean, string, string | null] | null {
  const s = sessions.get(serial)
  if (!s) return null
  const fr = serveNext(s.frame, needKey)
  if (fr) {
    return [fr.seq, fr.key, fr.data.toString('base64'), fr.config ? fr.config.toString('base64') : null]
  }
  // No frame available right now. If the stream ended, signal disconnect.
  // Otherwise return seq=0 so the frontend knows "no new frame, keep polling"
  // without mistaking it for a disconnect.
  if (s.streamEnded) return null
  return [0, false, '', null]
}

export async function input(adbPath: string, adbPort: string, serial: string, cmd: string): Promise<void> {
  const args = ['-s', serial, 'shell', 'input', ...cmd.split(/\s+/).filter((s) => s.length > 0)]
  await execAdb(adbPath, adbPort, args, 15000)
}
