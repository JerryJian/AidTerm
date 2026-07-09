import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { FileEntry } from '../types'

export const useSftpStore = defineStore('sftp', () => {
  const connId = ref<string | null>(null)
  const connected = ref(false)
  const currentPath = ref('/')
  const entries = ref<FileEntry[]>([])
  const loading = ref(false)
  const error = ref('')

  async function connect(
    host: string,
    port: number,
    username: string,
    password: string,
    privateKeyPath?: string,
  ) {
    const id = await invoke<string>('sftp_connect', {
      host, port, username, password,
      privateKeyPath: privateKeyPath ?? null,
    })
    connId.value = id
    connected.value = true
    error.value = ''
    await listDir('/')
    return id
  }

  async function disconnect() {
    if (!connId.value) return
    await invoke('sftp_disconnect', { connId: connId.value })
    connId.value = null
    connected.value = false
    entries.value = []
    currentPath.value = '/'
    error.value = ''
  }

  async function listDir(path: string) {
    if (!connId.value) return
    loading.value = true
    error.value = ''
    try {
      entries.value = await invoke<FileEntry[]>('sftp_list_dir', { connId: connId.value, path })
      currentPath.value = path
    } catch (e: any) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function download(transferId: string, remote: string, local: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_download', { connId: connId.value, transferId, remote, local })
  }

  async function upload(transferId: string, local: string, remote: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_upload', { connId: connId.value, transferId, remote, local })
    await listDir(currentPath.value)
  }

  async function cancelTransfer(transferId: string) {
    if (!connId.value) return
    await invoke('sftp_cancel_transfer', { connId: connId.value, transferId })
  }

  async function remove(path: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_remove', { connId: connId.value, path })
    await listDir(currentPath.value)
  }

  async function renameItem(oldPath: string, newPath: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_rename', { connId: connId.value, oldPath, newPath })
    await listDir(currentPath.value)
  }

  async function createFile(path: string, isDir: boolean, mode: number) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_create', { connId: connId.value, path, isDir, mode })
    await listDir(currentPath.value)
  }

  async function mkdir(path: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_mkdir', { connId: connId.value, path })
    await listDir(currentPath.value)
  }

  async function readFile(remote: string): Promise<string> {
    if (!connId.value) return ''
    error.value = ''
    return await invoke<string>('sftp_read_file', { connId: connId.value, remote })
  }

  async function writeFile(remote: string, content: string) {
    if (!connId.value) return
    error.value = ''
    await invoke('sftp_write_file', { connId: connId.value, remote, content })
  }

  return {
    connId, connected, currentPath, entries, loading, error,
    connect, disconnect, listDir, download, upload, cancelTransfer, remove, renameItem, mkdir,
    createFile, readFile, writeFile,
  }
})
