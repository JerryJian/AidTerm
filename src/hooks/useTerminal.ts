import { ref } from 'vue'
import { invoke, listen } from '@/api'
import type {
  TerminalOutputPayload,
  SerialConnectionInfo,
  AdbDevice,
  ConnectionConfig,
  ConnectionHandle,
} from '../types'

export function useTerminal() {
  const sessionId = ref<string | null>(null)
  const isConnected = ref(false)

  /** Unified session creation — builds on `connection_create` and returns the backend handle. */
  async function connect(config: ConnectionConfig, rows = 24, cols = 80): Promise<ConnectionHandle> {
    const handle = await invoke<ConnectionHandle>('connection_create', { config, rows, cols })
    sessionId.value = handle.id
    isConnected.value = true
    return handle
  }

  async function createSession(rows = 24, cols = 80, shell?: string, workingDir?: string) {
    return connect({ type: 'local', shell: shell ?? null, working_dir: workingDir ?? null }, rows, cols)
  }

  async function wslConnect(distro?: string, workingDir?: string, rows = 24, cols = 80) {
    return connect({ type: 'wsl', distro: distro ?? null, working_dir: workingDir ?? null }, rows, cols)
  }

  async function sshConnect(
    host: string,
    port: number,
    username: string,
    password: string,
    privateKeyPath?: string,
    proxyId?: string,
    agentForwarding?: boolean,
    x11Forwarding?: boolean,
    rows = 24,
    cols = 80,
  ) {
    return connect(
      {
        type: 'ssh',
        host,
        port,
        username,
        password,
        private_key_path: privateKeyPath ?? null,
        proxy_id: proxyId ?? null,
        agent_forwarding: agentForwarding ?? false,
        x11_forwarding: x11Forwarding ?? false,
      },
      rows,
      cols,
    )
  }

  async function telnetConnect(host: string, port: number) {
    return connect({ type: 'telnet', host, port })
  }

  async function serialConnect(info: SerialConnectionInfo) {
    return connect({
      type: 'serial',
      port_name: info.portName,
      baud_rate: info.baudRate,
      data_bits: info.dataBits,
      stop_bits: info.stopBits,
      parity: info.parity,
      flow_control: info.flowControl,
    })
  }

  async function adbConnect(serial: string, rows = 24, cols = 80) {
    return connect({ type: 'adb', serial }, rows, cols)
  }

  async function listSerialPorts() {
    return await invoke<{ port_name: string }[]>('serial_list_ports')
  }

  async function listAdbDevices() {
    return await invoke<AdbDevice[]>('adb_list_devices')
  }

  async function occupiedAdbDevices() {
    return await invoke<string[]>('adb_occupied_devices')
  }

  async function killAdbServer() {
    try {
      await invoke('adb_kill_server')
    } catch (e) {
      console.error('Failed to kill adb server:', e)
    }
  }

  async function writeInput(data: string) {
    if (!sessionId.value) return
    try {
      await invoke('connection_write', {
        sessionId: sessionId.value,
        data,
      })
    } catch (e) {
      console.error('Failed to write to terminal:', e)
    }
  }

  async function resize(rows: number, cols: number) {
    if (!sessionId.value) return
    try {
      await invoke('connection_resize', {
        sessionId: sessionId.value,
        rows,
        cols,
      })
    } catch (e) {
      console.error('Failed to resize terminal:', e)
    }
  }

  async function killSession() {
    if (!sessionId.value) return
    try {
      await invoke('connection_kill', {
        sessionId: sessionId.value,
      })
      isConnected.value = false
      sessionId.value = null
    } catch (e) {
      console.error('Failed to kill terminal:', e)
    }
  }

  async function onOutput(callback: (data: string) => void) {
    if (!sessionId.value) return
    return await listen<TerminalOutputPayload>('terminal-output', event => {
      if (event.payload.session_id === sessionId.value) {
        callback(event.payload.data)
      }
    })
  }

  return {
    sessionId,
    isConnected,
    connect,
    createSession,
    wslConnect,
    sshConnect,
    telnetConnect,
    serialConnect,
    listSerialPorts,
    adbConnect,
    listAdbDevices,
    occupiedAdbDevices,
    killAdbServer,
    writeInput,
    resize,
    killSession,
    onOutput,
  }
}
