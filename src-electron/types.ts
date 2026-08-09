import type { Client as SshClient, ClientChannel, ConnectConfig as SshConnectConfig } from 'ssh2'
import type SftpClient from 'ssh2-sftp-client'
import type { Server as NetServer, Socket } from 'net'
import type { SerialPort } from 'serialport'

// ── IPC Handler Argument Types ──

export interface WindowSetFullscreenArgs {
  fullscreen: boolean
}

export interface ClipboardWriteArgs {
  text: string
}

export interface SpawnTerminalArgs {
  rows: number
  cols: number
  shell: string | null
}

export interface WriteTerminalArgs {
  sessionId: string
  data: string
}

export interface ResizeTerminalArgs {
  sessionId: string
  rows: number
  cols: number
}

export interface KillTerminalArgs {
  sessionId: string
}

// ── Unified Connection Types (mirror the Rust/`src/types` ConnectionConfig) ──

export type ConnectionConfig =
  | { type: 'local'; shell?: string | null; working_dir?: string | null }
  | { type: 'wsl'; distro?: string | null; working_dir?: string | null }
  | {
      type: 'ssh'
      host: string
      port: number
      username: string
      password: string
      private_key_path?: string | null
      proxy_id?: string | null
      agent_forwarding?: boolean
      x11_forwarding?: boolean
    }
  | { type: 'telnet'; host: string; port: number }
  | {
      type: 'serial'
      port_name: string
      baud_rate: number
      data_bits: number
      stop_bits: number
      parity: string
      flow_control: string
    }
  | { type: 'adb'; serial: string }

export interface ConnectionHandle {
  id: string
  capabilities: string[]
}

export interface ConnectionCreateArgs {
  config: ConnectionConfig
  rows: number
  cols: number
}

export type FileConnectConfig =
  | { type: 'sftp'; host: string; port: number; username: string; password: string; private_key_path?: string | null }
  | { type: 'adb'; serial: string }
  | { type: 'local' }
  | { type: 'wsl'; distro?: string | null }

export interface FileConnectArgs {
  config: FileConnectConfig
}

export interface FileOpArgs {
  kind: string
  handle: string
}

export interface FileListDirArgs extends FileOpArgs {
  path: string
}

export interface FileTransferArgs extends FileOpArgs {
  transferId: string
  remote: string
  local: string
}

export interface FileRemoveArgs extends FileOpArgs {
  path: string
  is_dir: boolean
}

export interface FileRenameArgs extends FileOpArgs {
  old_path: string
  new_path: string
}

export interface FileMkdirArgs extends FileOpArgs {
  path: string
}

export interface FileCreateArgs extends FileOpArgs {
  path: string
  is_dir: boolean
  mode: number
}

export interface FileReadArgs extends FileOpArgs {
  remote: string
}

export interface FileWriteArgs extends FileOpArgs {
  remote: string
  content: string
}

export interface SshConnectArgs {
  host: string
  port: number
  username: string
  password: string
  privateKeyPath: string | null
  proxyId: string | null
  agentForwarding: boolean
  x11Forwarding: boolean
  rows: number
  cols: number
}

export interface TelnetConnectArgs {
  host: string
  port: number
}

export interface SerialConnectArgs {
  portName: string
  baudRate: number
  dataBits: number
  stopBits: number
  parity: string
  flowControl: string
}

export interface AdbDevice {
  serial: string
  state: string
  model: string
  product: string
  transport_id: string | null
}

export interface AdbConnectArgs {
  serial: string
  rows: number
  cols: number
}

export interface SftpConnectArgs {
  host: string
  port: number
  username: string
  password: string
  privateKeyPath: string | null
}

export interface SftpDisconnectArgs {
  connId: string
}

export interface SftpListDirArgs {
  connId: string
  path: string
}

export interface SftpTransferArgs {
  connId: string
  transferId: string
  remote: string
  local: string
}

export interface SftpMkdirArgs {
  connId: string
  path: string
}

export interface SftpRemoveArgs {
  connId: string
  path: string
}

export interface SftpRenameArgs {
  connId: string
  oldPath: string
  newPath: string
}

export interface SftpCreateArgs {
  connId: string
  path: string
  isDir: boolean
  mode: number
}

export interface SftpReadFileArgs {
  connId: string
  remote: string
}

export interface SftpWriteFileArgs {
  connId: string
  remote: string
  content: string
}

export type TunnelType = 'Local' | 'Remote' | 'Dynamic'

export interface TunnelCreateArgs {
  req: {
    host: string
    port: number
    username: string
    password: string | null
    privateKeyPath: string | null
    tunnel_type: TunnelType
    bind_addr: string
    bind_port: number
    target_host: string | null
    target_port: number | null
  }
}

