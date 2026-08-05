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

export interface TerminalSession {
  id: string
  title: string
  type: 'local' | 'ssh' | 'serial' | 'telnet'
  status: 'connecting' | 'connected' | 'disconnected'
  command?: string
  workingDir?: string
}

export interface SystemInfo {
  os: string
  arch: string
  hostname: string
  kernel: string
  shell: string
}

export type ToolTab = 'ai' | 'sftp' | 'tunnel'

export interface TerminalTab {
  id: string
  title: string
  session: TerminalSession | null
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
  serialInfo?: SerialConnectionInfo
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

export interface SftpProgress {
  remote: string
  local: string
  type: 'upload' | 'download'
  bytes_transferred: number
  total_size: number
}

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
