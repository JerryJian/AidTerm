<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useSftpStore } from '../../stores/sftpStore'
import { open, save } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import type { FileEntry, TerminalTab, UploadTask } from '../../types'

const svg = (d: string) => `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`

const icons = {
  up: svg('<path d="M5 3h14"/><path d="m18 13-6-6-6 6"/><path d="M12 7v14"/>'),
  refresh: svg('<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>'),
  folderPlus: svg('<path d="M12 10v6"/><path d="M9 13h6"/><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>'),
  upload: svg('<path d="M12 3v12"/><path d="m17 8-5-5-5 5"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>'),
  download: svg('<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>'),
  close: svg('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'),
  edit: svg('<path d="M14.364 13.634a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506l4.013-4.009a1 1 0 0 0-3.004-3.004z"/><path d="M14.487 7.858A1 1 0 0 1 14 7V2"/><path d="M20 19.645V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l2.516 2.516"/><path d="M8 18h1"/>'),
  rename: svg('<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>'),
  delete: svg('<path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>'),
  spinner: svg('<path d="M21 12a9 9 0 1 1-6.219-8.56"/>'),
}

const { t } = useI18n()
const store = useSftpStore()

const props = defineProps<{
  tabId: string
  tab: TerminalTab
}>()

const emit = defineEmits<{
  close: []
  editFile: [remotePath: string, connId: string]
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
const dragOver = ref(false)
const unlistens: Array<() => void> = []
const userDisconnected = ref(false)
const uploadTasks = ref<UploadTask[]>([])
const ctxEntry = ref<FileEntry | null>(null)
const ctxPos = ref<{ x: number; y: number } | null>(null)
const deleteConfirm = ref<FileEntry | null>(null)

let autoConnecting = false

watch(
  () => {
    const s = props.tab.session
    if (!s || s.type !== 'ssh' || !props.tab.sshInfo) return null
    return `${s.id}:${s.status}:${props.tab.sshInfo.host}:${props.tab.sshInfo.port}:${props.tab.sshInfo.username}`
  },
  async (key) => {
    if (autoConnecting || store.connected) return
    if (props.tab.activeToolTab !== 'sftp') return
    if (key && key.includes(':connected:')) {
      const info = props.tab.sshInfo!
      host.value = info.host
      port.value = info.port
      username.value = info.username
      password.value = info.password || ''
      if (host.value.trim()) {
        autoConnecting = true
        connecting.value = true
        try {
          await store.connect(info.host, info.port, info.username, info.password || '')
        } catch { /* ignored */ }
        connecting.value = false
        autoConnecting = false
      }
    }
  },
  { immediate: true },
)

function onRowCtxMenu(e: MouseEvent, entry: FileEntry) {
  e.preventDefault()
  selectedFile.value = entry.name
  ctxEntry.value = entry
  ctxPos.value = { x: e.clientX, y: e.clientY }
}

function closeCtxMenu() {
  ctxEntry.value = null
  ctxPos.value = null
}

const sortedEntries = computed(() => {
  const sorted = [...store.entries]
  sorted.sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  })
  return sorted
})

onMounted(async () => {
  const un = await listen<{ type: string; paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-drop', (event) => {
    const { type, paths } = event.payload
    if (type === 'over' || type === 'enter') {
      dragOver.value = true
    } else {
      dragOver.value = false
    }
    if (type === 'leave') {
      dragOver.value = false
    }
    if (type === 'drop' && paths && paths.length > 0) {
      for (const path of paths) {
        const name = path.split('\\').pop()?.split('/').pop() || 'file'
        const remotePath = store.currentPath.replace(/\/?$/, '/') + name
        store.upload(path, remotePath)
      }
      dragOver.value = false
    }
  })
  unlistens.push(un)
})

onUnmounted(() => {
  unlistens.forEach(fn => fn())
})

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

function handleDisconnect() {
  userDisconnected.value = true
  store.disconnect()
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
  const path = store.currentPath.replace(/\/?$/, '/') + entry.name
  if (entry.is_dir) {
    navigateTo(path)
  } else {
    emit('editFile', path, store.connId ?? '')
  }
}

async function doUpload() {
  const selected = await open({ multiple: true, directory: false })
  if (!selected) return
  const files = Array.isArray(selected) ? selected : [selected]
  for (const file of files) {
    const name = file.split('\\').pop()?.split('/').pop() || 'file'
    const remotePath = store.currentPath.replace(/\/?$/, '/') + name
    const task: UploadTask = { name, status: 'uploading' }
    uploadTasks.value.push(task)
    try {
      await store.upload(file, remotePath)
      task.status = 'done'
    } catch (e: any) {
      task.status = 'error'
      task.error = String(e)
    }
  }
  // auto-clear completed tasks after 5s
  setTimeout(() => {
    uploadTasks.value = uploadTasks.value.filter(t => t.status === 'uploading')
  }, 5000)
}

