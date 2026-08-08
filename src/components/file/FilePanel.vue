<script setup lang="ts">
import { ref, computed, watch, reactive, onMounted, onUnmounted } from 'vue'
import { useFileStore } from '../../stores/fileStore'
import { useTerminalStore } from '../../stores/terminal'
import { openDialog as open, saveDialog as save, listen } from '@/api'
import { useI18n } from 'vue-i18n'
import type { FileEntry, TerminalTab, UploadTask, FileProgress, FileKind } from '../../types'

const svg = (d: string) => `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`

const icons = {
  up: svg('<path d="M5 3h14"/><path d="m18 13-6-6-6 6"/><path d="M12 7v14"/>'),
  refresh: svg('<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>'),
  folderPlus: svg('<path d="M12 10v6"/><path d="M9 13h6"/><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>'),
  filePlus: svg('<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="12" y1="18" x2="12" y2="12"/><line x1="9" y1="15" x2="15" y2="15"/>'),
  upload: svg('<path d="M12 3v12"/><path d="m17 8-5-5-5 5"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>'),
  download: svg('<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>'),
  close: svg('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'),
  edit: svg('<path d="M14.364 13.634a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506l4.013-4.009a1 1 0 0 0-3.004-3.004z"/><path d="M14.487 7.858A1 1 0 0 1 14 7V2"/><path d="M20 19.645V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l2.516 2.516"/><path d="M8 18h1"/>'),
  rename: svg('<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>'),
  delete: svg('<path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>'),
  spinner: svg('<path d="M21 12a9 9 0 1 1-6.219-8.56"/>'),
  more: svg('<circle cx="12" cy="5" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="12" cy="19" r="1"/>'),
}

const { t } = useI18n()
const store = useFileStore()
const terminalStore = useTerminalStore()

const props = defineProps<{
  tabId: string
  tab: TerminalTab
  visible?: boolean
}>()

const emit = defineEmits<{
  editFile: [remotePath: string, connId: string, kind: FileKind]
}>()

const s = computed(() => store.tabState(props.tabId))

const sessionTab = computed(() => terminalStore.resolveSessionTab(props.tab))

const isAdb = computed(() => sessionTab.value?.session?.type === 'adb')

const host = ref('')
const port = ref(22)
const username = ref('')
const password = ref('')
const connecting = ref(false)
const selectedFile = ref<string | null>(null)
const showCreateDialog = ref(false)
const createName = ref('')
const createIsDir = ref(true)
const createPerms = reactive({
  owner_r: true, owner_w: true, owner_x: true,
  group_r: true, group_w: false, group_x: false,
  other_r: true, other_w: false, other_x: false,
})
const renameEntry = ref<FileEntry | null>(null)
const renameValue = ref('')
const showRenameDialog = ref(false)
const dragOver = ref(false)
const unlistens: Array<() => void> = []

const uploadTasks = ref<UploadTask[]>([])
const downloadTasks = ref<UploadTask[]>([])
const ctxEntry = ref<FileEntry | null>(null)
const ctxPos = ref<{ x: number; y: number } | null>(null)
const deleteConfirm = ref<FileEntry | null>(null)
const pathInput = ref('')
const rowMenuEntry = ref<FileEntry | null>(null)
const speedTracker = new Map<string, { time: number; bytes: number }>()

const allTasks = computed(() => [...uploadTasks.value, ...downloadTasks.value])
const rowMenuPos = ref({ x: 0, y: 0 })

function closeRowMenu() {
  rowMenuEntry.value = null
}

function toggleRowMenu(entry: FileEntry, e: MouseEvent) {
  if (rowMenuEntry.value === entry) {
    closeRowMenu()
    return
  }
  rowMenuEntry.value = entry
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  rowMenuPos.value = { x: window.innerWidth - rect.right, y: rect.bottom }
}

watch(() => s.value.currentPath, (p) => { pathInput.value = p }, { immediate: true })

function onPathSubmit() {
  const p = pathInput.value.trim() || '/'
  store.listDir(props.tabId, p)
}

let autoConnecting = false

