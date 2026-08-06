import { app, BrowserWindow, ipcMain, dialog, clipboard } from 'electron'
import * as path from 'path'
import * as os from 'os'
import * as fs from 'fs'
import * as net from 'net'
import * as crypto from 'crypto'
import type { Client as Ssh2Client, ClientChannel, ConnectConfig as SshConnectConfig } from 'ssh2'
import type SftpClient from 'ssh2-sftp-client'
import type {
  PtySession, SshSession, SerialSession, TunnelState, TunnelInfo,
  ProxyConfig, AiMessage, AiResponse, AiConfig, AiToolCall,
  KeyInfo, KnownHostEntry, SessionStoreData, SystemInfo,
  SftpFileEntry, SftpProgress, SerialPortInfo,
  SpawnTerminalArgs, WriteTerminalArgs, ResizeTerminalArgs, KillTerminalArgs,
  SshConnectArgs, TelnetConnectArgs, SerialConnectArgs,
  SftpConnectArgs, SftpDisconnectArgs, SftpListDirArgs, SftpTransferArgs,
  SftpMkdirArgs, SftpRemoveArgs, SftpRenameArgs, SftpCreateArgs,
  SftpReadFileArgs, SftpWriteFileArgs,
  TunnelCreateArgs, TunnelRemoveArgs,
  ProxySaveArgs, ProxyDeleteArgs,
  AiChatArgs, AiExecuteArgs, AiContinueArgs, AiClearHistoryArgs,
  FetchAiModelsArgs, GetRemoteSystemInfoArgs,
  SaveSessionStoreArgs, KeyGenerateRsaArgs, KeyGenerateEd25519Args,
  KeyDeleteArgs, KeyImportArgs, KnownHostsAddArgs, KnownHostsRemoveArgs,
  WindowSetFullscreenArgs, ClipboardWriteArgs,
  OpenDialogOpts, SaveDialogOpts, DialogFilter,
  TunnelType,
} from './types'

// ── Lazy-loaded native modules ──
let ptyModule: typeof import('node-pty') | null = null
let ssh2Module: typeof import('ssh2') | null = null
let SftpClientClass: typeof SftpClient | null = null
let SerialPortClass: typeof import('serialport').SerialPort | null = null

function loadNativeModules(): void {
  try { ptyModule = require('node-pty') } catch { console.warn('[electron] node-pty not available') }
  try { ssh2Module = require('ssh2') } catch { console.warn('[electron] ssh2 not available') }
  try { SftpClientClass = require('ssh2-sftp-client') } catch { console.warn('[electron] ssh2-sftp-client not available') }
  try { SerialPortClass = require('serialport').SerialPort } catch { console.warn('[electron] serialport not available') }
}

let mainWindow: BrowserWindow | null = null
const isDev = !app.isPackaged

// ── In-memory state ──
const ptySessions = new Map<string, PtySession>()
const sshSessions = new Map<string, SshSession>()
const serialSessions = new Map<string, SerialSession>()
const sftpConnections = new Map<string, SftpClient>()
const sftpTransfers = new Map<string, { abort: () => void }>()
const tunnelMap = new Map<string, TunnelState>()
const proxyConfigs: ProxyConfig[] = []
const aiHistories = new Map<string, AiMessage[]>()
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

