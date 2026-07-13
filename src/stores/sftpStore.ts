import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { invoke } from '@/api'
import type { FileEntry } from '../types'

interface SftpTabState {
  connId: string | null
  connected: boolean
  currentPath: string
  entries: FileEntry[]
  loading: boolean
  error: string
}

export const useSftpStore = defineStore('sftp', () => {
  const tabs = reactive<Record<string, SftpTabState>>({})

  function tabState(tabId: string): SftpTabState {
    if (!tabs[tabId]) {
      tabs[tabId] = {
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

  async function connect(
    tabId: string,
    host: string,
    port: number,
    username: string,
    password: string,
    privateKeyPath?: string,
  ) {
    const tab = tabState(tabId)
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

  async function disconnect(tabId: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    await invoke('sftp_disconnect', { connId: tab.connId }).catch(() => {})
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
      tab.entries = await invoke<FileEntry[]>('sftp_list_dir', { connId: tab.connId, path })
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
    await invoke('sftp_download', { connId: tab.connId, transferId, remote, local })
  }

  async function upload(tabId: string, transferId: string, local: string, remote: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('sftp_upload', { connId: tab.connId, transferId, remote, local })
    await listDir(tabId, tab.currentPath)
  }

  async function cancelTransfer(tabId: string, transferId: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    await invoke('sftp_cancel_transfer', { connId: tab.connId, transferId })
  }

  async function remove(tabId: string, path: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('sftp_remove', { connId: tab.connId, path })
    await listDir(tabId, tab.currentPath)
  }

  async function renameItem(tabId: string, oldPath: string, newPath: string) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('sftp_rename', { connId: tab.connId, oldPath, newPath })
    await listDir(tabId, tab.currentPath)
  }

  async function createFile(tabId: string, path: string, isDir: boolean, mode: number) {
    const tab = tabState(tabId)
    if (!tab.connId) return
    tab.error = ''
    await invoke('sftp_create', { connId: tab.connId, path, isDir, mode })
    await listDir(tabId, tab.currentPath)
  }

  async function readFile(connectionId: string, remote: string): Promise<string> {
    if (!connectionId) return ''
    return await invoke<string>('sftp_read_file', { connId: connectionId, remote })
  }

  async function writeFile(connectionId: string, remote: string, content: string) {
    if (!connectionId) return
    await invoke('sftp_write_file', { connId: connectionId, remote, content })
  }

  return {
    tabState,
    connected, connId, currentPath, entries, loading, error,
    connect, disconnect, listDir, download, upload, cancelTransfer,
    remove, renameItem, createFile, readFile, writeFile,
  }
})