async function autoConnect() {
  if (autoConnecting || s.value.connected) return
  const leaf = sessionTab.value
  const ss = leaf?.session
  if (!ss || ss.status !== 'connected') return
  if (ss.type === 'adb' && leaf.adbInfo?.serial) {
    autoConnecting = true
    connecting.value = true
    try {
      await store.connectAdb(props.tabId, leaf.adbInfo.serial)
    } catch (e: any) {
      s.value.error = String(e)
    }
    connecting.value = false
    autoConnecting = false
    return
  }
  const info = leaf?.sshInfo
  if (ss.type !== 'ssh' || !info) return
  host.value = info.host
  port.value = info.port
  username.value = info.username
  password.value = info.password || ''
  if (!host.value.trim()) return
  autoConnecting = true
  connecting.value = true
  try {
    await store.connect(props.tabId, info.host, info.port, info.username, info.password || '')
  } catch { /* ignored */ }
  connecting.value = false
  autoConnecting = false
}

watch(
  () => {
    const leaf = sessionTab.value
    const s = leaf?.session
    if (!s) return null
    if (s.type === 'ssh' && leaf?.sshInfo) {
      return `${s.id}:${s.status}:ssh:${leaf.sshInfo.host}:${leaf.sshInfo.port}:${leaf.sshInfo.username}`
    }
    if (s.type === 'adb' && leaf?.adbInfo) {
      return `${s.id}:${s.status}:adb:${leaf.adbInfo.serial}`
    }
    return null
  },
  async (key) => {
    if (props.visible === false) return
    if (key && key.includes(':connected:')) await autoConnect()
  },
  { immediate: true },
)

watch(() => props.visible, (v) => {
  if (v) autoConnect()
})

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
  const sorted = [...s.value.entries]
  sorted.sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  })
  return sorted
})

onMounted(async () => {
  const un1 = await listen<{ type: string; paths?: string[]; position?: { x: number; y: number } }>('tauri://drag-drop', (event) => {
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
        const remotePath = s.value.currentPath.replace(/\/?$/, '/') + name
        store.upload(props.tabId, genId(), path, remotePath)
      }
      dragOver.value = false
    }
  })
  unlistens.push(un1)

  const un2 = await listen<FileProgress>('file-progress', (event) => {
    const p = event.payload
    const tasks = p.type === 'download' ? downloadTasks : uploadTasks
    const name = p.remote.split('/').pop() || p.remote
    const task = tasks.value.find(t => t.name === name)
    if (task) {
      task.percent = Math.round((p.bytes_transferred / p.total_size) * 100)
      task.bytes_transferred = p.bytes_transferred
      task.total_size = p.total_size
      const now = performance.now()
      const prev = speedTracker.get(name)
      if (prev && prev.bytes > 0) {
        const dt = (now - prev.time) / 1000
        if (dt > 0) {
          const db = p.bytes_transferred - prev.bytes
          task.speed = Math.round(db / dt)
        }
      }
      speedTracker.set(name, { time: now, bytes: p.bytes_transferred })
    }
  })
  unlistens.push(un2)
})

onUnmounted(() => {
  unlistens.forEach(fn => fn())
})

