import { app, BrowserWindow, ipcMain, dialog, clipboard, MessageChannelMain } from 'electron'
import * as path from 'path'
const pathModule = path
import * as os from 'os'
import * as fs from 'fs'
import * as net from 'net'
import * as crypto from 'crypto'
import * as cast from './cast'
import type { Client as Ssh2Client, ClientChannel, ConnectConfig as SshConnectConfig } from 'ssh2'
import type SftpClient from 'ssh2-sftp-client'
import type {
  PtySession, SshSession, SerialSession, TunnelState, TunnelInfo,
  ProxyConfig, AiMessage, AiResponse, AiConfig, AiToolCall,
  KeyInfo, KnownHostEntry, SessionStoreData, SystemInfo,
  FileEntry, FileProgress, SerialPortInfo,
  SpawnTerminalArgs, WriteTerminalArgs, ResizeTerminalArgs, KillTerminalArgs,
  SshConnectArgs, TelnetConnectArgs, SerialConnectArgs,
  AdbConnectArgs, AdbDevice, AdbStatus,
  ConnectionConfig, ConnectionHandle, ConnectionCreateArgs,
  FileOpArgs, FileConnectArgs, FileListDirArgs, FileTransferArgs, FileRemoveArgs,
  FileRenameArgs, FileMkdirArgs, FileCreateArgs, FileReadArgs, FileWriteArgs,
  TunnelCreateArgs, TunnelRemoveArgs,
  ProxySaveArgs, ProxyDeleteArgs,
  AiChatArgs, AiExecuteArgs, AiContinueArgs, AiClearHistoryArgs,
  FetchAiModelsArgs, GetRemoteSystemInfoArgs, GetRemoteSystemMetricsArgs,
  RemoteSystemMetrics,
  SaveSessionStoreArgs, KeyGenerateRsaArgs, KeyGenerateEd25519Args,
  KeyDeleteArgs, KeyImportArgs, KnownHostsAddArgs, KnownHostsRemoveArgs,
  WindowSetFullscreenArgs, ClipboardWriteArgs, ToggleSettingArgs,
  OpenDialogOpts, SaveDialogOpts, DialogFilter,
  TunnelType,
} from './types'

// ── Lazy-loaded native modules ──
let ptyModule: typeof import('node-pty') | null = null
let ssh2Module: typeof import('ssh2') | null = null
let SftpClientClass: typeof SftpClient | null = null
let SerialPortClass: typeof import('serialport').SerialPort | null = null

function loadNativeModules(): void {
  try { ptyModule = require('node-pty') } catch (e) { console.warn('[electron] node-pty not available:', e) }
  try { ssh2Module = require('ssh2') } catch (e) { console.warn('[electron] ssh2 not available:', e) }
  try { SftpClientClass = require('ssh2-sftp-client') } catch (e) { console.warn('[electron] ssh2-sftp-client not available:', e) }
  try { SerialPortClass = require('serialport').SerialPort } catch (e) { console.warn('[electron] serialport not available:', e) }
}

/// First 200 characters of a raw response, for error diagnostics (no cut in
/// the middle of a UTF-8 sequence).
function snippet(s: string): string {
  const head = s.slice(0, 200)
  return head.length < s.length ? `${head}…` : head
}

let mainWindow: BrowserWindow | null = null
const isDev = !app.isPackaged

// ── In-memory state ──
const ptySessions = new Map<string, PtySession>()
const sshSessions = new Map<string, SshSession>()
const serialSessions = new Map<string, SerialSession>()
const wslSessions = new Map<string, string>()
const sftpConnections = new Map<string, SftpClient>()
const sftpTransfers = new Map<string, { abort: () => void }>()
const tunnelMap = new Map<string, TunnelState>()
const proxyConfigs: ProxyConfig[] = []
const aiHistories = new Map<string, AiMessage[]>()
const aiAborters = new Map<string, AbortController>()
const keyIndex = new Map<string, KeyInfo>()
let keysDir = ''
let keyIndexPath = ''
let knownHostsPath = ''
let appDataDir = ''

function getAppDataDir(): string {
  return appDataDir
}

function emitToRenderer(event: string, payload: Record<string, unknown>): void {
  mainWindow?.webContents.send(event, payload)
}

// ══════════════════════════════════════════════════════
//  ADB helpers
//
//  Every resolved adb binary (bundled or external) talks to the shared default
//  5037 server, so AidTerm sees the same devices as the user's other adb tools.
//  "bundled" only affects whether the server is stopped when the last adb
//  session closes.
// ══════════════════════════════════════════════════════
const ADB_PORT = '5037'
/** Backward-compatible alias: every source talks to the shared 5037 port. */
const ADB_DEFAULT_PORT = ADB_PORT

type AdbSource = 'env' | 'bundled' | 'path' | 'missing'

interface AdbResolution {
  path: string
  port: string
  source: AdbSource
}

function findInPath(exeName: string): string | null {
  const pathVar = process.env.PATH || ''
  for (const dir of pathVar.split(path.delimiter)) {
    const candidate = path.join(dir, exeName)
    if (fs.existsSync(candidate)) return candidate
  }
  return null
}

// Cached result of a process scan for an already-running adb process.
// `undefined` means "not probed yet", `null` = "probed, no adb running".
let runningAdbPath: string | null | undefined = undefined

/**
 * Enumerate running processes for an existing adb executable and cache it.
 * Only rescans when `force` is true (i.e. when the ADB connect dialog lists
 * devices); afterwards every adb call reuses the cached result so the session
 * and its cast share the same adb discovered at scan time.
 */
async function resolveRunningAdb(force: boolean): Promise<string | null> {
  if (!force && runningAdbPath !== undefined) return runningAdbPath
  try {
    const si = require('systeminformation') as typeof import('systeminformation')
    const { list } = await si.processes()
    const target = process.platform === 'win32' ? 'adb.exe' : 'adb'
    const found = list.find((p) => p.name.toLowerCase() === target.toLowerCase())
    if (found) {
      // Prefer the concrete binary of the running process; fall back to PATH.
      const exe = (found.path && fs.existsSync(found.path)) ? found.path : findInPath(target)
      runningAdbPath = exe ?? findInPath(target) ?? null
    } else {
      runningAdbPath = null
    }
    if (runningAdbPath) console.log('[electron] existing adb process found, reusing:', runningAdbPath)
  } catch {
    runningAdbPath = null
  }
  return runningAdbPath
}

/** Reuse the cached running-adb result (session ops, no process rescan). */
async function ensureAdbProbed(): Promise<void> {
  await resolveRunningAdb(false)
}

/** Re-scan for a running adb (called when the connect dialog lists devices). */
async function refreshRunningAdb(): Promise<void> {
  await resolveRunningAdb(true)
}

function resolveAdb(): AdbResolution {
  const envOverride = process.env.AIDTERM_ADB
  if (envOverride && fs.existsSync(envOverride)) return { path: envOverride, port: ADB_DEFAULT_PORT, source: 'env' }
  const exeName = process.platform === 'win32' ? 'adb.exe' : 'adb'
  // When an adb process is already running, reuse its executable (5037) rather
  // than starting AidTerm's own server; share the user's devices.
  if (runningAdbPath) return { path: runningAdbPath, port: ADB_DEFAULT_PORT, source: 'path' }
  const bundled = path.join(process.resourcesPath, 'bin', exeName)
  if (fs.existsSync(bundled)) return { path: bundled, port: ADB_PORT, source: 'bundled' }
  const fromPath = findInPath(exeName)
  if (fromPath) return { path: fromPath, port: ADB_DEFAULT_PORT, source: 'path' }
  // execFile will surface ENOENT when the fallback path is attempted.
  return { path: exeName, port: ADB_DEFAULT_PORT, source: 'missing' }
}

async function adbStatus(): Promise<AdbStatus> {
  // The dialog open is the discovery point: refresh the running-adb cache.
  await refreshRunningAdb()
  const { path: p, port, source } = resolveAdb()
  return {
    available: source !== 'missing',
    source,
    path: source !== 'missing' ? p : null,
    port: source !== 'missing' ? port : null,
  }
}

/** Map a WSL POSIX path onto the `\\wsl$` UNC namespace for a distro. */
function wslUncPath(distro: string, remote: string): string {
  const root = `\\\\wsl$\\${distro}`
  const rel = remote.replace(/^\//, '')
  if (!rel) return root
  return `${root}\\${rel.split('/').filter(Boolean).join('\\')}`
}

/** Resolve a browsed path for the `local`/`wsl` file kinds. */
function resolveLocalFsPath(kind: string, handle: string, remote: string): string {
  if (kind === 'wsl') return wslUncPath(handle, remote)
  if (!remote) return os.homedir()
  return remote
}

/** Distro for a WSL session (empty string = default distro). */
function wslDistroForSession(sessionId: string): string {
  return wslSessions.get(sessionId) ?? ''
}

function formatLocalMtime(ms: number): string {
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

async function runAdb(args: string[]): Promise<string> {
  const { execFile } = require('child_process') as typeof import('child_process')
  await ensureAdbProbed()
  return new Promise<string>((resolve, reject) => {
    const { path: adb, port } = resolveAdb()
    execFile(adb, ['-P', port, ...args], { timeout: 15000 }, (err: Error | null, stdout: string | Buffer) => {
      if (err) reject(new Error(err.message))
      else resolve(stdout.toString())
    })
  })
}

function parseAdbDevices(output: string): AdbDevice[] {
  const devices: AdbDevice[] = []
  for (const line of output.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('List of devices') || trimmed.startsWith('* daemon')) continue
    const tokens = trimmed.split(/\s+/)
    const serial = tokens[0]
    const state = tokens[1] || 'unknown'
    let model = ''
    let product = ''
    let transport_id: string | null = null
    for (let i = 2; i < tokens.length; i++) {
      const [key, value] = tokens[i].split(':')
      if (key === 'model') model = value || ''
      else if (key === 'product') product = value || ''
      else if (key === 'transport_id') transport_id = value || null
    }
    devices.push({ serial, state, model, product, transport_id })
  }
  return devices
}

// ── ADB file operations (Android file browser) ──
// Paths are quoted with shq before going through the on-device shell;
// pull/push take paths as argv entries, so no quoting is needed there.

function shq(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

function runAdbShell(serial: string, quotedParts: string[]): Promise<string> {
  return runAdb(['-s', serial, 'shell', ...quotedParts])
}

function parseLsEntries(output: string): FileEntry[] {
  const entries: FileEntry[] = []
  for (const line of output.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('total ')) continue
    const tokens = trimmed.split(/\s+/)
    if (tokens.length < 7) continue
    const perms = tokens[0]
    const is_dir = perms.startsWith('d')
    const nameStart = tokens[5].includes('-') ? 7 : 8
    if (tokens.length <= nameStart) continue
    let name = tokens.slice(nameStart).join(' ')
    if (perms.startsWith('l')) {
      const arrow = name.indexOf(' -> ')
      if (arrow >= 0) name = name.slice(0, arrow)
    }
    if (name === '.' || name === '..') continue
    entries.push({
      name,
      is_dir,
      size: Number(tokens[4]) || 0,
      modified: tokens.slice(5, nameStart).join(' '),
      permissions: perms,
    })
  }
  return entries
}

// Strictly read-only query of the user's default adb server (5037) over the raw
// wire protocol. We never shell out to the adb binary here: a client-version
// mismatch would make `adb` kill and restart the user's server.
function query5037Devices(): Promise<AdbDevice[]> {
  return new Promise<AdbDevice[]>((resolve) => {
    let done = false
    const finish = (devices: AdbDevice[]): void => {
      if (done) return
      done = true
      clearTimeout(timer)
      sock.destroy()
      resolve(devices)
    }
    const sock = net.connect(5037, '127.0.0.1')
    const timer = setTimeout(() => finish([]), 1200)
    let buf = Buffer.alloc(0)
    sock.once('connect', () => {
      const msg = Buffer.from('host:devices-l')
      const head = Buffer.from(msg.length.toString(16).padStart(4, '0'))
      sock.write(Buffer.concat([head, msg]))
    })
    sock.on('data', (chunk: Buffer) => {
      buf = Buffer.concat([buf, chunk])
      if (buf.length < 8) return
      const status = buf.subarray(0, 4).toString()
      if (status !== 'OKAY') return finish([])
      const len = parseInt(buf.subarray(4, 8).toString(), 16)
      if (Number.isNaN(len) || buf.length < 8 + len) return
      finish(parseAdbDevices(buf.subarray(8, 8 + len).toString()))
    })
    sock.once('error', () => finish([]))
    sock.once('close', () => finish([]))
  })
}

function isUsbSerial(serial: string): boolean {
  return !serial.startsWith('emulator-') && !serial.includes(':')
}

function createWindow(): void {
  const isWindows = process.platform === 'win32'
  const isLinux = process.platform === 'linux'
  const iconPath = isWindows
    ? path.join(__dirname, 'icons/icon.ico')
    : path.join(__dirname, 'icons/icon.png')

  // Windows/macOS (VS Code approach): keep the native frame so resize edges, Aero
  // snap and maximize/restore stay native; the titlebar is hidden and rendered by
  // our own HTML, made draggable via CSS `-webkit-app-region: drag`.
  // Linux (VS Code approach): frame:false + titleBarStyle:'hidden' + opaque
  // theme bg. Since Chromium now defaults to Wayland on Linux, frameless windows
  // get GTK drop shadows + extended resize boundaries; hasShadow:false removes
  // the shadow/decorations (the white border) while native resize edges remain.
  // No titleBarOverlay on Linux: it would draw native window controls over our
  // own titlebar buttons (WCO), and our buttons are already fully functional.
  const nativeFrameOpts = isLinux
    ? {
        frame: false,
        titleBarStyle: 'hidden' as const,
        backgroundColor: '#1e1e1e',
        hasShadow: false,
      }
    : { titleBarStyle: 'hidden' as const, backgroundColor: '#1e1e1e' }

  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    title: 'AidTerm',
    ...nativeFrameOpts,
    icon: (isWindows || isLinux) ? iconPath : undefined,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    show: false,
  })

  // macOS: keep the native frame/resize but hide native traffic lights since we
  // render our own in TitleBar.vue.
  if (process.platform === 'darwin') {
    mainWindow.setWindowButtonVisibility(false)
  }

  if (isDev) {
    mainWindow.loadURL('http://localhost:3000')
  } else {
    mainWindow.loadFile(path.join(process.resourcesPath, 'dist', 'index.html'))
  }

  mainWindow.once('ready-to-show', () => { mainWindow?.show() })
  mainWindow.on('resize', () => emitToRenderer('window:resized', {}))
  mainWindow.on('closed', () => {
    cleanupAllSessions()
    cast.closeAllPushes()
    mainWindow = null
  })

  // DevTools shortcuts: F12, Ctrl+Shift+I
  mainWindow.webContents.on('before-input-event', (_, input) => {
    if (input.type !== 'keyDown') return
    const toggleDevTools = () => {
      if (!mainWindow) return
      const wc = mainWindow.webContents
      if (wc.isDevToolsOpened()) wc.closeDevTools()
      else wc.openDevTools()
    }
    if (input.key === 'F12' && !input.control && !input.shift && !input.alt && !input.meta) {
      toggleDevTools()
    }
    const isCmdLike = input.control || (process.platform === 'darwin' && input.meta)
    if (input.key === 'I' && isCmdLike && input.shift && !input.alt) {
      toggleDevTools()
    }
  })
}

