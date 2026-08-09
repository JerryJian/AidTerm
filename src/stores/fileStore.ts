import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { invoke } from '@/api'
import type { FileEntry, FileKind } from '../types'

interface FileTabState {
  kind: FileKind
  connId: string | null
  connected: boolean
  currentPath: string
  entries: FileEntry[]
  loading: boolean
  error: string
}

export const useFileStore = defineStore('file', () => {
  const tabs = reactive<Record<string, FileTabState>>({})

  function tabState(tabId: string): FileTabState {
    if (!tabs[tabId]) {
      tabs[tabId] = {
        kind: 'sftp',
        connId: null,
        connected: false,
        currentPath: '/',
        entries: [],
        loading: false,
        error: '',
      }
    }
    return tabs[tabId]
  }

  function connected(tabId: string) { return tabState(tabId).connected }
  function connId(tabId: string) { return tabState(tabId).connId }
  function currentPath(tabId: string) { return tabState(tabId).currentPath }
  function entries(tabId: string) { return tabState(tabId).entries }
  function loading(tabId: string) { return tabState(tabId).loading }
  function error(tabId: string) { return tabState(tabId).error }
  function kind(tabId: string) { return tabState(tabId).kind }

  async function connect(
    tabId: string,
    host: string,
    port: number,
    username: string,
    password: string,
    privateKeyPath?: string,
  ) {
    const tab = tabState(tabId)
    tab.kind = 'sftp'
    tab.loading = true
    try {
      const id = await invoke<string>('file_connect', {
        config: {
          type: 'sftp',
          host, port, username, password,
          private_key_path: privateKeyPath ?? null,
        },
      })
      tab.connId = id
      tab.connected = true
      tab.error = ''
      await listDir(tabId, '/')
      return id
    } finally {
      tab.loading = false
    }
  }

  async function connectAdb(tabId: string, serial: string) {
    const tab = tabState(tabId)
    if (tab.connected && tab.kind === 'adb' && tab.connId === serial) return
    tab.kind = 'adb'
    tab.connId = serial
    tab.connected = true
    tab.error = ''
    await listDir(tabId, '/sdcard')
  }

  async function connectLocal(tabId: string) {
    const tab = tabState(tabId)
    if (tab.connected && tab.kind === 'local') return
    tab.kind = 'local'
    tab.connId = 'local'
    tab.connected = true
    tab.error = ''
    const home = await invoke<string>('file_home_dir')
    await listDir(tabId, home)
  }

  async function connectWsl(tabId: string, distro = '') {
    const tab = tabState(tabId)
    if (tab.connected && tab.kind === 'wsl' && tab.connId === distro) return
    if (!distro) {
      try {
        const list = await invoke<string[]>('wsl_list_distros')
        distro = list[0] ?? ''
      } catch { /* ignore */ }
    }
    if (!distro) {
      throw new Error('WSL not found')
    }
    tab.kind = 'wsl'
    tab.connId = distro
    tab.connected = true
    tab.error = ''
    await listDir(tabId, '/')
  }

  async function disconnect(tabId: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    await invoke('file_disconnect', { kind: tab.kind, handle: tab.connId }).catch(() => {})
    tab.connId = null
    tab.connected = false
    tab.entries = []
    tab.currentPath = '/'
    tab.error = ''
  }

  async function listDir(tabId: string, path: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.loading = true
    tab.error = ''
    try {
      tab.entries = await invoke<FileEntry[]>('file_list_dir', { kind: tab.kind, handle: tab.connId, path })
      tab.currentPath = path
    } catch (e: unknown) {
      tab.error = String(e)
    } finally {
      tab.loading = false
    }
  }

  async function download(tabId: string, transferId: string, remote: string, local: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('file_download', { kind: tab.kind, handle: tab.connId, transferId, remote, local })
  }

  async function upload(tabId: string, transferId: string, local: string, remote: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('file_upload', { kind: tab.kind, handle: tab.connId, transferId, remote, local })
    await listDir(tabId, tab.currentPath)
  }

  async function cancelTransfer(tabId: string, transferId: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    await invoke('file_cancel_transfer', { kind: tab.kind, handle: tab.connId, transferId })
  }

  async function remove(tabId: string, path: string, isDir = false) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('file_remove', { kind: tab.kind, handle: tab.connId, path, is_dir: isDir })
    await listDir(tabId, tab.currentPath)
  }

  async function renameItem(tabId: string, oldPath: string, newPath: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('file_rename', { kind: tab.kind, handle: tab.connId, old_path: oldPath, new_path: newPath })
    await listDir(tabId, tab.currentPath)
  }

  async function createFile(tabId: string, path: string, isDir: boolean, mode: number) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (isDir) {
      await invoke('file_mkdir', { kind: tab.kind, handle: tab.connId, path })
    } else {
      await invoke('file_create', { kind: tab.kind, handle: tab.connId, path, is_dir: false, mode })
    }
    await listDir(tabId, tab.currentPath)
  }

  async function readFile(connId: string, remote: string, kind: FileKind = 'sftp'): Promise<string> {
    if (!connId) return ''
    return await invoke<string>('file_read', { kind, handle: connId, remote })
  }

  async function writeFile(connId: string, remote: string, content: string, kind: FileKind = 'sftp') {
    if (!connId) return
    await invoke('file_write', { kind, handle: connId, remote, content })
  }

  return {
    tabState,
    connected, connId, currentPath, entries, loading, error, kind,
    connect, connectAdb, connectLocal, connectWsl, disconnect, listDir, download, upload, cancelTransfer,
    remove, renameItem, createFile, readFile, writeFile,
  }
})