function formatSize(size: number): string {
  if (size < 1024) return `${size}B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)}KB`
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)}MB`
  return `${(size / (1024 * 1024 * 1024)).toFixed(1)}GB`
}
function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec < 1024) return `${bytesPerSec}B/s`
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)}KB/s`
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)}MB/s`
}

function parentDir(path: string): string {
  const p = path.replace(/\/$/, '')
  const idx = p.lastIndexOf('/')
  return idx <= 0 ? '/' : p.slice(0, idx)
}

async function doConnect() {
  if (!host.value.trim()) return
  connecting.value = true
  try {
    await store.connect(props.tabId, host.value.trim(), port.value, username.value, password.value)
  } catch (e: any) {
    s.value.error = String(e)
  } finally {
    connecting.value = false
  }
}

function navigateTo(path: string) {
  store.listDir(props.tabId, path)
}

function goUp() {
  navigateTo(parentDir(s.value.currentPath))
}

function onEntryDblClick(entry: FileEntry) {
  const path = s.value.currentPath.replace(/\/?$/, '/') + entry.name
  if (entry.is_dir) {
    navigateTo(path)
  } else {
    emit('editFile', path, s.value.connId ?? '', isAdb.value ? 'adb' : 'sftp')
  }
}

function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}

async function doUpload() {
  const selected = await open({ multiple: true, directory: false })
  if (!selected) return
  const files = Array.isArray(selected) ? selected : [selected]
  for (const file of files) {
    const name = file.split('\\').pop()?.split('/').pop() || 'file'
    const remotePath = s.value.currentPath.replace(/\/?$/, '/') + name
    const id = genId()
    const task: UploadTask = { id, name, status: 'uploading', type: 'upload' }
    uploadTasks.value.push(task)
    try {
      await store.upload(props.tabId, id, file, remotePath)
      task.status = 'done'
    } catch (e: any) {
      if (String(e).includes('Cancelled')) {
        task.status = 'cancelled'
      } else {
        task.status = 'error'
        task.error = String(e)
      }
    }
    // auto-clear completed/cancelled tasks after 5s
    setTimeout(() => {
      uploadTasks.value = uploadTasks.value.filter(t => t.status === 'uploading')
    }, 5000)
  }
}

async function doDownload(entry: FileEntry) {
  const remotePath = s.value.currentPath.replace(/\/?$/, '/') + entry.name
  const dest = await save({ defaultPath: entry.name })
  if (!dest) return
  const id = genId()
  const task: UploadTask = { id, name: entry.name, status: 'uploading', type: 'download' }
  downloadTasks.value.push(task)
  try {
    await store.download(props.tabId, id, remotePath, dest)
    task.status = 'done'
  } catch (e: any) {
    if (String(e).includes('Cancelled')) {
      task.status = 'cancelled'
    } else {
      task.status = 'error'
      task.error = String(e)
    }
  }
  setTimeout(() => {
    downloadTasks.value = downloadTasks.value.filter(t => t.status === 'uploading')
  }, 5000)
}

function doCancel(task: UploadTask) {
  task.status = 'cancelled'
  store.cancelTransfer(props.tabId, task.id).catch(() => {})
  setTimeout(() => {
    const list = task.type === 'upload' ? uploadTasks : downloadTasks
    list.value = list.value.filter(t => t.id !== task.id)
  }, 1500)
}

function confirmDelete(entry: FileEntry) {
  deleteConfirm.value = entry
}

function cancelConfirmDelete() {
  deleteConfirm.value = null
}

async function doDelete(entry: FileEntry) {
  deleteConfirm.value = null
  const path = s.value.currentPath.replace(/\/?$/, '/') + entry.name
  await store.remove(props.tabId, path, entry.is_dir)
}

function startRename(entry: FileEntry) {
  renameEntry.value = entry
  renameValue.value = entry.name
  showRenameDialog.value = true
}

async function confirmRename() {
  if (!renameEntry.value || !renameValue.value.trim()) return
  const oldPath = s.value.currentPath.replace(/\/?$/, '/') + renameEntry.value.name
  const newPath = s.value.currentPath.replace(/\/?$/, '/') + renameValue.value.trim()
  await store.renameItem(props.tabId, oldPath, newPath)
  showRenameDialog.value = false
  renameEntry.value = null
  renameValue.value = ''
}

function permsToMode(): number {
  const p = createPerms
  let mode = 0
  if (p.owner_r) mode |= 0o400
  if (p.owner_w) mode |= 0o200
  if (p.owner_x) mode |= 0o100
  if (p.group_r) mode |= 0o040
  if (p.group_w) mode |= 0o020
  if (p.group_x) mode |= 0o010
  if (p.other_r) mode |= 0o004
  if (p.other_w) mode |= 0o002
  if (p.other_x) mode |= 0o001
  return mode
}

function openCreateDialog(isDir: boolean) {
  createName.value = ''
  createIsDir.value = isDir
  // default perms: 755 for dirs, 644 for files
  createPerms.owner_r = true
  createPerms.owner_w = true
  createPerms.owner_x = isDir
  createPerms.group_r = true
  createPerms.group_w = false
  createPerms.group_x = isDir
  createPerms.other_r = true
  createPerms.other_w = false
  createPerms.other_x = isDir
  showCreateDialog.value = true
}

async function doCreateItem() {
  if (!createName.value.trim()) return
  const path = s.value.currentPath.replace(/\/?$/, '/') + createName.value.trim()
  const mode = permsToMode()
  await store.createFile(props.tabId, path, createIsDir.value, mode)
  createName.value = ''
  showCreateDialog.value = false
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
    <!-- Connection form -->
    <div v-if="!s.connected" class="connect-form">
      <div v-if="isAdb" class="adb-notice">{{ t('adb_file.not_connected') }}</div>
      <template v-else>
        <input v-model="host" :placeholder="t('common.host')" class="sftp-input" />
        <input v-model="port" type="number" :placeholder="t('common.port')" class="sftp-input sftp-input-sm" />
        <input v-model="username" :placeholder="t('common.username')" class="sftp-input" />
        <input v-model="password" type="password" :placeholder="t('common.password')" class="sftp-input" />
        <button class="connect-btn" :disabled="connecting" @click="doConnect">
          {{ connecting ? t('sftp.connecting') : t('sftp.connect') }}
        </button>
      </template>
    </div>

    <!-- File browser -->
    <div v-else class="file-browser">
      <!-- Toolbar -->
      <div class="toolbar">
        <input v-model="pathInput" class="path-input" @keydown.enter="onPathSubmit" @blur="onPathSubmit" />
        <div class="toolbar-actions">
          <button class="tb-btn" :title="t('sftp.go_up')" @click="goUp" v-html="icons.up" />
          <button class="tb-btn" :title="t('sftp.refresh')" @click="store.listDir(props.tabId, s.currentPath)" v-html="icons.refresh" />
          <button class="tb-btn" :title="t('sftp.mkdir')" @click="openCreateDialog(true)" v-html="icons.folderPlus" />
          <button class="tb-btn" :title="t('sftp.new_file')" @click="openCreateDialog(false)" v-html="icons.filePlus" />
          <button class="tb-btn" :title="t('sftp.upload')" @click="doUpload" v-html="icons.upload" />
        </div>
      </div>

      <!-- Error -->
      <div v-if="s.error" class="error-bar">{{ s.error }}</div>

      <!-- Loading -->
        <div v-if="s.loading" class="loading">{{ t('common.loading') }}...</div>

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
            <button class="row-menu-btn" @click.stop="(e) => toggleRowMenu(entry, e)" v-html="icons.more" />
            <Teleport to="body">
              <div v-if="rowMenuEntry === entry" class="row-menu" :style="{ right: rowMenuPos.x + 'px', top: rowMenuPos.y + 'px' }" @click.stop>
                <button v-if="!entry.is_dir" class="row-menu-item" @click="closeRowMenu(); onEntryDblClick(entry)"><span v-html="icons.edit" />{{ t('sftp.edit') }}</button>
                <button class="row-menu-item" @click="closeRowMenu(); doDownload(entry)"><span v-html="icons.download" />{{ t('sftp.download') }}</button>
                <button class="row-menu-item" @click="closeRowMenu(); startRename(entry)"><span v-html="icons.rename" />{{ t('sftp.rename') }}</button>
                <div class="row-menu-divider" />
                <button class="row-menu-item danger" @click="closeRowMenu(); confirmDelete(entry)"><span v-html="icons.delete" />{{ t('sftp.delete') }}</button>
              </div>
            </Teleport>
          </span>
        </div>

        <div v-if="sortedEntries.length === 0" class="empty">{{ t('sftp.empty_directory') }}</div>
      </div>

      <!-- Backdrop for row menu & context menu -->
      <div v-if="ctxPos || rowMenuEntry" class="ctx-backdrop" @click="closeCtxMenu(); closeRowMenu()" @contextmenu.prevent="closeCtxMenu(); closeRowMenu()" />
      <Teleport to="body">
        <div v-if="ctxPos" class="ctx-menu" :style="{ left: ctxPos.x + 'px', top: ctxPos.y + 'px' }">
          <button v-if="!ctxEntry?.is_dir" class="ctx-item" @click="(async () => { const e = ctxEntry; closeCtxMenu(); e && onEntryDblClick(e) })()"><span v-html="icons.edit" />{{ t('sftp.edit') }}</button>
          <button class="ctx-item" @click="(async () => { const e = ctxEntry; closeCtxMenu(); e && doDownload(e) })()"><span v-html="icons.download" />{{ t('sftp.download') }}</button>
          <button class="ctx-item" @click="(async () => { const e = ctxEntry; closeCtxMenu(); e && startRename(e) })()"><span v-html="icons.rename" />{{ t('sftp.rename') }}</button>
          <div class="ctx-divider" />
          <button class="ctx-item danger" @click="(async () => { const e = ctxEntry; closeCtxMenu(); e && confirmDelete(e) })()"><span v-html="icons.delete" />{{ t('sftp.delete') }}</button>
        </div>
      </Teleport>

      <!-- Drop zone overlay -->
      <div v-if="dragOver" class="drop-zone">
        <span class="drop-label">{{ t('sftp.drop_to_upload') }}</span>
      </div>

      <!-- Delete confirm dialog -->
      <Teleport to="body">
        <div v-if="deleteConfirm" class="confirm-overlay">
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

      <!-- Create dialog -->
      <Teleport to="body">
        <div v-if="showCreateDialog" class="confirm-overlay">
          <div class="confirm-box create-box" @click.stop>
            <div class="confirm-msg">{{ t('sftp.create_title', [createIsDir ? t('sftp.dir') : t('sftp.file')]) }}</div>
            <div class="create-name-row">
              <input v-model="createName" class="create-name-input" :placeholder="t('sftp.enter_name')" @keydown.enter="doCreateItem" />
            </div>
            <div class="create-type-row">
              <label><input type="radio" v-model="createIsDir" :value="true" /> {{ t('sftp.dir') }}</label>
              <label><input type="radio" v-model="createIsDir" :value="false" /> {{ t('sftp.file') }}</label>
            </div>
            <div class="create-perms" v-if="!isAdb">
              <div class="perm-row perm-header">
                <span class="perm-label"></span>
                <span class="perm-col">r</span>
                <span class="perm-col">w</span>
                <span class="perm-col">x</span>
              </div>
              <div class="perm-row">
                <span class="perm-label">{{ t('sftp.owner') }}</span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.owner_r" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.owner_w" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.owner_x" /></span>
              </div>
              <div class="perm-row">
                <span class="perm-label">{{ t('sftp.group') }}</span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.group_r" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.group_w" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.group_x" /></span>
              </div>
              <div class="perm-row">
                <span class="perm-label">{{ t('sftp.other') }}</span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.other_r" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.other_w" /></span>
                <span class="perm-col"><input type="checkbox" v-model="createPerms.other_x" /></span>
              </div>
              <div class="perm-mode">chmod {{ permsToMode().toString(8) }}</div>
            </div>
            <div class="confirm-actions">
              <button class="btn btn-cancel" @click="showCreateDialog = false">{{ t('common.cancel') }}</button>
              <button class="btn btn-primary" @click="doCreateItem" :disabled="!createName.trim()">{{ t('sftp.ok') }}</button>
            </div>
          </div>
        </div>
      </Teleport>

      <!-- Rename dialog -->
      <Teleport to="body">
        <div v-if="showRenameDialog" class="confirm-overlay">
          <div class="confirm-box create-box" @click.stop>
            <div class="confirm-msg">{{ t('sftp.rename_title') }}</div>
            <div class="create-name-row">
              <input v-model="renameValue" class="create-name-input" :placeholder="t('sftp.enter_name')" @keydown.enter="confirmRename" />
            </div>
            <div class="confirm-actions">
              <button class="btn btn-cancel" @click="showRenameDialog = false; renameEntry = null">{{ t('common.cancel') }}</button>
              <button class="btn btn-primary" @click="confirmRename" :disabled="!renameValue.trim()">{{ t('sftp.ok') }}</button>
            </div>
          </div>
        </div>
      </Teleport>

      <!-- Transfer progress -->
      <div v-if="allTasks.length" class="upload-progress">
        <div v-for="(task, i) in allTasks" :key="i" class="upload-task" :class="task.status">
          <div class="task-row">
            <span class="task-icon">
              <span v-if="task.status === 'uploading'" class="spinner" v-html="icons.spinner" />
              <span v-else-if="task.status === 'done'" class="check">&#10003;</span>
              <span v-else-if="task.status === 'cancelled'" class="cross">&#10007;</span>
              <span v-else class="cross">&#10007;</span>
            </span>
            <span class="task-dir">{{ task.type === 'upload' ? '↑' : '↓' }}</span>
            <span class="task-name">{{ task.name }}</span>
            <span v-if="task.status === 'uploading' && task.total_size !== undefined" class="task-transfer-info">
              <span v-if="task.speed" class="task-speed">{{ formatSpeed(task.speed) }}</span>
              {{ formatSize(task.bytes_transferred ?? 0) }}/{{ formatSize(task.total_size) }}
            </span>
            <span v-else-if="task.status === 'error'" class="task-error" :title="task.error">{{ t('sftp.upload_failed') }}</span>
            <span v-else-if="task.status === 'cancelled'" class="task-cancelled">{{ t('sftp.cancelled') }}</span>
            <span v-else class="task-done">{{ t('sftp.upload_done') }}</span>
            <span v-if="task.status === 'uploading'" class="task-pct">{{ task.percent ?? 0 }}%</span>
            <button v-if="task.status === 'uploading'" class="task-cancel-btn" @click="doCancel(task)" :title="t('common.cancel')">✕</button>
          </div>
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

.connect-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
}

.adb-notice {
  color: var(--text-sub0);
  font-size: 12px;
  line-height: 1.6;
  padding: 4px 2px;
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

.path-input {
  flex: 1;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  border-radius: 3px;
  padding: 3px 8px;
  font-size: 12px;
  font-family: var(--font-mono, 'Consolas', 'Cascadia Code', monospace);
  color: var(--text);
  outline: none;
  min-width: 0;
}
.path-input:focus {
  border-color: var(--accent);
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
  width: 36px;
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

.row-menu-btn {
  background: none;
  border: none;
  color: var(--text-overlay0);
  cursor: pointer;
  padding: 2px 4px;
  border-radius: 3px;
  line-height: 1;
  opacity: 0.5;
}
.file-row:hover .row-menu-btn,
.row-menu-btn:focus-visible {
  opacity: 1;
}
.row-menu-btn:hover {
  color: var(--text);
  background: var(--bg-surface1);
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
  padding: 6px 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 140px;
  overflow-y: auto;
  flex-shrink: 0;
}
.upload-task {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.upload-task .task-row {
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
.upload-task .task-dir {
  width: 12px;
  text-align: center;
  color: var(--text-sub0);
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
  min-width: 0;
}
.upload-task .task-transfer-info {
  color: var(--text-sub0);
  flex-shrink: 0;
  white-space: nowrap;
}
.upload-task .task-speed {
  margin: 0 8px;
  color: var(--text-overlay0);
  width: 60px;
  display: inline-block;
  text-align: right;
}
.upload-task .task-done {
  color: var(--green);
  flex-shrink: 0;
}
.upload-task .task-cancelled {
  color: var(--text-overlay0);
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
.upload-task .task-pct {
  width: 22px;
  text-align: right;
  color: var(--text-sub0);
  flex-shrink: 0;
}
.upload-task .task-cancel-btn {
  background: none;
  border: none;
  color: var(--text-overlay0);
  cursor: pointer;
  padding: 0 2px;
  font-size: 11px;
  line-height: 1;
  flex-shrink: 0;
  opacity: 0.5;
}
.upload-task .task-cancel-btn:hover {
  opacity: 1;
  color: var(--danger);
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
.confirm-actions .btn-primary {
  background: var(--accent);
  color: var(--bg-base);
}
.confirm-actions .btn-primary:hover {
  opacity: 0.85;
}
.confirm-actions .btn-primary:disabled {
  opacity: 0.4;
  cursor: default;
}
.create-box {
  min-width: 300px;
}
.create-name-row {
  margin-bottom: 12px;
}
.create-name-input {
  width: 100%;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  padding: 6px 10px;
  font-size: 13px;
  outline: none;
  border-radius: 4px;
  box-sizing: border-box;
}
.create-name-input:focus {
  border-color: var(--accent);
}
.create-type-row {
  display: flex;
  gap: 16px;
  margin-bottom: 12px;
  font-size: 13px;
}
.create-type-row label {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  color: var(--text);
}
.create-perms {
  margin-bottom: 12px;
}
.perm-row {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  margin-bottom: 2px;
}
.perm-row .perm-label {
  width: 48px;
  flex-shrink: 0;
  color: var(--text-sub0);
}
.perm-col {
  width: 28px;
  flex-shrink: 0;
  text-align: center;
}
.perm-header .perm-col {
  color: var(--text-overlay0);
  font-weight: 600;
}
.perm-row input[type="checkbox"] {
  width: 14px;
  height: 14px;
  cursor: pointer;
  display: block;
  margin: 0 auto;
}
.perm-mode {
  margin-top: 4px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  color: var(--text-overlay0);
}
.row-menu {
  position: fixed;
  z-index: 10001;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 130px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}
.row-menu-item {
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
.row-menu-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.row-menu-item.danger:hover {
  color: var(--danger);
}
.row-menu-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
}
</style>