function createWindow(): void {
  const isWindows = process.platform === 'win32'
  const isLinux = process.platform === 'linux'
  const iconPath = isWindows
    ? path.join(__dirname, 'icons/icon.ico')
    : path.join(__dirname, 'icons/icon.png')

  // Windows/macOS (VS Code approach): keep the native frame so resize edges, Aero
  // snap and maximize/restore stay native; the titlebar is hidden and rendered by
  // our own HTML, made draggable via CSS `-webkit-app-region: drag`.
  // Linux (VS Code approach): frame:false + titleBarStyle:'hidden' +
  // titleBarOverlay + opaque theme bg. Since Chromium now defaults to Wayland on
  // Linux, frameless windows get GTK drop shadows + extended resize boundaries;
  // hasShadow:false removes the shadow/decorations (the white border) while
  // native resize edges remain.
  const nativeFrameOpts = isLinux
    ? {
        frame: false,
        titleBarStyle: 'hidden' as const,
        titleBarOverlay: { height: 32, color: '#1e1e1e', symbolColor: '#ffffff' },
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
  ipcMain.handle('window:getBounds', () => mainWindow?.getBounds() ?? null)
  ipcMain.handle('window:setBounds', (_, bounds: Electron.Rectangle) => mainWindow?.setBounds(bounds))
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
  //  PTY (node-pty)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('spawn_terminal', (_, args: SpawnTerminalArgs) => {
    const { rows, cols, shell } = args
    if (!ptyModule) throw new Error('node-pty not installed')

    const id = crypto.randomUUID()
    const shellCmd = shell || (process.platform === 'win32'
      ? (process.env.ComSpec || 'cmd.exe')
      : (process.env.SHELL || (process.platform === 'darwin' ? 'zsh' : 'bash')))

    const termEnv: NodeJS.ProcessEnv = { ...process.env, TERM: 'xterm-256color' }
    if (!process.env.LANG && !process.env.LC_ALL && !process.env.LC_CTYPE) termEnv.LANG = 'C.UTF-8'

    const term = ptyModule.spawn(shellCmd, [], {
      name: 'xterm-256color',
      cols: cols || 80,
      rows: rows || 24,
      cwd: process.env.HOME || process.env.USERPROFILE || '.',
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
  })

  ipcMain.handle('write_terminal', (_, args: WriteTerminalArgs) => {
    const { sessionId, data } = args
    const term = ptySessions.get(sessionId)
    if (term) { term.write(data); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { ssh.writeCh!.write(data); return }
    const sp = serialSessions.get(sessionId)
    if (sp) { sp.port.write(data); return }
    throw new Error(`Session ${sessionId} not found`)
  })

  ipcMain.handle('resize_terminal', (_, args: ResizeTerminalArgs) => {
    const { sessionId, rows, cols } = args
    const term = ptySessions.get(sessionId)
    if (term) { term.resize(cols, rows); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { ssh.writeCh!.setWindow(rows, cols, rows, cols); return }
  })

  ipcMain.handle('kill_terminal', (_, args: KillTerminalArgs) => {
    const { sessionId } = args
    const term = ptySessions.get(sessionId)
    if (term) { killPty(term); ptySessions.delete(sessionId); return }
    const ssh = sshSessions.get(sessionId)
    if (ssh) { try { ssh.conn.end() } catch {}; sshSessions.delete(sessionId); return }
    const sp = serialSessions.get(sessionId)
    if (sp) { try { sp.port.close() } catch {}; serialSessions.delete(sessionId); return }
  })

  // ══════════════════════════════════════════════════════
  //  SSH (ssh2)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('ssh_connect', (_, args: SshConnectArgs) => {
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
  })

  // ══════════════════════════════════════════════════════
  //  Telnet (net.Socket)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('telnet_connect', (_, args: TelnetConnectArgs) => {
    const { host, port } = args
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
  })

  // ══════════════════════════════════════════════════════
  //  Serial (serialport)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('serial_connect', (_, args: SerialConnectArgs) => {
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
  })

  ipcMain.handle('serial_list_ports', async () => {
    if (!SerialPortClass) return []
    try {
      const ports = await SerialPortClass.list()
      return ports.map((p: { path: string }): SerialPortInfo => ({ port_name: p.path }))
    } catch { return [] }
  })

  // ══════════════════════════════════════════════════════
  //  SFTP (ssh2-sftp-client)
  // ══════════════════════════════════════════════════════
  ipcMain.handle('sftp_connect', async (_, args: SftpConnectArgs) => {
    if (!SftpClientClass) throw new Error('ssh2-sftp-client not installed')

    const { host, port, username, password, privateKeyPath } = args
    const connId = crypto.randomUUID()
    const client = new SftpClientClass('sftp-' + connId)

    const opts: Record<string, unknown> = { host, port: port || 22, username }
    if (privateKeyPath && fs.existsSync(privateKeyPath)) {
      opts.privateKey = fs.readFileSync(privateKeyPath)
    } else if (password) {
      opts.password = password
    }

    await client.connect(opts as Parameters<SftpClient['connect']>[0])
    sftpConnections.set(connId, client)
    return connId
  })

  ipcMain.handle('sftp_disconnect', async (_, args: SftpDisconnectArgs) => {
    const { connId } = args
    const client = sftpConnections.get(connId)
    if (client) {
      try { await client.end() } catch {}
      sftpConnections.delete(connId)
    }
  })

  ipcMain.handle('sftp_list_dir', async (_, args: SftpListDirArgs) => {
    const { connId, path: dirPath } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    const list = await client.list(dirPath || '/')
    return list.map((item): SftpFileEntry => ({
      name: item.name,
      is_dir: item.type === 'd',
      size: item.size || 0,
      modified: item.modifyTime ? new Date(item.modifyTime).toISOString() : '',
      permissions: item.rights ? `${item.rights.user}${item.rights.group}${item.rights.other}` : '',
    }))
  })

  ipcMain.handle('sftp_download', async (_, args: SftpTransferArgs) => {
    const { connId, transferId, remote, local } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')

    const stat = await client.stat(remote)
    const totalSize = stat.size || 0
    let cancelled = false

    const rdr = client.createReadStream(remote)
    const wtr = fs.createWriteStream(local)
    const handle = {
      abort: () => {
        cancelled = true
        rdr.destroy()
        wtr.destroy()
      },
    }
    sftpTransfers.set(transferId, handle)

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
          emitToRenderer('sftp-progress', {
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

  ipcMain.handle('sftp_upload', async (_, args: SftpTransferArgs) => {
    const { connId, transferId, remote, local } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')

    const totalSize = fs.statSync(local).size
    let cancelled = false

    const rdr = fs.createReadStream(local)
    const wtr = client.createWriteStream(remote)
    const handle = {
      abort: () => {
        cancelled = true
        rdr.destroy()
        wtr.destroy()
      },
    }
    sftpTransfers.set(transferId, handle)

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
          emitToRenderer('sftp-progress', {
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

  ipcMain.handle('sftp_cancel_transfer', async (_, args: SftpTransferArgs) => {
    const { transferId } = args
    const handle = sftpTransfers.get(transferId)
    if (!handle) throw new Error('Transfer not found or already completed')
    handle.abort()
  })
  ipcMain.handle('sftp_mkdir', async (_, args: SftpMkdirArgs) => {
    const { connId, path: dirPath } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    await client.mkdir(dirPath)
  })

  ipcMain.handle('sftp_remove', async (_, args: SftpRemoveArgs) => {
    const { connId, path: filePath } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    const stat = await client.stat(filePath)
    if (stat.isDirectory) await client.rmdir(filePath)
    else await client.delete(filePath)
  })

  ipcMain.handle('sftp_rename', async (_, args: SftpRenameArgs) => {
    const { connId, oldPath, newPath } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    await client.rename(oldPath, newPath)
  })

  ipcMain.handle('sftp_create', async (_, args: SftpCreateArgs) => {
    const { connId, path: filePath, isDir, mode } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    if (isDir) {
      await client.mkdir(filePath)
      if (mode) await client.chmod(filePath, mode)
    } else {
      await client.put(Buffer.from(''), filePath)
      if (mode) await client.chmod(filePath, mode)
    }
  })

  ipcMain.handle('sftp_read_file', async (_, args: SftpReadFileArgs) => {
    const { connId, remote } = args
    const client = sftpConnections.get(connId)
    if (!client) throw new Error('SFTP connection not found')
    const buf = await client.get(remote)
    return Buffer.isBuffer(buf) ? buf.toString('utf8') : String(buf)
  })

  ipcMain.handle('sftp_write_file', async (_, args: SftpWriteFileArgs) => {
    const { connId, remote, content } = args
    const client = sftpConnections.get(connId)
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

    const response = await callAiChat(messages, config)

    if (response.tool_calls && response.tool_calls.length > 0) {
      const history = aiHistories.get(sessionId) || []
      history.push({ role: 'assistant', content: response.text || '', tool_calls: response.tool_calls })
      aiHistories.set(sessionId, history)
    }

    return response
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

    const response = await callAiChat(history, config)

    if (response.tool_calls && response.tool_calls.length > 0) {
      const h2 = aiHistories.get(sessionId) || []
      h2.push({ role: 'assistant', content: response.text || '', tool_calls: response.tool_calls })
      aiHistories.set(sessionId, h2)
    }

    return response
  })

  ipcMain.handle('ai_clear_history', (_, args: AiClearHistoryArgs) => {
    const { sessionId } = args
    aiHistories.delete(sessionId)
  })

  ipcMain.handle('fetch_ai_models', async (_, args: FetchAiModelsArgs) => {
    const { provider, baseUrl, apiKey } = args
    try {
      let baseURL = baseUrl.replace(/\/+$/, '')
      if (provider === 'ollama') {
        baseURL = `${baseURL}/v1`
      }

      const OpenAI = (await import('openai')).default
      const client = new OpenAI({ apiKey, baseURL })
      const list = await client.models.list()
      return list.data.map((m) => m.id)
    } catch { return [] }
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

  ipcMain.handle('get_remote_system_info', async (_, args: GetRemoteSystemInfoArgs) => {
    const { sessionId } = args
    const ssh = sshSessions.get(sessionId)
    if (!ssh || !ssh.conn) throw new Error('SSH session not found')

    // Open a fresh exec-only connection to avoid conflicts with the
    // interactive shell channel (some servers drop the socket when
    // exec is requested alongside an open shell).
    const execCmd = (cmd: string, timeoutMs = 5000): Promise<string> =>
      new Promise((resolve) => {
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

  ipcMain.handle('cli_args', () => {
    // Packaged builds put the executable path in argv[0]; dev mode has
    // [electron, main.js, ...]. Also drop macOS Finder's -psn_0_xxx arg.
    const args = process.argv.slice(app.isPackaged ? 1 : 2)
    return args.filter((a) => !a.startsWith('-psn_0_'))
  })
  ipcMain.handle('detect_shells', (): Array<{ name: string; command: string; icon: string }> => {
    const shells: Array<{ name: string; command: string; icon: string }> = []
    const has = (exe: string) => {
      const dirs = (process.env.PATH || '').split(path.delimiter).filter(Boolean)
      for (const dir of dirs) {
        const full = path.join(dir, exe)
        try { fs.accessSync(full, fs.constants.X_OK); return true } catch { /* keep searching */ }
      }
      return false
    }
    if (process.platform === 'win32') {
      shells.push({ name: '命令提示符', command: 'cmd.exe', icon: '\u{1F4DF}' })
      if (fs.existsSync('C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe')) shells.push({ name: 'Windows PowerShell', command: 'powershell.exe', icon: '\u{1F4DF}' })
      if (has('pwsh.exe')) shells.push({ name: 'PowerShell', command: 'pwsh.exe', icon: '\u{1F4DF}' })
      if (has('wsl.exe')) shells.push({ name: 'WSL', command: 'wsl.exe', icon: '\u{1F427}' })
      return shells
    }
    if (process.platform === 'darwin' && has('zsh')) shells.push({ name: 'Zsh', command: 'zsh', icon: '\u{1F334}' })
    if (has('bash')) shells.push({ name: 'Bash', command: 'bash', icon: '\u{1F40D}' })
    if (has('sh')) shells.push({ name: 'Sh', command: 'sh', icon: '\u{1F40D}' })
    if (process.platform !== 'darwin' && has('zsh')) shells.push({ name: 'Zsh', command: 'zsh', icon: '\u{1F334}' })
    if (has('fish')) shells.push({ name: 'Fish', command: 'fish', icon: '\u{1F41F}' })
    return shells
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

async function callAiChat(messages: AiMessage[], config: AiConfig): Promise<AiResponse> {
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
  })

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