function killPty(term: { pid: number; kill: (signal?: string) => void }): void {
  try {
    if (process.platform === 'win32') {
      // Windows: ConPTY kills the whole process tree.
      term.kill()
      return
    }
    // Unix: the shell is the session/process-group leader (via setsid), so HUP the
    // entire group to give the shell a chance to clean up vim/top/background jobs,
    // then force-kill the group after a short grace period as a fallback.
    const pid = term.pid
    try { process.kill(-pid, 'SIGHUP') } catch {}
    setTimeout(() => { try { process.kill(-pid, 'SIGKILL') } catch {} }, 500)
  } catch {}
}

function cleanupAllSessions(): void {
  for (const [, pty] of ptySessions) { killPty(pty) }
  ptySessions.clear()
  for (const [, ssh] of sshSessions) { try { ssh.conn.end() } catch {} }
  sshSessions.clear()
  for (const [, sp] of serialSessions) { try { sp.port.close() } catch {} }
  serialSessions.clear()
  for (const [, sftp] of sftpConnections) { try { sftp.end() } catch {} }
  sftpConnections.clear()
  for (const [, tunnel] of tunnelMap) { try { tunnel.server?.close(); tunnel.conn?.end() } catch {} }
  tunnelMap.clear()
}

// ══════════════════════════════════════════════════════════════
//  Session store (JSON file persistence, simple encryption)
// ══════════════════════════════════════════════════════════════

function sessionStorePath(): string {
  return path.join(getAppDataDir(), 'sessions.json')
}

function encryptPassword(pw: string): string {
  const key = crypto.scryptSync('aidterm-session-key', 'salt-term-v1', 32)
  const iv = crypto.randomBytes(16)
  const cipher = crypto.createCipheriv('aes-256-cbc', key, iv)
  let encrypted = cipher.update(pw, 'utf8', 'hex')
  encrypted += cipher.final('hex')
  return `enc:${iv.toString('hex')}:${encrypted}`
}

function decryptPassword(enc: string): string {
  if (!enc.startsWith('enc:')) return enc
  const parts = enc.split(':')
  if (parts.length < 3) return enc
  const key = crypto.scryptSync('aidterm-session-key', 'salt-term-v1', 32)
  const iv = Buffer.from(parts[1], 'hex')
  const decipher = crypto.createDecipheriv('aes-256-cbc', key, iv)
  let decrypted = decipher.update(parts.slice(2).join(':'), 'hex', 'utf8')
  decrypted += decipher.final('utf8')
  return decrypted
}

function decodeCmdOutput(buf: Buffer): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(buf)
  } catch {
    try {
      return new TextDecoder('gbk').decode(buf)
    } catch {
      return buf.toString('utf8')
    }
  }
}

function loadSessionStore(): SessionStoreData {
  const p = sessionStorePath()
  if (!fs.existsSync(p)) return { sessions: [], groups: [] }
  try {
    const raw = fs.readFileSync(p, 'utf8')
    const data = JSON.parse(raw) as SessionStoreData
    for (const s of data.sessions) {
      if (s.password && s.password.startsWith('enc:')) {
        s.password = decryptPassword(s.password)
      }
    }
    return data
  } catch { return { sessions: [], groups: [] } }
}

function saveSessionStore(data: SessionStoreData): void {
  const dir = getAppDataDir()
  fs.mkdirSync(dir, { recursive: true })
  const clone: SessionStoreData = JSON.parse(JSON.stringify(data))
  for (const s of clone.sessions) {
    if (s.password && !s.password.startsWith('enc:') && s.password.length > 0) {
      s.password = encryptPassword(s.password)
    }
  }
  fs.writeFileSync(sessionStorePath(), JSON.stringify(clone, null, 2))
}

// ══════════════════════════════════════════════════════════════
//  Keychain (file-based key storage)
// ══════════════════════════════════════════════════════════════

function loadKeyIndex(): void {
  if (!fs.existsSync(keyIndexPath)) return
  try {
    const data = JSON.parse(fs.readFileSync(keyIndexPath, 'utf8')) as Record<string, KeyInfo>
    for (const [k, v] of Object.entries(data)) keyIndex.set(k, v)
  } catch {}
}

function saveKeyIndex(): void {
  const obj: Record<string, KeyInfo> = {}
  for (const [k, v] of keyIndex) obj[k] = v
  fs.writeFileSync(keyIndexPath, JSON.stringify(obj, null, 2))
}

function runCmd(cmd: string, args: string[]): string {
  const { spawnSync } = require('child_process') as typeof import('child_process')
  const res = spawnSync(cmd, args, { encoding: 'utf8', timeout: 30000 })
  if (res.error) throw res.error
  if (res.status !== 0) {
    const detail = (res.stderr || res.stdout || '').toString().trim()
    throw new Error(`${cmd} failed (exit ${res.status}): ${detail}`)
  }
  return (res.stdout || '').toString()
}

const SHELL_CONTEXT_MENU_LABEL = '在 AidTerm 中打开'
const SHELL_CONTEXT_MENU_KEYS: Array<[string, string]> = [
  ['HKCU\\Software\\Classes\\Directory\\shell\\AidTerm', '%1'],
  ['HKCU\\Software\\Classes\\Directory\\Background\\shell\\AidTerm', '%V'],
  ['HKCU\\Software\\Classes\\DesktopBackground\\Shell\\AidTerm', '%V'],
]

function shellContextMenuCommand(target: string): string {
  const parts = [`"${process.execPath}"`]
  if (!app.isPackaged) parts.push(`"${path.resolve(process.argv[1])}"`)
  parts.push(`--cwd "${target}"`)
  return parts.join(' ')
}

function runRegistry(args: string[], allowMissing = false): string {
  const { spawnSync } = require('child_process') as typeof import('child_process')
  const result = spawnSync('reg.exe', args, { encoding: 'utf8', windowsHide: true })
  if (result.error) throw result.error
  if (result.status !== 0 && !allowMissing) {
    const detail = (result.stderr || result.stdout || '').toString().trim()
    throw new Error(`reg.exe failed (exit ${result.status}): ${detail}`)
  }
  return (result.stdout || '').toString()
}

function registryKeyExists(key: string): boolean {
  const { spawnSync } = require('child_process') as typeof import('child_process')
  const result = spawnSync('reg.exe', ['query', key], { windowsHide: true })
  if (result.error) throw result.error
  return result.status === 0
}

function shellContextMenuEnabled(): boolean {
  if (process.platform !== 'win32') return false
  return SHELL_CONTEXT_MENU_KEYS.every(([key]) => registryKeyExists(`${key}\\command`))
}

function setShellContextMenuEnabled(enabled: boolean): void {
  if (process.platform !== 'win32') {
    if (enabled) throw new Error('Explorer context menu is only supported on Windows')
    return
  }

  if (!enabled) {
    for (const [key] of SHELL_CONTEXT_MENU_KEYS) runRegistry(['delete', key, '/f'], true)
    return
  }

  const icon = `"${process.execPath}",0`
  try {
    for (const [key, target] of SHELL_CONTEXT_MENU_KEYS) {
      runRegistry(['add', key, '/ve', '/d', SHELL_CONTEXT_MENU_LABEL, '/f'])
      runRegistry(['add', key, '/v', 'Icon', '/d', icon, '/f'])
      runRegistry(['add', `${key}\\command`, '/ve', '/d', shellContextMenuCommand(target), '/f'])
    }
  } catch (error) {
    for (const [key] of SHELL_CONTEXT_MENU_KEYS) runRegistry(['delete', key, '/f'], true)
    throw error
  }
}

const USER_ENVIRONMENT_KEY = 'HKCU\\Software\\Environment'
const AIDTERM_ENVIRONMENT_KEY = 'HKCU\\Software\\AidTerm'

function aidtermExecutableDirectory(): string {
  if (!app.isPackaged) throw new Error('PATH integration is only available in packaged builds')
  return path.dirname(app.getPath('exe'))
}

function normalizeWindowsPath(value: string): string {
  return value.trim().replace(/[\\/]+$/, '').toLowerCase()
}

function pathEnvironmentEnabled(): boolean {
  return process.platform === 'win32' && registryKeyExists(`${AIDTERM_ENVIRONMENT_KEY}\\EnvironmentPath`)
}

function setPathEnvironmentEnabled(enabled: boolean): void {
  if (process.platform !== 'win32') {
    if (enabled) throw new Error('Adding AidTerm to PATH is only supported on Windows')
    return
  }
  const directory = aidtermExecutableDirectory()
  const target = normalizeWindowsPath(directory)
  const current = process.env.Path || process.env.PATH || ''
  const entries = current
    .split(';')
    .filter((entry) => entry.trim() && normalizeWindowsPath(entry) !== target)
  if (enabled) entries.push(directory)
  const nextPath = entries.join(';')
  runRegistry(['add', USER_ENVIRONMENT_KEY, '/v', 'Path', '/t', 'REG_EXPAND_SZ', '/d', nextPath, '/f'])
  process.env.Path = nextPath
  process.env.PATH = nextPath
  if (enabled) {
    runRegistry(['add', AIDTERM_ENVIRONMENT_KEY, '/v', 'EnvironmentPath', '/d', directory, '/f'])
  } else {
    runRegistry(['delete', AIDTERM_ENVIRONMENT_KEY, '/v', 'EnvironmentPath', '/f'], true)
  }
}

// ══════════════════════════════════════════════════════════════
//  Known hosts
// ══════════════════════════════════════════════════════════════

function loadKnownHosts(): KnownHostEntry[] {
  if (!fs.existsSync(knownHostsPath)) return []
  const content = fs.readFileSync(knownHostsPath, 'utf8')
  const entries: KnownHostEntry[] = []
  for (const line of content.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const parts = trimmed.split(/\s+/)
    if (parts.length < 3) continue
    if (parts[0].startsWith('@')) continue
    entries.push({
      host: parts[0],
      key_type: parts[1],
      fingerprint: `${parts[1]} ${parts[2].slice(0, 16)}...${parts[2].slice(-8)}`,
      line: trimmed,
    })
  }
  return entries
}

function addKnownHost(host: string, keyType: string, key: string): void {
  const line = `${host} ${keyType} ${key}\n`
  const sshDir = path.dirname(knownHostsPath)
  fs.mkdirSync(sshDir, { recursive: true })
  fs.appendFileSync(knownHostsPath, line)
}

function removeKnownHost(host: string, keyType: string): void {
  if (!fs.existsSync(knownHostsPath)) return
  const lines = fs.readFileSync(knownHostsPath, 'utf8').split('\n')
  const filtered = lines.filter(l => {
    const t = l.trim()
    if (!t || t.startsWith('#')) return true
    const parts = t.split(/\s+/)
    if (parts.length < 3) return true
    if (parts[0].startsWith('@')) return true
    return !(parts[0].toLowerCase() === host.toLowerCase()
      && parts[1].toLowerCase() === keyType.toLowerCase())
  })
  fs.writeFileSync(knownHostsPath, filtered.join('\n'))
}

