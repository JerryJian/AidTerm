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
      const id = await invoke<string>('sftp_connect', {
        host, port, username, password,
        privateKeyPath: privateKeyPath ?? null,
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

  async function disconnect(tabId: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    if (tab.kind === 'sftp') {
      await invoke('sftp_disconnect', { connId: tab.connId }).catch(() => {})
    }
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
      if (tab.kind === 'adb') {
        tab.entries = await invoke<FileEntry[]>('adb_list_dir', { serial: tab.connId, path })
      } else {
        tab.entries = await invoke<FileEntry[]>('sftp_list_dir', { connId: tab.connId, path })
      }
      tab.currentPath = path
    } catch (e: any) {
      tab.error = String(e)
    } finally {
      tab.loading = false
    }
  }

  async function download(tabId: string, transferId: string, remote: string, local: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (tab.kind === 'adb') {
      await invoke('adb_pull', { serial: tab.connId, remote, local })
    } else {
      await invoke('sftp_download', { connId: tab.connId, transferId, remote, local })
    }
  }

  async function upload(tabId: string, transferId: string, local: string, remote: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (tab.kind === 'adb') {
      await invoke('adb_push', { serial: tab.connId, local, remote })
    } else {
      await invoke('sftp_upload', { connId: tab.connId, transferId, remote, local })
    }
    await listDir(tabId, tab.currentPath)
  }

  async function cancelTransfer(tabId: string, transferId: string) {
    const tab = tabState(tabId)
    if (!tab.connId || tab.kind === 'adb') return
    await invoke('sftp_cancel_transfer', { connId: tab.connId, transferId })
  }

  async function remove(tabId: string, path: string, isDir = false) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (tab.kind === 'adb') {
      await invoke('adb_remove', { serial: tab.connId, path, is_dir: isDir })
    } else {
      await invoke('sftp_remove', { connId: tab.connId, path })
    }
    await listDir(tabId, tab.currentPath)
  }

  async function renameItem(tabId: string, oldPath: string, newPath: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (tab.kind === 'adb') {
      await invoke('adb_rename', { serial: tab.connId, old_path: oldPath, new_path: newPath })
    } else {
      await invoke('sftp_rename', { connId: tab.connId, oldPath, newPath })
    }
    await listDir(tabId, tab.currentPath)
  }

  async function createFile(tabId: string, path: string, isDir: boolean, mode: number) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    if (tab.kind === 'adb') {
      if (isDir) {
        await invoke('adb_mkdir', { serial: tab.connId, path })
      } else {
        await invoke('adb_touch', { serial: tab.connId, path })
      }
    } else {
      await invoke('sftp_create', { connId: tab.connId, path, isDir, mode })
    }
    await listDir(tabId, tab.currentPath)
  }

  async function readFile(connId: string, remote: string, kind: FileKind = 'sftp'): Promise<string> {
    if (!connId) return ''
    if (kind === 'adb') return await invoke<string>('adb_read_file', { serial: connId, remote })
    return await invoke<string>('sftp_read_file', { connId, remote })
  }

  async function writeFile(connId: string, remote: string, content: string, kind: FileKind = 'sftp') {
    if (!connId) return
    if (kind === 'adb') {
      await invoke('adb_write_file', { serial: connId, remote, content })
    } else {
      await invoke('sftp_write_file', { connId, remote, content })
    }
  }

  return {
    tabState,
    connected, connId, currentPath, entries, loading, error, kind,
    connect, connectAdb, disconnect, listDir, download, upload, cancelTransfer,
    remove, renameItem, createFile, readFile, writeFile,
  }
})