async function doDownload(entry: FileEntry) {
  const remotePath = store.currentPath.replace(/\/?$/, '/') + entry.name
  const dest = await save({ defaultPath: entry.name })
  if (!dest) return
  await store.download(remotePath, dest)
}

function confirmDelete(entry: FileEntry) {
  deleteConfirm.value = entry
}

function cancelConfirmDelete() {
  deleteConfirm.value = null
}

async function doDelete(entry: FileEntry) {
  deleteConfirm.value = null
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
  if (entry.is_dir) return '\uD83D\uDCC1'
  const ext = entry.name.split('.').pop()?.toLowerCase()
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'ico', 'webp'].includes(ext || '')) return '\uD83D\uDDBC'
  if (['zip', 'tar', 'gz', 'bz2', '7z', 'rar'].includes(ext || '')) return '\uD83D\uDCE6'
  if (['py', 'js', 'ts', 'rs', 'go', 'java', 'c', 'cpp', 'h'].includes(ext || '')) return '\uD83D\uDCC4'
  if (['txt', 'md', 'json', 'xml', 'yml', 'yaml', 'toml', 'ini', 'cfg'].includes(ext || '')) return '\uD83D\uDCDD'
  if (['sh', 'bash', 'zsh', 'bat', 'ps1', 'cmd'].includes(ext || '')) return '\u2699'
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx'].includes(ext || '')) return '\uD83D\uDCCA'
  return '\uD83D\uDCC4'
}
</script>