export interface TunnelRemoveArgs {
  id: string
}

export interface ProxyConfig {
  id: string
  name: string
  proxy_type: 'Http' | 'Socks5' | 'JumpHost'
  host: string
  port: number
  username: string | null
  password: string | null
  private_key_path: string | null
}

export interface ProxySaveArgs {
  config: ProxyConfig
}

export interface ProxyDeleteArgs {
  id: string
}

export interface AiChatArgs {
  sessionId: string
  messages: AiMessage[]
  config: AiConfig
}

export interface AiExecuteArgs {
  command: string
}

export interface AiContinueArgs {
  sessionId: string
  toolCallId: string
  toolResult: string
  config: AiConfig
}

export interface AiClearHistoryArgs {
  sessionId: string
}

export interface FetchAiModelsArgs {
  provider: string
  baseUrl: string
  apiKey: string
}

export interface GetRemoteSystemInfoArgs {
  sessionId: string
}

export interface SaveSessionStoreArgs {
  data: SessionStoreData
}

export interface KeyGenerateRsaArgs {
  name: string
  bits: number
  passphrase: string | null
}

export interface KeyGenerateEd25519Args {
  name: string
  passphrase: string | null
}

export interface KeyDeleteArgs {
  id: string
}

export interface KeyImportArgs {
  name: string
  privateKeyPath: string
}

export interface KnownHostsAddArgs {
  host: string
  keyType: string
  key: string
}

export interface KnownHostsRemoveArgs {
  host: string
  keyType: string
}

// ── Session & State Types ──

export interface PtySession {
  pid: number
  write(data: string): void
  resize(cols: number, rows: number): void
  kill(): void
  onData: (callback: (data: string) => void) => void
  onExit: (callback: (result: { exitCode: number }) => void) => void
}

export interface SshSession {
  conn: SshClient
  stream: ClientChannel | null
  writeCh: ClientChannel | null
  resizeCh: ClientChannel | null
  connectOpts?: SshConnectConfig
}

export interface SerialSession {
  port: {
    write(data: string | Buffer): void
    close(): void
  }
}

export interface TunnelInfo {
  id: string
  tunnel_type: TunnelType
  bind_addr: string
  bind_port: number
  target_host: string | null
  target_port: number | null
  host: string
  port: number
  username: string
  status: string
}

export interface TunnelState {
  info: TunnelInfo
  server: NetServer | null
  conn: SshClient | null
}

// ── Session Store Types ──

export interface SavedSession {
  id: string
  name: string
  session_type: string
  group_id: string | null
  host: string | null
  port: number | null
  username: string | null
  password: string | null
  private_key_path: string | null
  proxy_id: string | null
  last_connected: string | null
  created_at: string
  data_bits: number | null
  stop_bits: number | null
  parity: string | null
  flow_control: string | null
}

export interface SavedSessionGroup {
  id: string
  name: string
  expanded: boolean
}

export interface SessionStoreData {
  groups: SavedSessionGroup[]
  sessions: SavedSession[]
}

// ── Keychain Types ──

export interface KeyInfo {
  id: string
  name: string
  key_type: string
  bits: number
  public_key: string
  fingerprint: string
  private_key_path: string
  public_key_path: string
  created_at: string
}

// ── Known Hosts Types ──

export interface KnownHostEntry {
  host: string
  key_type: string
  fingerprint: string
  line: string
}

// ── AI Types ──

export interface AiMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string
  tool_call_id?: string
  tool_calls?: AiToolCall[]
}

export interface AiToolCall {
  id: string
  function: {
    name: string
    arguments: string
  }
}

export interface AiConfig {
  provider: string
  api_key: string
  model: string
  base_url: string
}

export interface AiResponse {
  text: string | null
  tool_calls: AiToolCall[]
}

// ── File Types ──

export interface FileEntry {
  name: string
  is_dir: boolean
  size: number
  modified: string
  permissions: string
}

export interface FileProgress {
  remote: string
  local: string
  type: string
  bytes_transferred: number
  total_size: number
}

// ── System Info Types ──

export interface SystemInfo {
  os: string
  arch: string
  hostname: string
  kernel: string
  shell: string
}

// ── Dialog Types ──

export interface DialogFilter {
  name: string
  extensions: string[]
}

export interface OpenDialogOpts {
  title?: string
  directory?: boolean
  multiple?: boolean
  filters?: DialogFilter[]
}

export interface SaveDialogOpts {
  title?: string
  filters?: DialogFilter[]
  defaultPath?: string
}

// ── Serial Port Types ──

export interface SerialPortInfo {
  port_name: string
}