// ══════════════════════════════════════════════════════════════
//  Main
// ══════════════════════════════════════════════════════════════

function singleInstanceConfigPath(): string {
  return path.join(app.getPath('userData'), 'aidterm-config.json')
}

function readSingleInstanceSetting(): boolean {
  try {
    const data = JSON.parse(fs.readFileSync(singleInstanceConfigPath(), 'utf8'))
    return data.single_instance === true
  } catch {
    return false
  }
}

function writeSingleInstanceSetting(enabled: boolean): void {
  const file = singleInstanceConfigPath()
  fs.mkdirSync(path.dirname(file), { recursive: true })
  fs.writeFileSync(file, JSON.stringify({ single_instance: enabled }, null, 2))
}

function setupApp(): void {
  app.whenReady().then(() => {
    loadNativeModules()

    appDataDir = path.join(app.getPath('userData'), 'aidterm-data')
    keysDir = path.join(appDataDir, 'keys')
    keyIndexPath = path.join(appDataDir, 'keys_index.json')
    knownHostsPath = path.join(os.homedir(), '.ssh', 'known_hosts')
    fs.mkdirSync(appDataDir, { recursive: true })
    fs.mkdirSync(keysDir, { recursive: true })
    loadKeyIndex()

    createWindow()
    registerIpcHandlers()

    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow()
    })
  })

  app.on('window-all-closed', () => {
    cleanupAllSessions()
    if (process.platform !== 'darwin') app.quit()
  })
}

if (readSingleInstanceSetting()) {
  if (!app.requestSingleInstanceLock()) {
    app.quit()
  } else {
    app.on('second-instance', (_event, argv) => {
      if (mainWindow) {
        if (mainWindow.isMinimized()) mainWindow.restore()
        mainWindow.show()
        mainWindow.focus()
      }
      const args = argv.slice(app.isPackaged ? 1 : 2).filter((a) => !a.startsWith('-psn_0_'))
      mainWindow?.webContents.send('cli-args', args)
    })
    setupApp()
  }
} else {
  setupApp()
}

// ── Update helpers ──

const UPDATE_REPO = 'JerryJian/AidTerm'
const UPDATE_LATEST_API = `https://api.github.com/repos/${UPDATE_REPO}/releases/latest`

function compareVersions(a: string, b: string): number {
  const pa = a.replace(/^v/, '').split(/[^\d]+/).map(n => parseInt(n, 10) || 0)
  const pb = b.replace(/^v/, '').split(/[^\d]+/).map(n => parseInt(n, 10) || 0)
  const len = Math.max(pa.length, pb.length)
  for (let i = 0; i < len; i++) {
    const x = pa[i] || 0
    const y = pb[i] || 0
    if (x !== y) return x - y
  }
  return 0
}

function pickElectronAsset(assets: Array<{ name: string; browser_download_url: string }>): { assetName: string | null; assetUrl: string | null } {
  const os = process.platform // win32 | darwin | linux
  const arch = process.arch // x64 | arm64
  const suffix = os === 'win32' ? '.exe' : os === 'darwin' ? '.dmg' : '.AppImage'
  const archKeys = arch === 'x64' ? ['x64'] : arch === 'arm64' ? ['arm64'] : [arch]
  const matches = (name: string): boolean => {
    const n = name.toLowerCase()
    if (!n.includes('electron')) return false
    if (!n.endsWith(suffix.toLowerCase())) return false
    if (os === 'win32' && !n.includes('setup')) return false
    return true
  }
  for (const key of archKeys) {
    const found = assets.find(a => matches(a.name) && a.name.toLowerCase().includes(key))
    if (found) return { assetName: found.name, assetUrl: found.browser_download_url }
  }
  const fallback = assets.find(a => matches(a.name))
  return fallback ? { assetName: fallback.name, assetUrl: fallback.browser_download_url } : { assetName: null, assetUrl: null }
}

// Electron builds NSIS only; still probe the registry so a future MSI build is handled.
function getInstallerType(): string {
  if (process.platform !== 'win32') return 'unknown'
  const { spawnSync } = require('child_process') as typeof import('child_process')
  const base = 'Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall'
  const keys = [`HKLM\\${base}\\com.aidterm.app`, `HKCU\\${base}\\com.aidterm.app`]
  try {
    for (const key of keys) {
      const res = spawnSync('reg', ['query', key], { encoding: 'utf8', windowsHide: true })
      if (res.status === 0 && /WindowsInstaller/i.test(res.stdout || '')) return 'msi'
    }
    return 'nsis'
  } catch {
    return 'nsis'
  }
}

async function downloadToFile(url: string, dest: string): Promise<string> {
  const resp = await fetch(url, { headers: { 'User-Agent': 'AidTerm' } })
  if (!resp.ok || !resp.body) throw new Error(`Download failed: HTTP ${resp.status}`)
  const total = Number(resp.headers.get('content-length')) || 0
  const reader = resp.body.getReader()
  const out = fs.createWriteStream(dest)
  let received = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      received += value.length
      out.write(Buffer.from(value))
      if (total > 0) emitToRenderer('update-progress', { received, total })
    }
    await new Promise<void>((resolve, reject) => {
      out.end((err?: Error | null) => (err ? reject(err) : resolve()))
    })
  } finally {
    emitToRenderer('update-progress', { received: total || received, total })
  }
  return dest
}

// ── IPC Handlers ──