<template>
  <div class="sftp-panel">
    <div class="panel-header">
      <span class="panel-title">{{ t('sftp.panel_title') }}</span>
      <button class="panel-btn" :title="t('sftp.close')" @click="emit('close')">✕</button>
    </div>

    <!-- Connection form -->
    <div v-if="!store.connected" class="connect-form">
      <input v-model="host" :placeholder="t('common.host')" class="sftp-input" />
      <input v-model="port" type="number" :placeholder="t('common.port')" class="sftp-input sftp-input-sm" />
      <input v-model="username" :placeholder="t('common.username')" class="sftp-input" />
      <input v-model="password" type="password" :placeholder="t('common.password')" class="sftp-input" />
      <button class="connect-btn" :disabled="connecting" @click="doConnect">
        {{ connecting ? t('sftp.connecting') : t('sftp.connect') }}
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
          <button class="tb-btn" :title="t('sftp.go_up')" @click="goUp" v-html="icons.up" />
          <button class="tb-btn" :title="t('sftp.refresh')" @click="store.listDir(store.currentPath)" v-html="icons.refresh" />
          <button class="tb-btn" :title="t('sftp.mkdir')" @click="showNewDir = !showNewDir" v-html="icons.folderPlus" />
          <button class="tb-btn" :title="t('sftp.upload')" @click="doUpload" v-html="icons.upload" />
          <button class="tb-btn" :title="t('sftp.disconnect')" @click="handleDisconnect" v-html="icons.close" />
        </div>
      </div>

      <!-- New dir input -->
      <div v-if="showNewDir" class="inline-form">
        <input v-model="newDirName" :placeholder="t('sftp.folder_name')" @keydown.enter="doMkdir" @keydown.escape="showNewDir = false" />
        <button @click="doMkdir">{{ t('sftp.ok') }}</button>
      </div>

      <!-- Error -->
      <div v-if="store.error" class="error-bar">{{ store.error }}</div>

      <!-- Loading -->
        <div v-if="store.loading" class="loading">{{ t('common.loading') }}...</div>

      <!-- File list -->
      <div
        v-else
        class="file-list"
        :class="{ 'drag-over': dragOver }"
      >
        <div class="file-header">
          <span class="col-name">{{ t('sftp.name') }}</span>
          <span class="col-size">{{ t('sftp.size') }}</span>
          <span class="col-modified">{{ t('sftp.modified') }}</span>
          <span class="col-actions">{{ t('sftp.actions') }}</span>
        </div>

        <!-- Rename inline -->
        <div v-if="renameTarget" class="rename-row">
          <input v-model="renameValue" class="rename-input" @keydown.enter="confirmRename" @keydown.escape="renameTarget = null" />
          <button @click="confirmRename">{{ t('sftp.ok') }}</button>
          <button @click="renameTarget = null">{{ t('common.cancel') }}</button>
        </div>

        <div
          v-for="entry in sortedEntries"
          :key="entry.name"
          class="file-row"
          :class="{ selected: selectedFile === entry.name }"
          @click="selectedFile = entry.name"
          @dblclick="onEntryDblClick(entry)"
          @contextmenu="(e) => onRowCtxMenu(e, entry)"
        >
          <span class="col-name">
            <span class="file-icon">{{ fileIcon(entry) }}</span>
            <span class="file-name">{{ entry.name }}</span>
          </span>
          <span class="col-size">{{ entry.is_dir ? '—' : formatSize(entry.size) }}</span>
          <span class="col-modified">{{ entry.modified }}</span>
          <span class="col-actions">
            <button v-if="!entry.is_dir" class="action-btn" :title="t('sftp.edit')" @click.stop="onEntryDblClick(entry)" v-html="icons.edit" />
            <button class="action-btn" :title="t('sftp.download')" @click.stop="doDownload(entry)" v-html="icons.download" />
            <button class="action-btn" :title="t('sftp.rename')" @click.stop="startRename(entry)" v-html="icons.rename" />
            <button class="action-btn danger" :title="t('sftp.delete')" @click.stop="confirmDelete(entry)" v-html="icons.delete" />
          </span>
        </div>

        <div v-if="sortedEntries.length === 0" class="empty">{{ t('sftp.empty_directory') }}</div>
      </div>

      <!-- Context menu -->
      <div v-if="ctxPos" class="ctx-backdrop" @click="closeCtxMenu" @contextmenu.prevent="closeCtxMenu" />
      <Teleport to="body">
        <div v-if="ctxPos" class="ctx-menu" :style="{ left: ctxPos.x + 'px', top: ctxPos.y + 'px' }">
          <button v-if="!ctxEntry?.is_dir" class="ctx-item" @click="closeCtxMenu; ctxEntry && onEntryDblClick(ctxEntry)"><span v-html="icons.edit" />{{ t('sftp.edit') }}</button>
          <button class="ctx-item" @click="closeCtxMenu; ctxEntry && doDownload(ctxEntry)"><span v-html="icons.download" />{{ t('sftp.download') }}</button>
          <button class="ctx-item" @click="closeCtxMenu; ctxEntry && startRename(ctxEntry)"><span v-html="icons.rename" />{{ t('sftp.rename') }}</button>
          <div class="ctx-divider" />
          <button class="ctx-item danger" @click="closeCtxMenu; ctxEntry && confirmDelete(ctxEntry)"><span v-html="icons.delete" />{{ t('sftp.delete') }}</button>
        </div>
      </Teleport>

      <!-- Drop zone overlay -->
      <div v-if="dragOver" class="drop-zone">
        <span class="drop-label">{{ t('sftp.drop_to_upload') }}</span>
      </div>

      <!-- Delete confirm dialog -->
      <Teleport to="body">
        <div v-if="deleteConfirm" class="confirm-overlay" @click="cancelConfirmDelete">
          <div class="confirm-box" @click.stop>
            <div class="confirm-msg">
              <span class="confirm-icon" v-html="icons.delete" />
              <span>{{ t('sftp.confirm_delete', { name: deleteConfirm.name }) }}</span>
            </div>
            <div class="confirm-actions">
              <button class="btn btn-cancel" @click="cancelConfirmDelete">{{ t('common.cancel') }}</button>
              <button class="btn btn-danger" @click="doDelete(deleteConfirm)">{{ t('common.delete') }}</button>
            </div>
          </div>
        </div>
      </Teleport>

      <!-- Upload progress -->
      <div v-if="uploadTasks.length" class="upload-progress">
        <div v-for="(task, i) in uploadTasks" :key="i" class="upload-task" :class="task.status">
          <span class="task-icon">
            <span v-if="task.status === 'uploading'" class="spinner" v-html="icons.spinner" />
            <span v-else-if="task.status === 'done'" class="check">&#10003;</span>
            <span v-else class="cross">&#10007;</span>
          </span>
          <span class="task-name">{{ task.name }}</span>
          <span v-if="task.status === 'uploading'" class="task-status">{{ t('sftp.uploading') }}</span>
          <span v-else-if="task.status === 'error'" class="task-error" :title="task.error">{{ t('sftp.upload_failed') }}</span>
          <span v-else class="task-done">{{ t('sftp.upload_done') }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sftp-panel {
  height: 100%;
  background: var(--bg-mantle);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.panel-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text);
}

.panel-btn {
  background: none;
  border: 1px solid transparent;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
}

.panel-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.connect-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
}

.sftp-input {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  padding: 6px 10px;
  font-size: 12px;
  outline: none;
}

.sftp-input:focus {
  border-color: var(--accent);
}

.sftp-input-sm {
  width: 80px;
}

.connect-btn {
  background: var(--accent);
  border: none;
  color: var(--bg-base);
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
  position: relative;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--bg-surface0);
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
  color: var(--accent);
  cursor: pointer;
  white-space: nowrap;
}

