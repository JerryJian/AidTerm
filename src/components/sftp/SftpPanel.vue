<script setup lang="ts">
import { ref, computed } from 'vue'
import { useSftpStore } from '../../stores/sftpStore'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { FileEntry } from '../../types'

const store = useSftpStore()

const emit = defineEmits<{
  close: []
}>()

const host = ref('')
const port = ref(22)
const username = ref('')
const password = ref('')
const connecting = ref(false)
const selectedFile = ref<string | null>(null)
const newDirName = ref('')
const showNewDir = ref(false)
const renameTarget = ref<FileEntry | null>(null)
const renameValue = ref('')

function formatSize(size: number): string {
  if (size < 1024) return `${size}`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)}K`
  return `${(size / (1024 * 1024)).toFixed(1)}M`
}

function parentDir(path: string): string {
  const p = path.replace(/\/$/, '')
  const idx = p.lastIndexOf('/')
  return idx <= 0 ? '/' : p.slice(0, idx)
}

function breadcrumbParts(path: string): { name: string; full: string }[] {
  const parts = path.replace(/\/$/, '').split('/').filter(Boolean)
  const crumbs = [{ name: '~', full: '/' }]
  let acc = ''
  for (const p of parts) {
    acc += '/' + p
    crumbs.push({ name: p, full: acc })
  }
  return crumbs
}

async function doConnect() {
  if (!host.value.trim()) return
  connecting.value = true
  try {
    await store.connect(host.value.trim(), port.value, username.value, password.value)
  } catch (e: any) {
    store.error = String(e)
  } finally {
    connecting.value = false
  }
}

function navigateTo(path: string) {
  store.listDir(path)
}

function goUp() {
  navigateTo(parentDir(store.currentPath))
}

function onEntryDblClick(entry: FileEntry) {
  if (entry.is_dir) {
    const path = store.currentPath.replace(/\/?$/, '/') + entry.name
    navigateTo(path)
  }
}

async function doUpload() {
  const selected = await open({ multiple: false, directory: false })
  if (!selected) return
  const name = selected.split('\\').pop()?.split('/').pop() || 'file'
  const remotePath = store.currentPath.replace(/\/?$/, '/') + name
  await store.upload(selected, remotePath)
}

async function doDownload(entry: FileEntry) {
  const remotePath = store.currentPath.replace(/\/?$/, '/') + entry.name
  const dest = await save({ defaultPath: entry.name })
  if (!dest) return
  await store.download(remotePath, dest)
}

async function doDelete(entry: FileEntry) {
  const path = store.currentPath.replace(/\/?$/, '/') + entry.name
  await store.remove(path)
}

function startRename(entry: FileEntry) {
  renameTarget.value = entry
  renameValue.value = entry.name
}

async function confirmRename() {
  if (!renameTarget.value || !renameValue.value.trim()) return
  const oldPath = store.currentPath.replace(/\/?$/, '/') + renameTarget.value.name
  const newPath = store.currentPath.replace(/\/?$/, '/') + renameValue.value.trim()
  await store.renameItem(oldPath, newPath)
  renameTarget.value = null
  renameValue.value = ''
}

async function doMkdir() {
  if (!newDirName.value.trim()) return
  const path = store.currentPath.replace(/\/?$/, '/') + newDirName.value.trim()
  await store.mkdir(path)
  newDirName.value = ''
  showNewDir.value = false
}

function fileIcon(entry: FileEntry): string {
  if (entry.is_dir) return '📁'
  const ext = entry.name.split('.').pop()?.toLowerCase()
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'ico', 'webp'].includes(ext || '')) return '🖼'
  if (['zip', 'tar', 'gz', 'bz2', '7z', 'rar'].includes(ext || '')) return '📦'
  if (['py', 'js', 'ts', 'rs', 'go', 'java', 'c', 'cpp', 'h'].includes(ext || '')) return '📄'
  if (['txt', 'md', 'json', 'xml', 'yml', 'yaml', 'toml', 'ini', 'cfg'].includes(ext || '')) return '📝'
  if (['sh', 'bash', 'zsh', 'bat', 'ps1', 'cmd'].includes(ext || '')) return '⚙'
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx'].includes(ext || '')) return '📊'
  return '📄'
}
</script>

<template>
  <div class="sftp-panel">
    <div class="panel-header">
      <span class="panel-title">SFTP</span>
      <button class="panel-btn" title="Close" @click="emit('close')">✕</button>
    </div>

    <!-- Connection form -->
    <div v-if="!store.connected" class="connect-form">
      <input v-model="host" placeholder="Host" class="sftp-input" />
      <input v-model="port" type="number" placeholder="Port" class="sftp-input sftp-input-sm" />
      <input v-model="username" placeholder="Username" class="sftp-input" />
      <input v-model="password" type="password" placeholder="Password" class="sftp-input" />
      <button class="connect-btn" :disabled="connecting" @click="doConnect">
        {{ connecting ? 'Connecting...' : 'Connect' }}
      </button>
    </div>

    <!-- File browser -->
    <div v-else class="file-browser">
      <!-- Toolbar -->
      <div class="toolbar">
        <span class="breadcrumb">
          <span v-for="(crumb, i) in breadcrumbParts(store.currentPath)" :key="i" class="crumb" @click="navigateTo(crumb.full)">
            {{ crumb.name }}<span v-if="i < breadcrumbParts(store.currentPath).length - 1" class="crumb-sep">/</span>
          </span>
        </span>
        <div class="toolbar-actions">
          <button class="tb-btn" title="Go Up" @click="goUp">⬆</button>
          <button class="tb-btn" title="Refresh" @click="store.listDir(store.currentPath)">🔄</button>
          <button class="tb-btn" title="New Folder" @click="showNewDir = !showNewDir">📁+</button>
          <button class="tb-btn" title="Upload" @click="doUpload">⬆</button>
          <button class="tb-btn" title="Disconnect" @click="store.disconnect">✕</button>
        </div>
      </div>

      <!-- New dir input -->
      <div v-if="showNewDir" class="inline-form">
        <input v-model="newDirName" placeholder="Folder name" @keydown.enter="doMkdir" @keydown.escape="showNewDir = false" />
        <button @click="doMkdir">OK</button>
      </div>

      <!-- Error -->
      <div v-if="store.error" class="error-bar">{{ store.error }}</div>

      <!-- Loading -->
      <div v-if="store.loading" class="loading">Loading...</div>

      <!-- File list -->
      <div v-else class="file-list">
        <div class="file-header">
          <span class="col-name">Name</span>
          <span class="col-size">Size</span>
          <span class="col-modified">Modified</span>
          <span class="col-actions">Actions</span>
        </div>

        <!-- Rename inline -->
        <div v-if="renameTarget" class="rename-row">
          <input v-model="renameValue" class="rename-input" @keydown.enter="confirmRename" @keydown.escape="renameTarget = null" />
          <button @click="confirmRename">OK</button>
          <button @click="renameTarget = null">Cancel</button>
        </div>

        <div
          v-for="entry in store.entries"
          :key="entry.name"
          class="file-row"
          :class="{ selected: selectedFile === entry.name }"
          @click="selectedFile = entry.name"
          @dblclick="onEntryDblClick(entry)"
        >
          <span class="col-name">
            <span class="file-icon">{{ fileIcon(entry) }}</span>
            <span class="file-name">{{ entry.name }}</span>
          </span>
          <span class="col-size">{{ entry.is_dir ? '—' : formatSize(entry.size) }}</span>
          <span class="col-modified">{{ entry.modified }}</span>
          <span class="col-actions">
            <button class="action-btn" title="Download" @click.stop="doDownload(entry)">⬇</button>
            <button class="action-btn" title="Rename" @click.stop="startRename(entry)">✏</button>
            <button class="action-btn danger" title="Delete" @click.stop="doDelete(entry)">🗑</button>
          </span>
        </div>

        <div v-if="store.entries.length === 0" class="empty">Empty directory</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sftp-panel {
  width: 400px;
  min-width: 300px;
  background: #181825;
  border-left: 1px solid #313244;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid #313244;
}

.panel-title {
  font-weight: 600;
  font-size: 13px;
  color: #cdd6f4;
}

.panel-btn {
  background: none;
  border: 1px solid transparent;
  color: #a6adc8;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
}

.panel-btn:hover {
  background: #313244;
  color: #cdd6f4;
}

.connect-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
}

.sftp-input {
  background: #1e1e2e;
  border: 1px solid #45475a;
  color: #cdd6f4;
  padding: 6px 10px;
  font-size: 12px;
  outline: none;
}

.sftp-input:focus {
  border-color: #89b4fa;
}

.sftp-input-sm {
  width: 80px;
}

.connect-btn {
  background: #89b4fa;
  border: none;
  color: #1e1e2e;
  cursor: pointer;
  padding: 8px;
  font-weight: 600;
  border-radius: 4px;
  margin-top: 4px;
}

.connect-btn:disabled {
  opacity: 0.5;
}

.file-browser {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid #313244;
  gap: 8px;
}

.breadcrumb {
  flex: 1;
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
  font-size: 12px;
  overflow: hidden;
}

.crumb {
  color: #89b4fa;
  cursor: pointer;
  white-space: nowrap;
}

.crumb:hover {
  color: #74c7ec;
}

.crumb-sep {
  color: #585b70;
  margin: 0 1px;
}

.toolbar-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}

.tb-btn {
  background: none;
  border: 1px solid transparent;
  color: #a6adc8;
  cursor: pointer;
  padding: 2px 5px;
  border-radius: 3px;
  font-size: 13px;
}

.tb-btn:hover {
  background: #313244;
  color: #cdd6f4;
}

.inline-form {
  display: flex;
  gap: 4px;
  padding: 4px 12px;
  border-bottom: 1px solid #313244;
}

.inline-form input {
  flex: 1;
  background: #1e1e2e;
  border: 1px solid #45475a;
  color: #cdd6f4;
  padding: 4px 8px;
  font-size: 12px;
  outline: none;
}

.inline-form button {
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  cursor: pointer;
  padding: 4px 8px;
  font-size: 12px;
}

.error-bar {
  padding: 6px 12px;
  background: #1e1e2e;
  color: #f38ba8;
  font-size: 12px;
  border-bottom: 1px solid #313244;
}

.loading {
  padding: 24px;
  text-align: center;
  color: #585b70;
  font-size: 13px;
}

.file-list {
  flex: 1;
  overflow-y: auto;
  font-size: 12px;
}

.file-header {
  display: flex;
  padding: 6px 12px;
  color: #585b70;
  font-weight: 600;
  border-bottom: 1px solid #313244;
  position: sticky;
  top: 0;
  background: #181825;
}

.file-row {
  display: flex;
  padding: 5px 12px;
  cursor: default;
  align-items: center;
}

.file-row:hover {
  background: #1e1e2e;
}

.file-row.selected {
  background: #313244;
}

.col-name {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  overflow: hidden;
}

.col-size {
  width: 70px;
  text-align: right;
  color: #a6adc8;
  flex-shrink: 0;
}

.col-modified {
  width: 120px;
  color: #585b70;
  flex-shrink: 0;
  text-align: right;
}

.col-actions {
  width: 80px;
  display: flex;
  gap: 2px;
  justify-content: flex-end;
  flex-shrink: 0;
}

.file-icon {
  font-size: 12px;
  width: 18px;
  text-align: center;
}

.file-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #cdd6f4;
}

.action-btn {
  background: none;
  border: none;
  color: #585b70;
  cursor: pointer;
  padding: 1px 3px;
  font-size: 12px;
  border-radius: 3px;
}

.action-btn:hover {
  color: #cdd6f4;
  background: #45475a;
}

.action-btn.danger:hover {
  color: #f38ba8;
}

.rename-row {
  display: flex;
  padding: 4px 12px;
  gap: 4px;
  background: #1e1e2e;
  border-bottom: 1px solid #313244;
  align-items: center;
}

.rename-input {
  flex: 1;
  background: #313244;
  border: 1px solid #89b4fa;
  color: #cdd6f4;
  padding: 3px 6px;
  font-size: 12px;
  outline: none;
}

.rename-row button {
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  cursor: pointer;
  padding: 3px 8px;
  font-size: 11px;
}

.empty {
  padding: 24px 12px;
  text-align: center;
  color: #585b70;
  font-style: italic;
}
</style>
