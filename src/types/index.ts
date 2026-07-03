export interface SshConnectionInfo {
  host: string
  port: number
  username: string
  password: string
  privateKeyPath?: string
  proxyId?: string
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

export interface TerminalSession {
  id: string
  title: string
  type: 'local' | 'ssh' | 'serial' | 'telnet'
  status: 'connecting' | 'connected' | 'disconnected'
}

export interface TerminalTab {
  id: string
  title: string
  session: TerminalSession | null
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
  splitDirection?: 'horizontal' | 'vertical'
  children?: TerminalTab[]
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
  private_key_path: string | null
  proxy_id: string | null
  last_connected: string | null
  created_at: string
}

export interface SavedSessionGroup {
  id: string
  name: string
  expanded: boolean
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
