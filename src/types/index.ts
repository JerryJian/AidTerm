export interface SshConnectionInfo {
  host: string
  port: number
  username: string
  password: string
  privateKeyPath?: string
  proxyId?: string
  agentForwarding?: boolean
  x11Forwarding?: boolean
  savePassword?: boolean
}

export type ProxyType = 'Http' | 'Socks5' | 'JumpHost'

export interface ProxyConfig {
  id: string
  name: string
  proxy_type: ProxyType
  host: string
  port: number
  username: string | null
  password: string | null
  private_key_path: string | null
}

export interface TelnetConnectionInfo {
  host: string
  port: number
}

export interface SerialConnectionInfo {
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

/** Result of probing for an adb binary: bundled resource, AIDTERM_ADB env
 *  override, adb on PATH, or none (e.g. arm64 builds ship no bundled adb). */
export interface AdbStatus {
  available: boolean
  source: 'env' | 'bundled' | 'path' | 'missing'
  path: string | null
  port: string | null
}

export interface AdbConnectionInfo {
  serial: string
  model?: string
  product?: string
}

export interface WslConnectionInfo {
  distro?: string
  workingDir?: string
}

export interface TerminalSession {
  id: string
  title: string
  type: 'local' | 'wsl' | 'ssh' | 'serial' | 'telnet' | 'adb'
  status: 'connecting' | 'connected' | 'disconnected'
  command?: string
  workingDir?: string
  capabilities?: ConnectionCapability[]
}

export interface SystemInfo {
  os: string
  arch: string
  hostname: string
  kernel: string
  shell: string
}

export interface UpdateInfo {
  current_version: string
  latest_version: string
  has_update: boolean
  release_url: string
  asset_name: string | null
  asset_url: string | null
  published_at: string | null
  body: string | null
  installer_type: string
}

export type ToolTab = 'ai' | 'history' | 'file' | 'tunnel' | 'cast' | 'monitor'

export interface RemoteDiskMetric {
  mount: string
  total_mb: number
  used_mb: number
}

export interface RemoteNetMetric {
  name: string
  rx_bps: number
  tx_bps: number
}

export interface RemoteGpuMetric {
  vendor: string
  name: string
  utilization: number
  mem_total_mb: number
  mem_used_mb: number
  temperature: number
}

export interface RemoteSystemMetrics {
  cpu_percent: number
  cpu_cores: number
  load_1: number
  load_5: number
  load_15: number
  mem_total_mb: number
  mem_used_mb: number
  swap_total_mb: number
  swap_used_mb: number
  disks: RemoteDiskMetric[]
  nets: RemoteNetMetric[]
  gpus: RemoteGpuMetric[]
}

/** A command recorded from terminal output, keyed per terminal pane (tab id). */
export interface CommandHistoryEntry {
  id: string
  command: string
  timestamp: number
}

export type FileKind = 'sftp' | 'adb' | 'local' | 'wsl'

export interface TerminalTab {
  id: string
  title: string
  session: TerminalSession | null
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
  serialInfo?: SerialConnectionInfo
  adbInfo?: AdbConnectionInfo
  wslInfo?: WslConnectionInfo
  splitDirection?: 'horizontal' | 'vertical'
  children?: TerminalTab[]
  systemInfo?: SystemInfo
  toolSidebarOpen?: boolean
  openToolTabs?: ToolTab[]
  activeToolTab?: ToolTab
  aiSessionId?: string
}

export interface TerminalOutputPayload {
  session_id: string
  data: string
}

export interface TerminalResizePayload {
  session_id: string
  rows: number
  cols: number
}

export interface SavedSession {
  id: string
  name: string
  session_type: 'ssh' | 'telnet' | 'serial' | 'local'
  /** Terminal kind for built-in local profiles: `wsl` opens a real WSL session,
   *  anything else opens a plain local shell. Determines the backend config type. */
  terminal_type?: 'local' | 'wsl' | null
  group_id: string | null
  host: string | null
  port: number | null
  username: string | null
  password: string | null
  private_key_path: string | null
  proxy_id: string | null
  last_connected: string | null
  created_at: string
  data_bits?: number | null
  stop_bits?: number | null
  parity?: string | null
  flow_control?: string | null
  command?: string | null
  working_dir?: string | null
  icon?: string | null
  built_in?: boolean
  hidden?: boolean
}

export interface SavedSessionGroup {
  id: string
  name: string
  expanded: boolean
  built_in?: boolean
}

export interface UploadTask {
  id: string
  name: string
  status: 'uploading' | 'done' | 'error' | 'cancelled'
  error?: string
  type: 'upload' | 'download'
  percent?: number
  total_size?: number
  bytes_transferred?: number
  speed?: number
}

export interface FileProgress {
  remote: string
  local: string
  type: 'upload' | 'download'
  bytes_transferred: number
  total_size: number
}

/** A connection's declared capabilities; drives which tool panels are available. */
export type ConnectionCapability = 'file' | 'tunnel' | 'exec' | 'zmodem' | 'cast' | 'monitor'

export type ConnectionType = 'local' | 'wsl' | 'ssh' | 'telnet' | 'serial' | 'adb'

export interface ConnectionHandle {
  id: string
  capabilities: ConnectionCapability[]
}

/** Unified connection creation config (field names mirror the backend's snake_case). */
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

/** Unified file connection target; `id` is the sftp conn id or adb device serial. */
export type FileConnectConfig =
  | { type: 'sftp'; host: string; port: number; username: string; password: string; private_key_path?: string | null }
  | { type: 'adb'; serial: string }
  | { type: 'local' }
  | { type: 'wsl'; distro?: string | null }

export interface FileEntry {
  name: string
  is_dir: boolean
  size: number
  modified: string
  permissions: string
}

export interface SessionStoreData {
  groups: SavedSessionGroup[]
  sessions: SavedSession[]
}

/** A detected shell entry from the backend (`detect_shells`). */
export interface ShellProfile {
  name: string
  command: string
  icon: string
  terminal_type?: 'local' | 'wsl'
}

export type TunnelType = 'Local' | 'Remote' | 'Dynamic'

export type TunnelStatus = 'Starting' | 'Running' | 'Stopped' | { Error: string }

export interface TunnelCreateRequest {
  host: string
  port: number
  username: string
  password: string | null
  private_key_path: string | null
  tunnel_type: TunnelType
  bind_addr: string
  bind_port: number
  target_host: string | null
  target_port: number | null
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
  status: TunnelStatus
}

export interface Snippet {
  id: string
  name: string
  command: string
  sort_order: number
}

export interface Trigger {
  id: string
  name: string
  pattern: string
  response: string
  enabled: boolean
  cooldown_ms: number
  last_fired: number
}

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

export interface ThemeConfig {
  name: string
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  selectionBackground: string
  black: string
  red: string
  green: string
  yellow: string
  blue: string
  magenta: string
  cyan: string
  white: string
  brightBlack: string
  brightRed: string
  brightGreen: string
  brightYellow: string
  brightBlue: string
  brightMagenta: string
  brightCyan: string
  brightWhite: string
}
