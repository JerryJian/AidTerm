import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { TerminalOutputPayload, SerialConnectionInfo } from '../types'

export function useTerminal() {
  const sessionId = ref<string | null>(null)
  const isConnected = ref(false)

  async function createSession(rows = 24, cols = 80, shell?: string) {
    const id = await invoke<string>('spawn_terminal', { rows, cols, shell: shell ?? null })
    sessionId.value = id
    isConnected.value = true
    return id
  }

  async function telnetConnect(host: string, port: number) {
    const id = await invoke<string>('telnet_connect', { host, port })
    sessionId.value = id
    isConnected.value = true
    return id
  }

  async function serialConnect(info: SerialConnectionInfo) {
    const id = await invoke<string>('serial_connect', {
      portName: info.portName,
      baudRate: info.baudRate,
      dataBits: info.dataBits,
      stopBits: info.stopBits,
      parity: info.parity,
      flowControl: info.flowControl,
    })
    sessionId.value = id
    isConnected.value = true
    return id
  }

  async function listSerialPorts() {
    return await invoke<{ port_name: string }[]>('serial_list_ports')
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
    const id = await invoke<string>('ssh_connect', {
      host,
      port,
      username,
      password,
      privateKeyPath: privateKeyPath ?? null,
      proxyId: proxyId ?? null,
      agentForwarding: agentForwarding ?? false,
      x11Forwarding: x11Forwarding ?? false,
      rows,
      cols,
    })
    sessionId.value = id
    isConnected.value = true
    return id
  }

  async function writeInput(data: string) {
    if (!sessionId.value) return
    try {
      await invoke('write_terminal', {
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
      await invoke('resize_terminal', {
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
      await invoke('kill_terminal', {
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
    createSession,
    sshConnect,
    telnetConnect,
    serialConnect,
    listSerialPorts,
    writeInput,
    resize,
    killSession,
    onOutput,
  }
}
