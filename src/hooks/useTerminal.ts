import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { TerminalOutputPayload } from '../types'

export function useTerminal() {
  const sessionId = ref<string | null>(null)
  const isConnected = ref(false)

  async function createSession() {
    try {
      const id = await invoke<string>('spawn_terminal', {
        rows: 24,
        cols: 80,
      })
      sessionId.value = id
      isConnected.value = true
      return id
    } catch (e) {
      console.error('Failed to create terminal session:', e)
      return null
    }
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
    writeInput,
    resize,
    killSession,
    onOutput,
  }
}