.crumb:hover {
  color: var(--accent-hover);
}

.crumb-sep {
  color: var(--text-overlay0);
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
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 5px;
  border-radius: 3px;
  font-size: 13px;
}

.tb-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.inline-form {
  display: flex;
  gap: 4px;
  padding: 4px 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.inline-form input {
  flex: 1;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  padding: 4px 8px;
  font-size: 12px;
  outline: none;
}

.inline-form button {
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  cursor: pointer;
  padding: 4px 8px;
  font-size: 12px;
}

.error-bar {
  padding: 6px 12px;
  background: var(--bg-base);
  color: var(--danger);
  font-size: 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.loading {
  padding: 24px;
  text-align: center;
  color: var(--text-overlay0);
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
  color: var(--text-overlay0);
  font-weight: 600;
  border-bottom: 1px solid var(--bg-surface0);
  position: sticky;
  top: 0;
  background: var(--bg-mantle);
}

.file-row {
  display: flex;
  padding: 5px 12px;
  cursor: default;
  align-items: center;
}

.file-row:hover {
  background: var(--bg-base);
}

.file-row.selected {
  background: var(--bg-surface0);
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
  color: var(--text-sub0);
  flex-shrink: 0;
}

.col-modified {
  width: 120px;
  color: var(--text-overlay0);
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
  color: var(--text);
}

.action-btn {
  background: none;
  border: none;
  color: var(--text-overlay0);
  cursor: pointer;
  padding: 1px 3px;
  font-size: 12px;
  border-radius: 3px;
}

.action-btn:hover {
  color: var(--text);
  background: var(--bg-surface1);
}

.action-btn.danger:hover {
  color: var(--danger);
}

.rename-row {
  display: flex;
  padding: 4px 12px;
  gap: 4px;
  background: var(--bg-base);
  border-bottom: 1px solid var(--bg-surface0);
  align-items: center;
}

.rename-input {
  flex: 1;
  background: var(--bg-surface0);
  border: 1px solid var(--accent);
  color: var(--text);
  padding: 3px 6px;
  font-size: 12px;
  outline: none;
}

.rename-row button {
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  cursor: pointer;
  padding: 3px 8px;
  font-size: 11px;
}

.empty {
  padding: 24px 12px;
  text-align: center;
  color: var(--text-overlay0);
  font-style: italic;
}

.drag-over {
  background: rgba(137, 180, 250, 0.08);
  outline: 2px dashed var(--accent);
  outline-offset: -2px;
}

.drop-zone {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--accent-glass);
  z-index: 10;
}

.drop-label {
  font-size: 14px;
  font-weight: 600;
  color: var(--accent);
  pointer-events: none;
}

.upload-progress {
  border-top: 1px solid var(--bg-surface0);
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 120px;
  overflow-y: auto;
  flex-shrink: 0;
}
.upload-task {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
.upload-task .task-icon {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.upload-task .spinner {
  animation: spin 1s linear infinite;
  display: flex;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
.upload-task .check {
  color: var(--green);
  font-weight: bold;
}
.upload-task .cross {
  color: var(--red);
  font-weight: bold;
}
.upload-task .task-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text);
}
.upload-task .task-status {
  color: var(--text-sub0);
  flex-shrink: 0;
}
.upload-task .task-done {
  color: var(--green);
  flex-shrink: 0;
}
.upload-task .task-error {
  color: var(--red);
  flex-shrink: 0;
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>

<style>
.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  background: transparent;
}
.ctx-menu {
  position: fixed;
  z-index: 10000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 140px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}
.ctx-menu .ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  border: none;
  background: none;
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
  border-radius: 4px;
  white-space: nowrap;
}
.ctx-menu .ctx-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.ctx-menu .ctx-item.danger:hover {
  color: var(--danger);
}
.ctx-menu .ctx-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
}

.confirm-overlay {
  position: fixed;
  inset: 0;
  z-index: 99999;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
}
.confirm-box {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  padding: 20px 24px;
  min-width: 280px;
  max-width: 400px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}
.confirm-msg {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
  color: var(--text);
  margin-bottom: 18px;
}
.confirm-icon {
  display: flex;
  color: var(--danger);
  flex-shrink: 0;
}
.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.confirm-actions .btn {
  padding: 6px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}
.confirm-actions .btn-cancel {
  background: var(--bg-surface0);
  color: var(--text);
}
.confirm-actions .btn-cancel:hover {
  background: var(--bg-surface1);
}
.confirm-actions .btn-danger {
  background: var(--danger);
  color: var(--bg-base);
}
.confirm-actions .btn-danger:hover {
  opacity: 0.85;
}
</style>
