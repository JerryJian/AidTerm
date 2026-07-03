export interface SshConnectionInfo {
  host: string
  port: number
  username: string
  password: string
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