function registerIpcHandlers(): void {
  // ═══ Window ═══
  ipcMain.handle('window:isFullscreen', () => mainWindow?.isFullScreen() ?? false)
  ipcMain.handle('window:setFullscreen', (_, args: WindowSetFullscreenArgs) => mainWindow?.setFullScreen(args.fullscreen))
  ipcMain.handle('window:isMaximized', () => mainWindow?.isMaximized() ?? false)
  ipcMain.handle('window:minimize', () => mainWindow?.minimize())
  ipcMain.handle('window:maximize', () => mainWindow?.maximize())
  ipcMain.handle('window:unmaximize', () => mainWindow?.unmaximize())
  ipcMain.handle('window:toggleMaximize', () => {
    if (mainWindow?.isMaximized()) mainWindow.unmaximize()
    else mainWindow?.maximize()
  })
  ipcMain.handle('window:startDragging', () => {})
  ipcMain.handle('window:show', () => mainWindow?.show())
  ipcMain.handle('window:hide', () => mainWindow?.hide())
  ipcMain.handle('window:close', () => mainWindow?.close())
  ipcMain.handle('window:openDevtools', () => mainWindow?.webContents.openDevTools())

  // ═══ Platform ═══
  ipcMain.handle('get_platform', () => {
    switch (process.platform) {
      case 'win32': return 'windows'
      case 'darwin': return 'macos'
      default: return 'linux'
    }
  })

  // ═══ Dialog ═══
  ipcMain.handle('dialog:open', async (_, opts: OpenDialogOpts | undefined) => {
    if (!mainWindow) return null
    if (opts?.directory) {
      const result = await dialog.showOpenDialog(mainWindow, { properties: ['openDirectory'], title: opts?.title })
      return result.canceled ? null : result.filePaths
    }
    const result = await dialog.showOpenDialog(mainWindow, {
      title: opts?.title,
      filters: opts?.filters?.map((f: DialogFilter) => ({ name: f.name, extensions: f.extensions })),
      properties: opts?.multiple ? ['openFile', 'multiSelections'] : ['openFile'],
    })
    if (result.canceled) return null
    return opts?.multiple ? result.filePaths : result.filePaths[0]
  })

  ipcMain.handle('dialog:save', async (_, opts: SaveDialogOpts | undefined) => {
    if (!mainWindow) return null
    const result = await dialog.showSaveDialog(mainWindow, {
      title: opts?.title,
      filters: opts?.filters?.map((f: DialogFilter) => ({ name: f.name, extensions: f.extensions })),
      defaultPath: opts?.defaultPath,
    })
    return result.canceled ? null : result.filePath
  })

  // ═══ Clipboard ═══
  ipcMain.handle('clipboard:write', (_, args: ClipboardWriteArgs) => { clipboard.writeText(args.text) })
  ipcMain.handle('clipboard:read', () => clipboard.readText())

  // ═══ File → data URL (background image etc.) ═══
  ipcMain.handle('file_to_data_url', async (_e, args: { path: string }) => {
    const data = fs.readFileSync(args.path)
    const ext = path.extname(args.path).slice(1).toLowerCase()
    const mime = ext === 'png' ? 'image/png'
      : ext === 'jpg' || ext === 'jpeg' ? 'image/jpeg'
      : ext === 'gif' ? 'image/gif'
      : ext === 'webp' ? 'image/webp'
      : ext === 'svg' ? 'image/svg+xml'
      : 'application/octet-stream'
    return `data:${mime};base64,${data.toString('base64')}`
  })

  // ══════════════════════════════════════════════════════
  //  Unified connection lifecycle (connection_*)
  //
  //  All session types are created through `connection_create`, which returns
  //  { id, capabilities }. write/resize/kill dispatch across the underlying
  //  maps exactly like the old write_terminal/resize_terminal/kill_terminal.
  // ══════════════════════════════════════════════════════
  ipcMain.handle('wsl_list_distros', (): Promise<string[]> => {
    const { execFile } = require('child_process') as typeof import('child_process')
    return new Promise((resolve) => {
      execFile('wsl.exe', ['-l', '-q'], { timeout: 10000, encoding: 'buffer' }, (err: Error | null, stdout: string | Buffer) => {
        if (err) { resolve([]); return }
        const buf = stdout as Buffer
        const isUtf8Bom = buf.length >= 3 && buf[0] === 0xEF && buf[1] === 0xBB && buf[2] === 0xBF
        const isUtf16le = (buf.length >= 2 && buf[0] === 0xFF && buf[1] === 0xFE) || (!isUtf8Bom && buf.includes(0x00))
        const text = isUtf16le
          ? buf.toString('utf16le', buf.length >= 2 && buf[0] === 0xFF && buf[1] === 0xFE ? 2 : 0)
          : buf.toString('utf8').replace(/^\uFEFF/, '')
        const distros = text.split(/\r?\n/).map(l => l.trim()).filter(Boolean)
        resolve(distros)
      })
    })
  })

  ipcMain.handle('connection_write', (_, args: WriteTerminalArgs) => {
    const { sessionId, data } = args
    const term = ptySessions.get(sessionId)
    if (term) { term.write(data); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { ssh.writeCh!.write(data); return }
    const sp = serialSessions.get(sessionId)
    if (sp) { sp.port.write(data); return }
    throw new Error(`Session ${sessionId} not found`)
  })

  ipcMain.handle('connection_resize', (_, args: ResizeTerminalArgs) => {
    const { sessionId, rows, cols } = args
    const term = ptySessions.get(sessionId)
    if (term) { term.resize(cols, rows); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { ssh.writeCh!.setWindow(rows, cols, rows, cols); return }
  })

  ipcMain.handle('connection_kill', (_, args: KillTerminalArgs) => {
    const { sessionId } = args
    const term = ptySessions.get(sessionId)
    if (term) { killPty(term); ptySessions.delete(sessionId); wslSessions.delete(sessionId); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { try { ssh.conn.end() } catch {}; sshSessions.delete(sessionId); monitorPrev.delete(sessionId); return }
    const sp = serialSessions.get(sessionId)
    if (sp) { try { sp.port.close() } catch {}; serialSessions.delete(sessionId); return }
  })

  // ── Shared session creators (used by `connection_create`) ──

  function spawnLocalPty(opts: { rows: number; cols: number; shell?: string | null; cwd?: string | null; args?: string[] }): string {
    if (!ptyModule) throw new Error('node-pty not installed')

    const id = crypto.randomUUID()
    const shellCmd = opts.shell || (process.platform === 'win32'
      ? (process.env.ComSpec || 'cmd.exe')
      : (process.env.SHELL || (process.platform === 'darwin' ? 'zsh' : 'bash')))

    const termEnv: NodeJS.ProcessEnv = { ...process.env, TERM: 'xterm-256color' }
    if (!process.env.LANG && !process.env.LC_ALL && !process.env.LC_CTYPE) termEnv.LANG = 'C.UTF-8'

    // No explicit directory: inherit the process working directory. The app
    // points this at `--cwd` (right-click menu) or the user's home (double-click)
    // at startup, so tabs opened without an explicit directory land in the right place.
    const cwd = opts.cwd?.trim() || process.cwd()
    if (!fs.existsSync(cwd) || !fs.statSync(cwd).isDirectory()) {
      throw new Error(`Working directory does not exist: ${cwd}`)
    }

    const term = ptyModule.spawn(shellCmd, opts.args || [], {
      name: 'xterm-256color',
      cols: opts.cols || 80,
      rows: opts.rows || 24,
      cwd,
      env: termEnv,
    })

    ptySessions.set(id, term)

    term.onData((data: string) => {
      emitToRenderer('terminal-output', { session_id: id, data })
    })

    term.onExit(({ exitCode }: { exitCode: number }) => {
      emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: exitCode !== 0 ? `Exit code: ${exitCode}` : undefined })
      ptySessions.delete(id)
    })

    emitToRenderer('session-status', { session_id: id, status: 'connected' })
    return id
  }

  // ══════════════════════════════════════════════════════
  //  SSH (ssh2)
  // ══════════════════════════════════════════════════════
  function connectSshInternal(args: SshConnectArgs): string {
    if (!ssh2Module) throw new Error('ssh2 not installed')

    const { host, port, username, password, privateKeyPath, proxyId, rows, cols } = args
    const id = crypto.randomUUID()
    const conn = new ssh2Module.Client()

    const state: SshSession = { conn, stream: null, writeCh: null, resizeCh: null }
    sshSessions.set(id, state)

    conn.on('ready', () => {
      conn.shell({ term: 'xterm-256color', cols: cols || 80, rows: rows || 24 }, (err: Error | undefined, stream: ClientChannel) => {
        if (err) {
          emitToRenderer('terminal-output', { session_id: id, data: `\r\n[SSH Error: ${err.message}]\r\n` })
          emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: err.message })
          sshSessions.delete(id)
          return
        }
        state.stream = stream
        state.writeCh = stream
        state.resizeCh = stream

        stream.on('data', (data: Buffer) => {
          emitToRenderer('terminal-output', { session_id: id, data: data.toString('utf8') })
        })

        stream.stderr.on('data', (data: Buffer) => {
          emitToRenderer('terminal-output', { session_id: id, data: data.toString('utf8') })
        })

        stream.on('close', () => {
          emitToRenderer('session-status', { session_id: id, status: 'disconnected' })
          sshSessions.delete(id)
        })

        emitToRenderer('session-status', { session_id: id, status: 'connected' })
      })
    })

    conn.on('error', (err: Error) => {
      emitToRenderer('terminal-output', { session_id: id, data: `\r\n[SSH Error: ${err.message}]\r\n` })
      emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: err.message })
      sshSessions.delete(id)
    })

    const connectOpts: SshConnectConfig = { host, port: port || 22, username }
    if (privateKeyPath && fs.existsSync(privateKeyPath)) {
      connectOpts.privateKey = fs.readFileSync(privateKeyPath)
    } else if (password) {
      connectOpts.password = password
    }

    // Store connection params for exec-only reconnection
    state.connectOpts = { ...connectOpts }

    if (proxyId) {
      const proxy = proxyConfigs.find(p => p.id === proxyId)
      if (proxy) {
        const sock = new net.Socket()
        ;(connectOpts as Record<string, unknown>).sock = sock
        sock.connect(proxy.port, proxy.host, () => {
          conn.connect(connectOpts)
        })
        sock.on('error', (err: Error) => {
          emitToRenderer('terminal-output', { session_id: id, data: `\r\n[Proxy Error: ${err.message}]\r\n` })
          emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: err.message })
          sshSessions.delete(id)
        })
      } else {
        conn.connect(connectOpts)
      }
    } else {
      conn.connect(connectOpts)
    }

    return id
  }

  // Kept: used for one-shot SSH connections (TunnelManager). Terminal sessions
  // go through `connection_create` instead.
  ipcMain.handle('ssh_connect', (_, args: SshConnectArgs) => {
    return connectSshInternal(args)
  })

  // ══════════════════════════════════════════════════════
  //  Telnet (net.Socket)
  // ══════════════════════════════════════════════════════
  function connectTelnet(host: string, port: number): string {
    const id = crypto.randomUUID()
    const sock = new net.Socket()

    sock.connect(port || 23, host, () => {
      emitToRenderer('session-status', { session_id: id, status: 'connected' })
    })

    sock.on('data', (data: Buffer) => {
      emitToRenderer('terminal-output', { session_id: id, data: data.toString('latin1') })
    })

    sock.on('close', () => {
      emitToRenderer('session-status', { session_id: id, status: 'disconnected' })
    })

    sock.on('error', (err: Error) => {
      emitToRenderer('terminal-output', { session_id: id, data: `\r\n[Telnet Error: ${err.message}]\r\n` })
      emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: err.message })
    })

    serialSessions.set(id, { port: { write: (d: string) => { sock.write(d) }, close: () => { sock.destroy() } } })
    return id
  }

  // ══════════════════════════════════════════════════════
  //  Serial (serialport)
  // ══════════════════════════════════════════════════════
  function connectSerial(args: SerialConnectArgs): string {
    if (!SerialPortClass) throw new Error('serialport not installed')

    const { portName, baudRate, dataBits, stopBits, parity, flowControl } = args
    const id = crypto.randomUUID()

    const port = new SerialPortClass({
      path: portName,
      baudRate: baudRate || 9600,
      dataBits: (dataBits || 8) as 5 | 6 | 7 | 8,
      stopBits: (stopBits || 1) as 1 | 2,
      parity: (parity?.toLowerCase() || 'none') as 'none' | 'even' | 'odd',
      // flowControl not in serialport types; cast to bypass
    })

    port.on('open', () => {
      emitToRenderer('session-status', { session_id: id, status: 'connected' })
    })

    port.on('data', (data: Buffer) => {
      emitToRenderer('terminal-output', { session_id: id, data: data.toString('utf8') })
    })

    port.on('close', () => {
      emitToRenderer('session-status', { session_id: id, status: 'disconnected' })
      serialSessions.delete(id)
    })

    port.on('error', (err: Error) => {
      emitToRenderer('terminal-output', { session_id: id, data: `\r\n[Serial Error: ${err.message}]\r\n` })
      emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: err.message })
      serialSessions.delete(id)
    })

    serialSessions.set(id, { port })
    return id
  }

  ipcMain.handle('serial_list_ports', async () => {
    if (!SerialPortClass) return []
    try {
      const ports = await SerialPortClass.list()
      return ports.map((p: { path: string }): SerialPortInfo => ({ port_name: p.path }))
    } catch { return [] }
  })

  // ══════════════════════════════════════════════════════
  //  ADB (every source talks to the shared 5037 port)
  // ══════════════════════════════════════════════════════
  function connectAdb(serial: string, rows: number, cols: number): string {
    if (!ptyModule) throw new Error('node-pty not installed')

    const id = crypto.randomUUID()
    const { path: adb, port } = resolveAdb()

    const term = ptyModule.spawn(adb, ['-P', port, '-s', serial, 'shell'], {
      name: 'xterm-256color',
      cols: cols || 80,
      rows: rows || 24,
      cwd: process.cwd(),
      env: { ...process.env, TERM: 'xterm-256color' },
    })

    ptySessions.set(id, term)

    term.onData((data: string) => {
      emitToRenderer('terminal-output', { session_id: id, data })
    })

    term.onExit(({ exitCode }: { exitCode: number }) => {
      emitToRenderer('session-status', { session_id: id, status: 'disconnected', error: exitCode !== 0 ? `Exit code: ${exitCode}` : undefined })
      ptySessions.delete(id)
    })

    emitToRenderer('session-status', { session_id: id, status: 'connected' })
    return id
  }

  // ── Unified connection factory ──

  ipcMain.handle('connection_create', async (_, args: ConnectionCreateArgs): Promise<ConnectionHandle> => {
    const { config, rows, cols } = args
    switch (config.type) {
      case 'local': {
        const id = spawnLocalPty({ rows, cols, shell: config.shell ?? null, cwd: config.working_dir ?? null })
        return { id, capabilities: ['file', 'monitor'] }
      }
      case 'wsl': {
        const args: string[] = []
        if (config.distro) args.push('-d', config.distro)
        if (config.working_dir?.trim()) args.push('--cd', config.working_dir.trim())
        const id = spawnLocalPty({
          rows,
          cols,
          shell: 'wsl.exe',
          args,
        })
        wslSessions.set(id, config.distro ?? '')
        return { id, capabilities: ['file', 'monitor'] }
      }
      case 'ssh': {
        const id = connectSshInternal({
          host: config.host,
          port: config.port,
          username: config.username,
          password: config.password,
          privateKeyPath: config.private_key_path ?? null,
          proxyId: config.proxy_id ?? null,
          agentForwarding: config.agent_forwarding ?? false,
          x11Forwarding: config.x11_forwarding ?? false,
          rows,
          cols,
        })
        return { id, capabilities: ['file', 'tunnel', 'exec', 'zmodem'] }
      }
      case 'telnet': {
        const id = connectTelnet(config.host, config.port)
        return { id, capabilities: [] }
      }
      case 'serial': {
        const id = connectSerial({
          portName: config.port_name,
          baudRate: config.baud_rate,
          dataBits: config.data_bits,
          stopBits: config.stop_bits,
          parity: config.parity,
          flowControl: config.flow_control,
        })
        return { id, capabilities: [] }
      }
      case 'adb': {
        await ensureAdbProbed()
        const id = connectAdb(config.serial, rows, cols)
        return { id, capabilities: ['file', 'cast'] }
      }
    }
  })

  ipcMain.handle('adb_status', async (): Promise<AdbStatus> => adbStatus())

  ipcMain.handle('adb_list_devices', async (): Promise<AdbDevice[]> => {
    try {
      await refreshRunningAdb()
      const out = await runAdb(['devices', '-l'])
      return parseAdbDevices(out)
    } catch (e) {
      console.warn('[electron] adb devices failed:', e)
      return []
    }
  })

  ipcMain.handle('adb_kill_server', async (): Promise<void> => {
    try {
      await ensureAdbProbed()
      const { source } = resolveAdb()
      // Only the bundled adb's server is ever stopped; an external/system adb's
      // server is shared with (and may be run by) the user's other adb tools.
      if (source !== 'bundled') return
      await runAdb(['kill-server'])
    } catch (e) {
      console.warn('[electron] adb kill-server failed:', e)
    }
  })

  // ══════════════════════════════════════════════════════
  //  Screen casting (scrcpy-server standalone) — Node port of cast.rs
  //  Mirrors the Tauri `cast_start`/`cast_frame`/`cast_stop`/`cast_input` IPC.
  // ══════════════════════════════════════════════════════
  ipcMain.handle(
    'cast_start',
    async (_, args: { serial: string; maxSize?: number }): Promise<{ port: number; width: number | null; height: number | null }> => {
      await ensureAdbProbed()
      const { path: adb, port } = resolveAdb()
      return cast.start(adb, port, args.serial, args.maxSize ?? 0)
    }
  )

  ipcMain.handle(
    'cast_frame',
    (_, args: { serial: string; needKey?: boolean; seenSeq?: number }): [number, boolean, string, string | null] | null => {
      return cast.frame(args.serial, args.needKey ?? false)
    }
  )

  ipcMain.handle('cast_stop', (_, args: { serial: string }): void => {
    cast.stop(args.serial)
  })

  ipcMain.handle('cast_input', async (_, args: { serial: string; cmd: string }): Promise<void> => {
    await ensureAdbProbed()
    const { path: adb, port } = resolveAdb()
    await cast.input(adb, port, args.serial, args.cmd)
  })

  // Push channel for the frame stream: the renderer opens a MessageChannel,
  // `cast.openPush` registers the main-side port as the sink, and every demuxed
  // frame is posted to the channel as a binary ArrayBuffer (no base64). The
  // frontend reads frames straight off the port instead of polling `cast_frame`.
  //
  // Note: Electron's MessagePortMain only transfers ports, not ArrayBuffers
  // (electron/electron#34905), so each frame is structured-cloned by the port —
  // still one copy, far cheaper than base64 string round-trips.
  ipcMain.on('cast_stream_port', (event, args: { serial: string }): void => {
    const { port1, port2 } = new MessageChannelMain()
    const serial = args.serial
    let closed = false
    const sink: import('./cast').PushSink = {
      post: (msg) => {
        if (closed) return
        try {
          port1.postMessage(msg)
        } catch {
          closed = true
        }
      },
      close: () => {
        closed = true
        try {
          port1.close()
        } catch {
          /* already closed */
        }
      },
    }
    cast.openPush(serial, sink)
    event.sender.postMessage(`cast-stream-port:${serial}`, null, [port2])
  })

  ipcMain.on('cast_stream_close', (_event, args: { serial: string }): void => {
    cast.closePush(args.serial)
  })

  // USB devices held by a separate adb server never show up on the one AidTerm
  // uses. Since every source shares port 5037 this normally reports nothing.
  // Report them so the UI can explain why a device is missing.
  ipcMain.handle('adb_occupied_devices', async (): Promise<string[]> => {
    try {
      const own = new Set<string>()
      try {
        const out = await runAdb(['devices', '-l'])
        parseAdbDevices(out).forEach(d => own.add(d.serial))
      } catch { /* fall through with empty own list */ }

      const occupied: string[] = []
      const userDevices = await query5037Devices()
      for (const d of userDevices) {
        if (d.state === 'device' && isUsbSerial(d.serial) && !own.has(d.serial)) {
          occupied.push(d.serial)
        }
      }
      return occupied
    } catch (e) {
      console.warn('[electron] adb occupied devices failed:', e)
      return []
    }
  })

  // ══════════════════════════════════════════════════════
  //  Unified file backend (file_*)
  //  `kind` is 'sftp' (handle = connection id) or 'adb' (handle = device serial).
  //  Mirrors the Rust `file_*` commands one-to-one.
  // ══════════════════════════════════════════════════════
  ipcMain.handle('file_connect', async (_, args: FileConnectArgs): Promise<string> => {
    const { config } = args
    if (config.type === 'adb') return config.serial
    if (config.type === 'local') return 'local'
    if (config.type === 'wsl') return config.distro ?? ''
    if (!SftpClientClass) throw new Error('ssh2-sftp-client not installed')

    const connId = crypto.randomUUID()
    const client = new SftpClientClass('sftp-' + connId)

    const opts: Record<string, unknown> = { host: config.host, port: config.port || 22, username: config.username }
    if (config.private_key_path && fs.existsSync(config.private_key_path)) {
      opts.privateKey = fs.readFileSync(config.private_key_path)
    } else if (config.password) {
      opts.password = config.password
    }

    await client.connect(opts as Parameters<SftpClient['connect']>[0])
    sftpConnections.set(connId, client)
    return connId
  })

  ipcMain.handle('file_disconnect', async (_, args: FileOpArgs) => {
    const { kind, handle } = args
    if (kind !== 'sftp') return
    const client = sftpConnections.get(handle)
    if (client) {
      try { await client.end() } catch {}
      sftpConnections.delete(handle)
    }
  })

  ipcMain.handle('file_home_dir', (): string => os.homedir())

  ipcMain.handle('file_list_dir', async (_, args: FileListDirArgs): Promise<FileEntry[]> => {
    const { kind, handle, path } = args
    if (kind === 'adb') {
      try {
        const out = await runAdbShell(handle, ['ls', '-la', shq(path)])
        return parseLsEntries(out)
      } catch (e) {
        console.warn('[electron] adb list dir failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
    }
    if (kind === 'local' || kind === 'wsl') {
      const dir = resolveLocalFsPath(kind, handle, path)
      const names = fs.readdirSync(dir)
      return names.map((name): FileEntry | null => {
        const full = pathModule.join(dir, name)
        let st: fs.Stats | null = null
        try { st = fs.statSync(full) } catch { st = null }
        if (!st) return null
        return {
          name,
          is_dir: st.isDirectory(),
          size: st.size,
          modified: formatLocalMtime(st.mtimeMs),
          permissions: '',
        }
      }).filter((x): x is FileEntry => x !== null)
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    const list = await client.list(path || '/')
    return list.map((item): FileEntry => ({
      name: item.name,
      is_dir: item.type === 'd',
      size: item.size || 0,
      modified: item.modifyTime ? new Date(item.modifyTime).toISOString() : '',
      permissions: item.rights ? `${item.rights.user}${item.rights.group}${item.rights.other}` : '',
    }))
  })

  ipcMain.handle('file_download', async (_, args: FileTransferArgs) => {
    const { kind, handle, transferId, remote, local } = args
    if (kind === 'adb') {
      try {
        await runAdb(['-s', handle, 'pull', remote, local])
      } catch (e) {
        console.warn('[electron] adb pull failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      fs.copyFileSync(resolveLocalFsPath(kind, handle, remote), local)
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')

    const stat = await client.stat(remote)
    const totalSize = stat.size || 0
    let cancelled = false

    const rdr = client.createReadStream(remote)
    const wtr = fs.createWriteStream(local)
    const h = {
      abort: () => {
        cancelled = true
        rdr.destroy()
        wtr.destroy()
      },
    }
    sftpTransfers.set(transferId, h)

    try {
      let transferred = 0
      await new Promise<void>((resolve, reject) => {
        const settle = () => {
          if (cancelled) reject(new Error('Cancelled'))
        }
        rdr.on('error', (err: Error) => reject(err))
        wtr.on('error', (err: Error) => reject(err))
        rdr.on('close', settle)
        wtr.on('close', settle)
        rdr.on('data', (chunk: Buffer) => {
          if (cancelled) {
            rdr.destroy()
            return
          }
          transferred += chunk.length
          emitToRenderer('file-progress', {
            remote, local, type: 'download',
            bytes_transferred: transferred, total_size: totalSize,
          })
          if (!wtr.write(chunk)) rdr.pause()
        })
        wtr.on('drain', () => rdr.resume())
        rdr.on('end', () => wtr.end(() => resolve()))
      })
      if (cancelled) throw new Error('Cancelled')
    } finally {
      sftpTransfers.delete(transferId)
    }
  })

  ipcMain.handle('file_upload', async (_, args: FileTransferArgs) => {
    const { kind, handle, transferId, local, remote } = args
    if (kind === 'adb') {
      try {
        await runAdb(['-s', handle, 'push', local, remote])
      } catch (e) {
        console.warn('[electron] adb push failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      fs.copyFileSync(local, resolveLocalFsPath(kind, handle, remote))
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')

    const totalSize = fs.statSync(local).size
    let cancelled = false

    const rdr = fs.createReadStream(local)
    const wtr = client.createWriteStream(remote)
    const h = {
      abort: () => {
        cancelled = true
        rdr.destroy()
        wtr.destroy()
      },
    }
    sftpTransfers.set(transferId, h)

    try {
      let transferred = 0
      await new Promise<void>((resolve, reject) => {
        const settle = () => {
          if (cancelled) reject(new Error('Cancelled'))
        }
        rdr.on('error', (err: Error) => reject(err))
        wtr.on('error', (err: Error) => reject(err))
        rdr.on('close', settle)
        wtr.on('close', settle)
        rdr.on('data', (chunk: Buffer) => {
          if (cancelled) {
            rdr.destroy()
            return
          }
          transferred += chunk.length
          emitToRenderer('file-progress', {
            remote, local, type: 'upload',
            bytes_transferred: transferred, total_size: totalSize,
          })
          if (!wtr.write(chunk)) rdr.pause()
        })
        wtr.on('drain', () => rdr.resume())
        rdr.on('end', () => wtr.end(() => resolve()))
      })
      if (cancelled) throw new Error('Cancelled')
    } finally {
      sftpTransfers.delete(transferId)
    }
  })

  ipcMain.handle('file_cancel_transfer', async (_, args: FileOpArgs & { transferId: string }) => {
    const h = sftpTransfers.get(args.transferId)
    if (!h) throw new Error('Transfer not found or already completed')
    h.abort()
  })

  ipcMain.handle('file_mkdir', async (_, args: FileMkdirArgs) => {
    const { kind, handle, path } = args
    if (kind === 'adb') {
      try {
        await runAdbShell(handle, ['mkdir', '-p', shq(path)])
      } catch (e) {
        console.warn('[electron] adb mkdir failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      fs.mkdirSync(resolveLocalFsPath(kind, handle, path), { recursive: true })
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    await client.mkdir(path)
  })

  ipcMain.handle('file_remove', async (_, args: FileRemoveArgs) => {
    const { kind, handle, path, is_dir } = args
    if (kind === 'adb') {
      try {
        await runAdbShell(handle, ['rm', is_dir ? '-rf' : '-f', shq(path)])
      } catch (e) {
        console.warn('[electron] adb remove failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      const target = resolveLocalFsPath(kind, handle, path)
      if (is_dir) fs.rmSync(target, { recursive: true, force: true })
      else fs.unlinkSync(target)
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    const stat = await client.stat(path)
    if (stat.isDirectory) await client.rmdir(path)
    else await client.delete(path)
  })

  ipcMain.handle('file_rename', async (_, args: FileRenameArgs) => {
    const { kind, handle, old_path, new_path } = args
    if (kind === 'adb') {
      try {
        await runAdbShell(handle, ['mv', shq(old_path), shq(new_path)])
      } catch (e) {
        console.warn('[electron] adb rename failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      fs.renameSync(resolveLocalFsPath(kind, handle, old_path), resolveLocalFsPath(kind, handle, new_path))
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    await client.rename(old_path, new_path)
  })

  ipcMain.handle('file_create', async (_, args: FileCreateArgs) => {
    const { kind, handle, path, is_dir, mode } = args
    if (kind === 'adb') {
      try {
        await runAdbShell(handle, is_dir ? ['mkdir', '-p', shq(path)] : ['touch', shq(path)])
      } catch (e) {
        console.warn('[electron] adb create failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      const target = resolveLocalFsPath(kind, handle, path)
      if (is_dir) fs.mkdirSync(target, { recursive: true })
      else fs.closeSync(fs.openSync(target, 'a'))
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    if (is_dir) {
      await client.mkdir(path)
      if (mode) await client.chmod(path, mode)
    } else {
      await client.put(Buffer.from(''), path)
      if (mode) await client.chmod(path, mode)
    }
  })

  ipcMain.handle('file_read', async (_, args: FileReadArgs): Promise<string> => {
    const { kind, handle, remote } = args
    if (kind === 'adb') {
      try {
        return await runAdbShell(handle, ['cat', shq(remote)])
      } catch (e) {
        console.warn('[electron] adb read file failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      }
    }
    if (kind === 'local' || kind === 'wsl') {
      return fs.readFileSync(resolveLocalFsPath(kind, handle, remote), 'utf8')
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    const buf = await client.get(remote)
    return Buffer.isBuffer(buf) ? buf.toString('utf8') : String(buf)
  })

  ipcMain.handle('file_write', async (_, args: FileWriteArgs) => {
    const { kind, handle, remote, content } = args
    if (kind === 'adb') {
      const tmp = path.join(os.tmpdir(), `aidterm_upload_${crypto.randomUUID()}.tmp`)
      try {
        fs.writeFileSync(tmp, content)
        await runAdb(['-s', handle, 'push', tmp, remote])
      } catch (e) {
        console.warn('[electron] adb write file failed:', e)
        throw new Error(e instanceof Error ? e.message : String(e))
      } finally {
        try { fs.unlinkSync(tmp) } catch { /* ignore */ }
      }
      return
    }
    if (kind === 'local' || kind === 'wsl') {
      fs.writeFileSync(resolveLocalFsPath(kind, handle, remote), content)
      return
    }
    const client = sftpConnections.get(handle)
    if (!client) throw new Error('SFTP connection not found')
    await client.put(Buffer.from(content), remote)
  })

  // ══════════════════════════════════════════════════════
  //  Tunnels (ssh2 port forwarding)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('tunnel_create', (_, args: TunnelCreateArgs) => {
    if (!ssh2Module) throw new Error('ssh2 not installed')

    const req = args.req || args
    const { host, port, username, password, privateKeyPath, tunnel_type, bind_addr, bind_port, target_host, target_port } = req
    const id = crypto.randomUUID()

    const conn = new ssh2Module.Client()
    const server = net.createServer()
    const tunnelInfo: TunnelInfo = {
      id, tunnel_type, bind_addr, bind_port, target_host, target_port,
      host, port, username, status: 'Starting',
    }

    conn.on('ready', () => {
      tunnelInfo.status = 'Running'
      emitToRenderer('tunnel-status', tunnelInfo as unknown as Record<string, unknown>)

      if (tunnel_type === 'Local' && target_host && target_port) {
        server.on('connection', (sock: net.Socket) => {
          // Direct-TCP/IP channel via the remote server — no `nc` needed on the
          // remote host (the old `conn.exec('nc host port')` failed on systems
          // without netcat).
          conn.forwardOut(bind_addr === '0.0.0.0' ? '127.0.0.1' : bind_addr, bind_port || 0, target_host, target_port, (err: Error | undefined, stream: ClientChannel) => {
            if (err) { sock.destroy(); return }
            sock.on('error', () => stream.close())
            sock.on('close', () => stream.close())
            stream.on('error', () => sock.destroy())
            stream.on('close', () => sock.destroy())
            sock.pipe(stream)
            stream.pipe(sock)
          })
        })
      }

      server.listen(bind_port, bind_addr === '0.0.0.0' ? '127.0.0.1' : bind_addr)
    })

    conn.on('error', (err: Error) => {
      tunnelInfo.status = `Error: ${err.message}`
      emitToRenderer('tunnel-status', tunnelInfo as unknown as Record<string, unknown>)
    })

    const connectOpts: SshConnectConfig = { host, port: port || 22, username }
    if (privateKeyPath && fs.existsSync(privateKeyPath)) connectOpts.privateKey = fs.readFileSync(privateKeyPath)
    else if (password) connectOpts.password = password
    conn.connect(connectOpts)

    tunnelMap.set(id, { info: tunnelInfo, server, conn })
    return tunnelInfo
  })

  ipcMain.handle('tunnel_list', () => {
    return Array.from(tunnelMap.values()).map(t => t.info)
  })

  ipcMain.handle('tunnel_remove', (_, args: TunnelRemoveArgs) => {
    const { id } = args
    const tunnel = tunnelMap.get(id)
    if (tunnel) {
      try { tunnel.server?.close() } catch {}
      try { tunnel.conn?.end() } catch {}
      tunnelMap.delete(id)
    }
  })

  // ══════════════════════════════════════════════════════
  //  Proxy
  // ══════════════════════════════════════════════════════
  ipcMain.handle('proxy_list', () => [...proxyConfigs])
  ipcMain.handle('proxy_save', (_, args: ProxySaveArgs) => {
    const config = args.config || args as unknown as ProxyConfig
    const idx = proxyConfigs.findIndex(p => p.id === config.id)
    if (idx >= 0) proxyConfigs[idx] = config
    else proxyConfigs.push(config)
  })
  ipcMain.handle('proxy_delete', (_, args: ProxyDeleteArgs) => {
    const { id } = args
    const idx = proxyConfigs.findIndex(p => p.id === id)
    if (idx >= 0) proxyConfigs.splice(idx, 1)
  })

  // ══════════════════════════════════════════════════════
  //  AI (OpenAI SDK)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('ai_chat', async (_, args: AiChatArgs) => {
    const { sessionId, messages, config } = args

    aiHistories.set(sessionId, messages)

    const controller = new AbortController()
    aiAborters.get(sessionId)?.abort()
    aiAborters.set(sessionId, controller)
    try {
      const response = await callAiChat(messages, config, controller.signal)

      if (response.tool_calls && response.tool_calls.length > 0) {
        const history = aiHistories.get(sessionId) || []
        history.push({ role: 'assistant', content: response.text || '', tool_calls: response.tool_calls })
        aiHistories.set(sessionId, history)
      }

      return response
    } finally {
      if (aiAborters.get(sessionId) === controller) aiAborters.delete(sessionId)
    }
  })

  ipcMain.handle('ai_cancel', (_, args: AiClearHistoryArgs) => {
    const { sessionId } = args
    aiAborters.get(sessionId)?.abort()
    aiAborters.delete(sessionId)
  })

  ipcMain.handle('ai_execute', async (_, args: AiExecuteArgs) => {
    const { command } = args
    const { spawnSync } = require('child_process') as typeof import('child_process')
    const isWin = process.platform === 'win32'
    const res = spawnSync(isWin ? 'cmd' : 'sh', isWin ? ['/C', command] : ['-c', command], {
      timeout: 60000,
      windowsHide: true,
      shell: false,
      maxBuffer: 16 * 1024 * 1024,
    })
    let result = ''
    if (res.stdout && res.stdout.length > 0) result += decodeCmdOutput(res.stdout as Buffer)
    if (res.stderr && res.stderr.length > 0) result += (result ? '\n' : '') + decodeCmdOutput(res.stderr as Buffer)
    if (res.status != null && res.status !== 0) result += `\n[Exit code: ${res.status}]`
    if (!result && res.error) result = res.error.message || 'Unknown error'
    return result
  })

  ipcMain.handle('ai_continue', async (_, args: AiContinueArgs) => {
    const { sessionId, toolCallId, toolResult, config } = args
    const history = aiHistories.get(sessionId) || []

    history.push({ role: 'tool', content: toolResult, tool_call_id: toolCallId })
    aiHistories.set(sessionId, history)

    const controller = new AbortController()
    aiAborters.get(sessionId)?.abort()
    aiAborters.set(sessionId, controller)
    try {
      const response = await callAiChat(history, config, controller.signal)

      if (response.tool_calls && response.tool_calls.length > 0) {
        const h2 = aiHistories.get(sessionId) || []
        h2.push({ role: 'assistant', content: response.text || '', tool_calls: response.tool_calls })
        aiHistories.set(sessionId, h2)
      }

      return response
    } finally {
      if (aiAborters.get(sessionId) === controller) aiAborters.delete(sessionId)
    }
  })

  ipcMain.handle('ai_clear_history', (_, args: AiClearHistoryArgs) => {
    const { sessionId } = args
    aiAborters.get(sessionId)?.abort()
    aiAborters.delete(sessionId)
    aiHistories.delete(sessionId)
  })

  ipcMain.handle('fetch_ai_models', async (_, args: FetchAiModelsArgs) => {
    const { provider, baseUrl, apiKey } = args
    try {
      let baseURL = baseUrl.replace(/\/+$/, '')
      if (provider === 'ollama') {
        baseURL = `${baseURL}/v1`
      }
      // Plain fetch + tolerant parse instead of the openai SDK: the SDK's typed
      // Model requires the legacy fields (created/object/owned_by), so providers
      // serving the newer {id, type, display_name, created_at} shape (2025+)
      // make the SDK deserialization fail and yield an empty list. Both shapes
      // carry a string `id`, so collect ids from data/models/bare-array bodies.
      const resp = await fetch(`${baseURL}/models`, {
        method: 'GET',
        headers: {
          Authorization: `Bearer ${apiKey}`,
          'Content-Type': 'application/json',
        },
        signal: AbortSignal.timeout(20000),
      })
      const text = await resp.text()
      if (!resp.ok) {
        throw new Error(`模型列表接口返回 HTTP ${resp.status}: ${snippet(text)}`)
      }
      const json = JSON.parse(text)
      const items = Array.isArray(json)
        ? json
        : (json.data ?? json.models ?? json.result) ?? []
      const ids = Array.isArray(items)
        ? items.map((m: any) => m?.id).filter((id: unknown) => typeof id === 'string' && id.length > 0)
        : []
      if (ids.length === 0) {
        throw new Error(`模型列表为空，未在响应中找到模型 id：${snippet(text)}`)
      }
      return ids
    } catch (e) {
      throw new Error((e as Error).message)
    }
  })

  // ══════════════════════════════════════════════════════
  //  System
  // ══════════════════════════════════════════════════════
  ipcMain.handle('get_system_info', (): SystemInfo => ({
    os: process.platform,
    arch: process.arch,
    hostname: os.hostname(),
    kernel: os.release(),
    shell: process.env.SHELL || process.env.ComSpec || 'sh',
  }))

  const makeRemoteExec = (sessionId: string) => (cmd: string, timeoutMs = 5000): Promise<string> => {
    const ssh = sshSessions.get(sessionId)
    if (!ssh || !ssh.conn) return Promise.resolve('')
    return new Promise((resolve) => {
      if (!ssh2Module) { resolve(''); return }
      const execConn = new ssh2Module.Client()
      const timer = setTimeout(() => {
        try { execConn.end() } catch {}
        resolve('')
      }, timeoutMs)

      execConn.on('ready', () => {
        execConn.exec(cmd, (err: Error | undefined, stream: ClientChannel) => {
          if (err) {
            clearTimeout(timer)
            try { execConn.end() } catch {}
            resolve('')
            return
          }
          let output = ''
          stream.on('data', (d: Buffer) => { output += d.toString('utf8') })
          stream.stderr.on('data', (d: Buffer) => { output += d.toString('utf8') })
          stream.on('close', () => {
            clearTimeout(timer)
            try { execConn.end() } catch {}
            resolve(output)
          })
        })
      })

      execConn.on('error', () => {
        clearTimeout(timer)
        resolve('')
      })

      // Reuse the stored connection parameters (host, port, auth)
      execConn.connect({ ...ssh.connectOpts })
    })
  }

  // WSL sessions run commands inside the WSL distro via wsl.exe (the WSL
  // environment is a Linux system, same as SSH targets).
  const makeWslExec = (sessionId: string) => async (cmd: string, timeoutMs = 8000): Promise<string> => {
    const distro = wslDistroForSession(sessionId)
    const args: string[] = []
    if (distro) args.push('-d', distro)
    args.push('-e', 'bash', '-lc', cmd)
    const { execFile } = require('child_process') as typeof import('child_process')
    return new Promise((resolve) => {
      execFile('wsl.exe', args, { timeout: timeoutMs, maxBuffer: 8 * 1024 * 1024 }, (err: Error | null, stdout: string | Buffer, stderr: string | Buffer) => {
        if (err) { resolve(''); return }
        resolve(String(stdout) + String(stderr))
      })
    })
  }

  // Run a command on the session's target: SSH -> remote exec, WSL -> local
  // wsl.exe exec, anything else -> empty output.
  const makeSessionExec = (sessionId: string) => (cmd: string, timeoutMs = 8000): Promise<string> => {
    if (sshSessions.has(sessionId)) return makeRemoteExec(sessionId)(cmd, timeoutMs)
    if (wslSessions.has(sessionId)) return makeWslExec(sessionId)(cmd, timeoutMs)
    return Promise.resolve('')
  }

  ipcMain.handle('get_remote_system_info', async (_, args: GetRemoteSystemInfoArgs) => {
    const { sessionId } = args
    const ssh = sshSessions.get(sessionId)
    if (!ssh || !ssh.conn) throw new Error('SSH session not found')

    // Open a fresh exec-only connection to avoid conflicts with the
    // interactive shell channel (some servers drop the socket when
    // exec is requested alongside an open shell).
    const execCmd = makeRemoteExec(sessionId)

    const uname = await execCmd('uname -a')
    if (!uname.trim()) {
      return { os: 'remote', arch: 'remote', hostname: 'remote', kernel: 'remote', shell: 'remote' }
    }
    const parts = uname.trim().split(/\s+/)
    let osLabel = parts[0] || 'remote'

    const osRelease = await execCmd('cat /etc/os-release')
    if (osRelease) {
      const prettyName = osRelease.split('\n').find((l: string) => l.startsWith('PRETTY_NAME='))
      if (prettyName) osLabel = prettyName.split('=')[1]?.replace(/"/g, '') || osLabel
      else {
        const nameLine = osRelease.split('\n').find((l: string) => l.startsWith('NAME='))
        if (nameLine) osLabel = nameLine.split('=')[1]?.replace(/"/g, '') || osLabel
      }
    }

    const shellOut = await execCmd('basename $SHELL 2>/dev/null || echo unknown')
    const shell = shellOut.trim() && shellOut.trim() !== 'unknown' ? shellOut.trim() : 'remote'

    const unameM = await execCmd('uname -m')
    const arch = unameM.trim() || parts[parts.length - 2] || 'remote'

    return {
      os: osLabel,
      arch,
      hostname: parts[1] || 'remote',
      kernel: parts[2] || 'remote',
      shell,
    }
  })

  // Per-session previous sample (CPU + net counters + timestamp) for rate deltas.
  const monitorPrev = new Map<string, {
    cpu: [number, number] | null
    net: Map<string, [number, number]>
    at: number
  }>()

  const collectRemoteSystemMetrics = async (sessionId: string, execCmd: (cmd: string, timeoutMs?: number) => Promise<string>): Promise<RemoteSystemMetrics> => {

    const MARKERS = '__AID_MONITOR__'
    const out = await execCmd(
      `printf '${MARKERS}\\n'; cat /proc/stat 2>/dev/null; ` +
      `printf '${MARKERS}\\n'; cat /proc/meminfo 2>/dev/null; ` +
      `printf '${MARKERS}\\n'; cat /proc/loadavg 2>/dev/null; ` +
      `printf '${MARKERS}\\n'; df -Pk 2>/dev/null | tail -n +2; ` +
      `printf '${MARKERS}\\n'; cat /proc/net/dev 2>/dev/null | tail -n +3; ` +
      `printf '${MARKERS}\\n'; ` +
      `if command -v nvidia-smi >/dev/null 2>&1; then ` +
      `printf '__AID_GPU_NVIDIA__\\n'; ` +
      `nvidia-smi --query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu --format=csv,noheader,nounits 2>/dev/null; ` +
      `elif command -v rocm-smi >/dev/null 2>&1; then ` +
      `printf '__AID_GPU_AMD__\\n'; ` +
      `rocm-smi --showuse --showmeminfo vram --showtemp --json 2>/dev/null; ` +
      `elif command -v intel_gpu_top >/dev/null 2>&1; then ` +
      `printf '__AID_GPU_INTEL__\\n'; ` +
      `intel_gpu_top -J -s 1000 -l 2>/dev/null | head -n 1; ` +
      `fi`,
      8000,
    )

    const sections = out.split(MARKERS)
    const stat = sections[1] || ''
    const meminfo = sections[2] || ''
    const loadavg = sections[3] || ''
    const dfOut = sections[4] || ''
    const netdev = sections[5] || ''
    const gpuSection = sections[6] || ''

    // CPU: first "cpu " line -> [total, idle]
    const parseCpu = (txt: string): [number, number] | null => {
      const line = txt.split('\n').find((l) => l.startsWith('cpu '))
      if (!line) return null
      const nums = line.split(/\s+/).slice(1).map(Number)
      if (nums.length < 8 || nums.some(Number.isNaN)) return null
      return [nums.slice(0, 8).reduce((a, b) => a + b, 0), nums[3] + nums[4]]
    }
    const curCpu = parseCpu(stat)
    const cpuCores = Math.max(1, stat.split('\n').filter((l) => /^cpu\d/.test(l)).length)

    // Network: iface -> [rx, tx]
    const parseNet = (txt: string): Map<string, [number, number]> => {
      const out = new Map<string, [number, number]>()
      for (const line of txt.split('\n')) {
        const m = line.trim().match(/^(\S+):\s+(.+)$/)
        if (!m) continue
        const name = m[1]
        if (!name || name === 'lo') continue
        const fields = m[2].split(/\s+/).map(Number)
        if (fields.length >= 16 && !fields.some(Number.isNaN)) {
          out.set(name, [fields[0], fields[8]])
        }
      }
      return out
    }
    const curNet = parseNet(netdev)

    const now = Date.now()
    const prev = monitorPrev.get(sessionId)
    const dt = prev ? (now - prev.at) / 1000 : 0

    let cpuPercent = 0
    if (prev?.cpu && curCpu) {
      const [pt, pid] = prev.cpu
      const [t, idle] = curCpu
      const dTotal = t - pt
      const dIdle = idle - pid
      if (dTotal > 0) {
        cpuPercent = Math.min(100, Math.max(0, (1 - dIdle / dTotal) * 100))
      }
    }

    const loads = loadavg.trim().split(/\s+/).map(Number)
    const load1 = loads[0] || 0
    const load5 = loads[1] || 0
    const load15 = loads[2] || 0

    const memVal = (key: string): number => {
      const line = meminfo.split('\n').find((l) => l.startsWith(key + ':'))
      return line ? Number(line.split(':')[1]?.trim().split(/\s+/)[0]) || 0 : 0
    }
    const memTotal = memVal('MemTotal')
    const memAvail = memVal('MemAvailable')
    const memUsed = memAvail > 0 ? memTotal - memAvail : memTotal - memVal('MemFree')
    const swapTotal = memVal('SwapTotal')
    const swapUsed = swapTotal - memVal('SwapFree')

    const disks: RemoteSystemMetrics['disks'] = []
    for (const line of dfOut.split('\n')) {
      const f = line.trim().split(/\s+/)
      if (f.length < 6) continue
      const fstype = f[0]
      if (['tmpfs', 'devtmpfs', 'udev', 'overlay', 'squashfs', 'shm'].includes(fstype)) continue
      const totalKb = Number(f[1]) || 0
      const usedKb = Number(f[2]) || 0
      const mount = f[5]
      if (mount.startsWith('/run') || mount.startsWith('/sys') || mount.startsWith('/dev') || mount.startsWith('/proc') || mount.includes('/snap')) continue
      disks.push({ mount, total_mb: Math.floor(totalKb / 1024), used_mb: Math.floor(usedKb / 1024) })
    }

    const prevNet = prev?.net ?? new Map<string, [number, number]>()
    const nets: RemoteSystemMetrics['nets'] = []
    for (const [name, [rx, tx]] of curNet) {
      const [prx, ptx] = prevNet.get(name) ?? [rx, tx]
      const rxBps = dt > 0 ? Math.max(0, (rx - prx) / dt) : 0
      const txBps = dt > 0 ? Math.max(0, (tx - ptx) / dt) : 0
      nets.push({ name, rx_bps: rxBps, tx_bps: txBps })
    }
    nets.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))

    // GPU: probe whichever vendor tool exists (nvidia-smi / rocm-smi / intel_gpu_top).
    const gpus: RemoteSystemMetrics['gpus'] = []
    const gpuBody = gpuSection.trim()
    if (gpuBody.startsWith('__AID_GPU_NVIDIA__')) {
      for (const line of gpuBody.split('\n').slice(1)) {
        const parts = line.split(',').map((p) => p.trim())
        if (parts.length < 5) continue
        gpus.push({
          vendor: 'nvidia',
          name: parts[0],
          utilization: Math.min(100, Math.max(0, Number(parts[1]) || 0)),
          mem_total_mb: Number(parts[2]) || 0,
          mem_used_mb: Number(parts[3]) || 0,
          temperature: Number(parts[4]) || 0,
        })
      }
    } else if (gpuBody.startsWith('__AID_GPU_AMD__')) {
      const jsonText = gpuBody.split('\n').slice(1).join('\n')
      try {
        const parsed = JSON.parse(jsonText)
        for (const [card, val] of Object.entries(parsed)) {
          if (!card.startsWith('card')) continue
          const obj = (val as Record<string, unknown>) ?? {}
          let name = card
          let util = 0
          let memUsedB = 0
          let memTotalB = 0
          let temp = 0
          for (const [k, v] of Object.entries(obj)) {
            const kk = k.toLowerCase()
            const s = String(v ?? '')
            const num = s.split(/\s+/)[0]?.replace(/%$/, '')
            if (kk.includes('product name')) name = s.trim()
            else if (kk.includes('gpu use')) util = Number(num) || 0
            else if (kk.includes('memory used')) memUsedB = Number(num) || 0
            else if (kk.includes('memory total')) memTotalB = Number(num) || 0
            else if (kk.includes('temperature')) temp = Number(num) || 0
          }
          gpus.push({
            vendor: 'amd',
            name,
            utilization: Math.min(100, Math.max(0, util)),
            mem_total_mb: Math.floor(memTotalB / (1024 * 1024)),
            mem_used_mb: Math.floor(memUsedB / (1024 * 1024)),
            temperature: temp,
          })
        }
      } catch { /* ignore malformed rocm-smi JSON */ }
    } else if (gpuBody.startsWith('__AID_GPU_INTEL__')) {
      const jsonText = gpuBody.split('\n').slice(1).join('\n')
      try {
        const parsed = JSON.parse(jsonText)
        let util = 0
        for (const e of parsed.engines ?? []) {
          if (typeof e.busy === 'number') util = Math.max(util, e.busy)
        }
        gpus.push({
          vendor: 'intel',
          name: 'Intel GPU',
          utilization: Math.min(100, Math.max(0, util)),
          mem_total_mb: 0,
          mem_used_mb: 0,
          temperature: 0,
        })
      } catch { /* ignore malformed intel_gpu_top JSON */ }
    }

    monitorPrev.set(sessionId, { cpu: curCpu, net: curNet, at: now })

    return {
      cpu_percent: cpuPercent,
      cpu_cores: cpuCores,
      load_1: load1,
      load_5: load5,
      load_15: load15,
      mem_total_mb: Math.floor(memTotal / 1024),
      mem_used_mb: Math.floor(memUsed / 1024),
      swap_total_mb: Math.floor(swapTotal / 1024),
      swap_used_mb: Math.floor(swapUsed / 1024),
      disks,
      nets,
      gpus,
    }
  }

  const collectLocalSystemMetrics = async (): Promise<RemoteSystemMetrics> => {
    const si = require('systeminformation') as typeof import('systeminformation')

    const cpuLoad = await si.currentLoad().catch(() => ({ currentLoad: 0 }))
    const cpuPercent = Math.min(100, Math.max(0, cpuLoad.currentLoad))
    const cpuCores = os.cpus().length || 1
    const loads = os.loadavg()
    const mem = await si.mem().catch(() => ({ total: 0, active: 0, swaptotal: 0, swapused: 0 }))
    const memTotalMb = Math.floor(mem.total / (1024 * 1024))
    const memUsedMb = Math.floor(mem.active / (1024 * 1024))
    const swapTotalMb = Math.floor(mem.swaptotal / (1024 * 1024))
    const swapUsedMb = Math.floor(mem.swapused / (1024 * 1024))

    const fsSizes = await si.fsSize().catch(() => [] as Array<{ mount: string; size: number; used: number }>)
    const disks: RemoteSystemMetrics['disks'] = []
    for (const d of fsSizes) {
      const mount = String(d.mount ?? '')
      if (mount.startsWith('/run') || mount.startsWith('/sys') || mount.startsWith('/dev') || mount.startsWith('/proc') || mount.includes('/snap')) continue
      disks.push({
        mount,
        total_mb: Math.floor(d.size / (1024 * 1024)),
        used_mb: Math.floor(d.used / (1024 * 1024)),
      })
    }

    const netStats = await si.networkStats().catch(() => [] as Array<{ iface: string; rx_sec: number; tx_sec: number }>)
    const nets: RemoteSystemMetrics['nets'] = []
    for (const n of netStats) {
      if (n.iface === 'lo') continue
      nets.push({ name: n.iface, rx_bps: n.rx_sec || 0, tx_bps: n.tx_sec || 0 })
    }
    nets.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))

    const gpus: RemoteSystemMetrics['gpus'] = []
    const { spawnSync } = require('child_process') as typeof import('child_process')
    const nvidia = spawnSync('nvidia-smi', ['--query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu', '--format=csv,noheader,nounits'], { encoding: 'utf8', timeout: 8000 })
    if (!nvidia.error && nvidia.status === 0) {
      for (const line of String(nvidia.stdout).split('\n')) {
        const parts = line.split(',').map((p: string) => p.trim())
        if (parts.length < 5) continue
        gpus.push({
          vendor: 'nvidia',
          name: parts[0],
          utilization: Math.min(100, Math.max(0, Number(parts[1]) || 0)),
          mem_total_mb: Number(parts[2]) || 0,
          mem_used_mb: Number(parts[3]) || 0,
          temperature: Number(parts[4]) || 0,
        })
      }
    } else if (process.platform === 'linux') {
      const rocm = spawnSync('rocm-smi', ['--showuse', '--showmeminfo', 'vram', '--showtemp', '--json'], { encoding: 'utf8', timeout: 8000 })
      if (!rocm.error && rocm.status === 0) {
        try {
          const parsed = JSON.parse(String(rocm.stdout))
          for (const [card, val] of Object.entries(parsed)) {
            if (!card.startsWith('card')) continue
            const obj = (val as Record<string, unknown>) ?? {}
            let name = card
            let util = 0
            let memUsedB = 0
            let memTotalB = 0
            let temp = 0
            for (const [k, v] of Object.entries(obj)) {
              const kk = k.toLowerCase()
              const s = String(v ?? '')
              const num = s.split(/\s+/)[0]?.replace(/%$/, '')
              if (kk.includes('product name')) name = s.trim()
              else if (kk.includes('gpu use')) util = Number(num) || 0
              else if (kk.includes('memory used')) memUsedB = Number(num) || 0
              else if (kk.includes('memory total')) memTotalB = Number(num) || 0
              else if (kk.includes('temperature')) temp = Number(num) || 0
            }
            gpus.push({
              vendor: 'amd',
              name,
              utilization: Math.min(100, Math.max(0, util)),
              mem_total_mb: Math.floor(memTotalB / (1024 * 1024)),
              mem_used_mb: Math.floor(memUsedB / (1024 * 1024)),
              temperature: temp,
            })
          }
        } catch { /* ignore malformed rocm-smi JSON */ }
      } else {
        const intel = spawnSync('intel_gpu_top', ['-J', '-s', '1000', '-l'], { encoding: 'utf8', timeout: 8000 })
        if (!intel.error && intel.status === 0) {
          try {
            const parsed = JSON.parse(String(intel.stdout))
            let util = 0
            for (const e of parsed.engines ?? []) {
              if (typeof e.busy === 'number') util = Math.max(util, e.busy)
            }
            gpus.push({
              vendor: 'intel',
              name: 'Intel GPU',
              utilization: Math.min(100, Math.max(0, util)),
              mem_total_mb: 0,
              mem_used_mb: 0,
              temperature: 0,
            })
          } catch { /* ignore malformed intel_gpu_top JSON */ }
        }
      }
    }

    return {
      cpu_percent: cpuPercent,
      cpu_cores: cpuCores,
      load_1: loads[0] || 0,
      load_5: loads[1] || 0,
      load_15: loads[2] || 0,
      mem_total_mb: memTotalMb,
      mem_used_mb: memUsedMb,
      swap_total_mb: swapTotalMb,
      swap_used_mb: swapUsedMb,
      disks,
      nets,
      gpus,
    }
  }

  // Unified monitor entry: report the connection target's system. SSH and WSL
  // report a Linux environment (remote / WSL distro) via exec; local sessions
  // report the local host.
  ipcMain.handle('get_system_metrics', async (_, args: GetRemoteSystemMetricsArgs): Promise<RemoteSystemMetrics> => {
    const { sessionId } = args
    if (sshSessions.has(sessionId) || wslSessions.has(sessionId)) {
      return collectRemoteSystemMetrics(sessionId, makeSessionExec(sessionId))
    }
    return collectLocalSystemMetrics()
  })

  ipcMain.handle('cli_args', () => {
    // Packaged builds put the executable path in argv[0]; dev mode has
    // [electron, main.js, ...]. Also drop macOS Finder's -psn_0_xxx arg.
    const args = process.argv.slice(app.isPackaged ? 1 : 2)
    const clean = args.filter((a) => !a.startsWith('-psn_0_'))
    // Point the process working directory at the requested folder so tabs opened
    // without an explicit directory land there: --cwd when given (right-click
    // menu), otherwise the user's home (e.g. double-clicked from Explorer).
    const cwdIndex = clean.indexOf('--cwd')
    const target = cwdIndex !== -1 ? clean[cwdIndex + 1] : os.homedir()
    try { process.chdir(target) } catch { /* ignore */ }
    return clean
  })
  ipcMain.handle('set_working_directory', (_event, args: { dir: string }) => {
    process.chdir(args.dir)
  })
  ipcMain.handle('shell_context_menu_get_enabled', () => shellContextMenuEnabled())
  ipcMain.handle('shell_context_menu_set_enabled', (_, args: ToggleSettingArgs) => {
    setShellContextMenuEnabled(args.enabled)
  })
  ipcMain.handle('path_environment_get_enabled', () => pathEnvironmentEnabled())
  ipcMain.handle('path_environment_set_enabled', (_, args: ToggleSettingArgs) => {
    setPathEnvironmentEnabled(args.enabled)
  })
  ipcMain.handle('get_single_instance', () => readSingleInstanceSetting())
  ipcMain.handle('set_single_instance', (_, args: ToggleSettingArgs) => {
    writeSingleInstanceSetting(args.enabled)
  })
  ipcMain.handle('detect_shells', (): Array<{ name: string; command: string; icon: string; terminal_type?: 'local' | 'wsl' }> => {
    const shells: Array<{ name: string; command: string; icon: string; terminal_type?: 'local' | 'wsl' }> = []
    const has = (exe: string) => {
      const dirs = (process.env.PATH || '').split(path.delimiter).filter(Boolean)
      for (const dir of dirs) {
        const full = path.join(dir, exe)
        try { fs.accessSync(full, fs.constants.X_OK); return true } catch { /* keep searching */ }
      }
      return false
    }
    if (process.platform === 'win32') {
      shells.push({ name: '命令提示符', command: 'cmd.exe', icon: '\u{1F4DF}', terminal_type: 'local' })
      if (fs.existsSync('C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe')) shells.push({ name: 'Windows PowerShell', command: 'powershell.exe', icon: '\u{1F4DF}', terminal_type: 'local' })
      if (has('pwsh.exe')) shells.push({ name: 'PowerShell', command: 'pwsh.exe', icon: '\u{1F4DF}', terminal_type: 'local' })
      if (has('wsl.exe')) shells.push({ name: 'WSL', command: 'wsl.exe', icon: '\u{1F427}', terminal_type: 'wsl' })
      if (has('bash.exe')) shells.push({ name: 'Bash', command: 'bash.exe', icon: '\u{1F40D}', terminal_type: 'local' })
      return shells
    }
    if (process.platform === 'darwin' && has('zsh')) shells.push({ name: 'Zsh', command: 'zsh', icon: '\u{1F334}', terminal_type: 'local' })
    if (has('bash')) shells.push({ name: 'Bash', command: 'bash', icon: '\u{1F40D}', terminal_type: 'local' })
    if (has('sh')) shells.push({ name: 'Sh', command: 'sh', icon: '\u{1F40D}', terminal_type: 'local' })
    if (process.platform !== 'darwin' && has('zsh')) shells.push({ name: 'Zsh', command: 'zsh', icon: '\u{1F334}', terminal_type: 'local' })
    if (has('fish')) shells.push({ name: 'Fish', command: 'fish', icon: '\u{1F41F}', terminal_type: 'local' })
    return shells
  })

  // ══════════════════════════════════════════════════════
  //  Update (check GitHub releases → download → install)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('get_app_version', () => app.getVersion())

  ipcMain.handle('get_installer_type', () => getInstallerType())

  ipcMain.handle('check_for_update', async () => {
    const current = app.getVersion()
    const resp = await fetch(UPDATE_LATEST_API, { headers: { 'User-Agent': `AidTerm/${current}` } })
    if (!resp.ok) throw new Error(`Update check failed: HTTP ${resp.status}`)
    const release = await resp.json() as {
      tag_name?: string
      html_url?: string
      published_at?: string | null
      body?: string | null
      assets?: Array<{ name: string; browser_download_url: string }>
    }
    const latest = String(release.tag_name || '').replace(/^v/, '')
    const hasUpdate = compareVersions(latest, current) > 0
    const { assetName, assetUrl } = pickElectronAsset(release.assets || [])
    return {
      current_version: current,
      latest_version: latest,
      has_update: hasUpdate,
      release_url: release.html_url || `https://github.com/${UPDATE_REPO}/releases/latest`,
      asset_name: assetName,
      asset_url: assetUrl,
      published_at: release.published_at || null,
      body: release.body || null,
      installer_type: getInstallerType(),
    }
  })

  ipcMain.handle('download_update', async (_, args: { url: string }) => {
    const fname = args.url.split('/').filter(Boolean).pop() || 'AidTerm-update'
    const dest = path.join(app.getPath('temp'), fname)
    return downloadToFile(args.url, dest)
  })

  ipcMain.handle('install_update', (_, args: { path: string }) => {
    const { spawn } = require('child_process') as typeof import('child_process')
    const p = args.path
    if (process.platform === 'win32') {
      spawn(p, [], { detached: true, stdio: 'ignore' }).unref()
      setTimeout(() => app.exit(0), 1200)
    } else if (process.platform === 'darwin') {
      spawn('open', [p], { detached: true, stdio: 'ignore' }).unref()
    } else {
      try { fs.chmodSync(p, 0o755) } catch {}
      if (p.endsWith('.AppImage')) {
        spawn(p, [], { detached: true, stdio: 'ignore' }).unref()
        setTimeout(() => app.exit(0), 1200)
      } else {
        spawn('xdg-open', [p], { detached: true, stdio: 'ignore' }).unref()
      }
    }
  })

  // ══════════════════════════════════════════════════════
  //  Zmodem (stub)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('zmodem_respond', () => {})

  // ══════════════════════════════════════════════════════
  //  Session Store
  // ══════════════════════════════════════════════════════
  ipcMain.handle('load_session_store', () => loadSessionStore())
  ipcMain.handle('save_session_store', (_, args: SaveSessionStoreArgs) => { saveSessionStore(args.data || args as unknown as SessionStoreData) })

  // ══════════════════════════════════════════════════════
  //  Keychain
  // ══════════════════════════════════════════════════════
  ipcMain.handle('key_list', () => {
    return Array.from(keyIndex.values())
  })

  ipcMain.handle('key_generate_rsa', (_, args: KeyGenerateRsaArgs) => {
    const { name, bits, passphrase } = args
    const id = crypto.randomUUID()
    const privPath = path.join(keysDir, `${name}_id_rsa`)
    const pubPath = `${privPath}.pub`

    const keyArgs = ['-t', 'rsa', '-b', String(bits || 2048), '-f', privPath, '-C', `aidterm-${name}`, '-N', passphrase || '']
    runCmd('ssh-keygen', keyArgs)

    const pubContent = fs.readFileSync(pubPath, 'utf8').trim()
    const fpOut = runCmd('ssh-keygen', ['-l', '-f', privPath])
    const fingerprint = fpOut.trim().split(/\s+/)[0] || ''

    const info: KeyInfo = {
      id, name, key_type: 'RSA', bits: bits || 2048,
      public_key: pubContent, fingerprint,
      private_key_path: privPath,
      public_key_path: pubPath,
      created_at: new Date().toISOString(),
    }
    keyIndex.set(id, info)
    saveKeyIndex()
    return info
  })

  ipcMain.handle('key_generate_ed25519', (_, args: KeyGenerateEd25519Args) => {
    const { name, passphrase } = args
    const id = crypto.randomUUID()
    const privPath = path.join(keysDir, `${name}_id_ed25519`)
    const pubPath = `${privPath}.pub`

    const keyArgs = ['-t', 'ed25519', '-f', privPath, '-C', `aidterm-${name}`, '-N', passphrase || '']
    runCmd('ssh-keygen', keyArgs)

    const pubContent = fs.readFileSync(pubPath, 'utf8').trim()
    const fpOut = runCmd('ssh-keygen', ['-l', '-f', privPath])
    const fingerprint = fpOut.trim().split(/\s+/)[0] || ''

    const info: KeyInfo = {
      id, name, key_type: 'ED25519', bits: 256,
      public_key: pubContent, fingerprint,
      private_key_path: privPath,
      public_key_path: pubPath,
      created_at: new Date().toISOString(),
    }
    keyIndex.set(id, info)
    saveKeyIndex()
    return info
  })

  ipcMain.handle('key_delete', (_, args: KeyDeleteArgs) => {
    const { id } = args
    const info = keyIndex.get(id)
    if (info) {
      try { fs.unlinkSync(info.private_key_path) } catch {}
      try { fs.unlinkSync(info.public_key_path) } catch {}
      keyIndex.delete(id)
      saveKeyIndex()
    }
  })

  ipcMain.handle('key_import', (_, args: KeyImportArgs) => {
    const { name, privateKeyPath } = args
    if (!fs.existsSync(privateKeyPath)) throw new Error('Private key file not found')

    const id = crypto.randomUUID()
    const destPriv = path.join(keysDir, `${name}_imported`)
    fs.copyFileSync(privateKeyPath, destPriv)
    try { fs.chmodSync(destPriv, 0o600) } catch {}

    let pubContent = ''
    try {
      pubContent = runCmd('ssh-keygen', ['-y', '-f', destPriv]).trim()
    } catch (err: unknown) {
      const e = err as { message?: string }
      throw new Error(`Failed to extract public key: ${e.message}`)
    }

    let keyType = 'RSA'
    if (pubContent.startsWith('ssh-ed25519')) keyType = 'ED25519'
    else if (pubContent.startsWith('ecdsa-sha2-nistp256')) keyType = 'ECDSA'
    else if (pubContent.includes('RSA')) keyType = 'RSA'

    let bits = 256
    const bitsMatch = pubContent.match(/(\d+)\s*$/)
    if (bitsMatch) bits = parseInt(bitsMatch[1])

    let fingerprint = ''
    try {
      const fpOut = runCmd('ssh-keygen', ['-l', '-f', destPriv])
      fingerprint = fpOut.trim().split(/\s+/)[0] || ''
    } catch {}

    const info: KeyInfo = {
      id, name, key_type: keyType, bits,
      public_key: pubContent, fingerprint,
      private_key_path: destPriv,
      public_key_path: '',
      created_at: new Date().toISOString(),
    }
    keyIndex.set(id, info)
    saveKeyIndex()
    return info
  })

  // ══════════════════════════════════════════════════════
  //  Known Hosts
  // ══════════════════════════════════════════════════════
  ipcMain.handle('known_hosts_list', () => loadKnownHosts())
  ipcMain.handle('known_hosts_add', (_, args: KnownHostsAddArgs) => {
    addKnownHost(args.host, args.keyType, args.key)
  })
  ipcMain.handle('known_hosts_remove', (_, args: KnownHostsRemoveArgs) => {
    removeKnownHost(args.host, args.keyType)
  })
}

// ══════════════════════════════════════════════════════════════
//  AI helper — OpenAI SDK chat completion
// ══════════════════════════════════════════════════════════════

async function callAiChat(messages: AiMessage[], config: AiConfig, signal?: AbortSignal): Promise<AiResponse> {
  const { provider, api_key, model, base_url } = config

  let baseURL = base_url.replace(/\/+$/, '')
  if (provider === 'ollama') {
    baseURL = `${baseURL}/v1`
  }

  const OpenAI = (await import('openai')).default
  const client = new OpenAI({ apiKey: api_key, baseURL })

  const apiMessages = messages.map((m) => {
    const msg: Record<string, unknown> = { role: m.role, content: m.content }
    if (m.tool_call_id) msg.tool_call_id = m.tool_call_id
    if (m.tool_calls) {
      msg.tool_calls = m.tool_calls.map((tc) => ({
        id: tc.id,
        type: 'function',
        function: { name: tc.function.name, arguments: tc.function.arguments },
      }))
    }
    return msg as unknown as Parameters<typeof client.chat.completions.create>[0]['messages'][number]
  })

  const response = await client.chat.completions.create({
    model,
    messages: apiMessages,
    tools: [
      {
        type: 'function',
        function: {
          name: 'execute_command',
          description: '在用户当前终端中执行一条 shell 命令（若连接了 SSH 则在远端执行），返回命令的输出结果。执行命令后，系统会将输出结果返回给你，请根据结果继续推理。',
          parameters: {
            type: 'object',
            properties: {
              command: { type: 'string', description: '要执行的 shell 命令' },
            },
            required: ['command'],
          },
        },
      },
      {
        type: 'function',
        function: {
          name: 'read_output_page',
          description: '读取命令输出中指定的一页内容。当工具结果注明输出共有 N 页时使用。参数 output_id 为命令输出的唯一标识，page 为页码（从 1 开始）。',
          parameters: {
            type: 'object',
            properties: {
              output_id: { type: 'string', description: '命令输出的唯一标识' },
              page: { type: 'integer', description: '页码（从 1 开始）' },
            },
            required: ['output_id', 'page'],
          },
        },
      },
    ],
    tool_choice: 'auto',
    parallel_tool_calls: false,
  }, { signal })

  const choice = response.choices?.[0]
  if (!choice) throw new Error('No choices in AI response')

  const toolCalls: AiToolCall[] = (choice.message?.tool_calls || [])
    .filter((tc): tc is typeof tc & { function: { name: string; arguments: string } } => 'function' in tc)
    .map((tc) => ({
      id: tc.id,
      function: { name: tc.function.name, arguments: tc.function.arguments },
    }))

  return { text: choice.message?.content || null, tool_calls: toolCalls }
}
