<script setup lang="ts">
import { ref, computed, reactive, nextTick, onMounted, onUnmounted } from 'vue'
import { useTerminalStore } from './stores/terminal'
import { useSessionStore } from './stores/sessionStore'
import { useSettingsStore } from './stores/settingsStore'
import { useUiStore } from './stores/uiStore'
import { useThemeStore } from './stores/themeStore'
import type { SshConnectionInfo, TelnetConnectionInfo, SerialConnectionInfo, SavedSession } from './types'
import { invoke, listen, getCurrentWindow, saveDialog as save } from '@/api'
import TabBar from './components/terminal/TabBar.vue'
import TitleBar from './components/titlebar/TitleBar.vue'
import TerminalPane from './components/terminal/TerminalPane.vue'
import LeftSidebar from './components/sidebar/LeftSidebar.vue'

import StatusBar from './components/status/StatusBar.vue'
import SshDialog from './components/session/SshDialog.vue'
import SerialDialog from './components/session/SerialDialog.vue'
import SessionDialog from './components/session/SessionDialog.vue'
import SettingsDialog from './components/settings/SettingsDialog.vue'
import FileEditor from './components/editor/FileEditor.vue'
import LockScreen from './components/lock/LockScreen.vue'
import { useTriggerWatcher } from './hooks/useTriggerWatcher'

const store = useTerminalStore()
const sessionStore = useSessionStore()
const settings = useSettingsStore()
const ui = useUiStore()
useThemeStore()

const sshDialogPrefill = ref<{ host: string; port: number; username: string; password?: string }>()
const showSessionDialog = ref(false)
const editingSession = ref<SavedSession | undefined>()
const editorFile = ref<{ connId: string; remotePath: string } | null>(null)
const locked = ref(false)
const isFullscreen = ref(false)

const inputCtx = reactive({
  show: false,
  x: 0,
  y: 0,
  el: null as HTMLElement | null,
})

function showInputCtx(e: MouseEvent) {
  e.preventDefault()
  inputCtx.x = e.clientX
  inputCtx.y = e.clientY
  inputCtx.el = e.target as HTMLElement
  inputCtx.show = true
}

function hideInputCtx() {
  inputCtx.show = false
  inputCtx.el = null
}

function onInputCtxDocClick(e: MouseEvent) {
  if (!inputCtx.show) return
  const target = e.target as HTMLElement
  if (target.closest('.input-ctx-menu') || target.closest('.input-ctx-overlay')) return
  hideInputCtx()
}

function onInputCtxKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && inputCtx.show) {
    hideInputCtx()
  }
}

function inputCtxAction(action: 'cut' | 'copy' | 'paste' | 'selectAll') {
  const el = inputCtx.el
  hideInputCtx()
  if (!el) return
  if (action === 'selectAll') {
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
      (el as HTMLInputElement).select()
    } else if (el.isContentEditable) {
      const r = document.createRange()
      r.selectNodeContents(el)
      const s = window.getSelection()
      s?.removeAllRanges()
      s?.addRange(r)
    }
    return
  }
  if (action === 'cut') { document.execCommand('cut'); return }
  if (action === 'copy') { document.execCommand('copy'); return }
  if (action === 'paste') {
    if (document.execCommand('paste')) return
    navigator.clipboard.readText().then(text => {
      if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
        const input = el as HTMLInputElement
        const start = input.selectionStart ?? input.value.length
        const end = input.selectionEnd ?? start
        input.setRangeText(text, start, end, 'end')
        input.dispatchEvent(new Event('input', { bubbles: true }))
      } else if (el.isContentEditable) {
        document.execCommand('insertText', false, text)
      }
    }).catch(() => {})
  }
}

const appStyle = computed(() => {
  const style: Record<string, string> = {}
  if (settings.transparency < 1) {
    style.opacity = String(settings.transparency)
  }
  if (settings.backgroundImage) {
    style.backgroundImage = `url('file:///${settings.backgroundImage.replace(/\\/g, '/')}')`
    style.backgroundSize = 'cover'
    style.backgroundPosition = 'center'
    style.backgroundRepeat = 'no-repeat'
  }
  return style
})

useTriggerWatcher()

if (store.tabs.length === 0) {
  store.addTab('local')
}

function onSshConnect(info: SshConnectionInfo) {
  ui.sshDialog = false
  store.addTab('ssh', info)
}

function onQuickSsh(host: string, port: number, username: string) {
  sshDialogPrefill.value = { host, port, username }
  ui.sshDialog = true
}

function onQuickTelnet(host: string, port: number) {
  const info: TelnetConnectionInfo = { host, port }
  store.addTab('telnet', undefined, info)
}

function onSerialConnect(info: SerialConnectionInfo) {
  ui.serialDialog = false
  store.addTab('serial', undefined, undefined, undefined, info)
}

function onQuickSerial() {
  ui.serialDialog = true
}

function onEditFile(remotePath: string, connId: string) {
  editorFile.value = { connId, remotePath }
}

function onConnectSession(session: SavedSession) {
  if (session.session_type === 'ssh') {
    sshDialogPrefill.value = {
      host: session.host ?? '',
      port: session.port ?? 22,
      username: session.username ?? '',
      password: session.password ?? undefined,
    }
    ui.sshDialog = true
    sessionStore.updateLastConnected(session.id)
  } else if (session.session_type === 'telnet') {
    const info: TelnetConnectionInfo = {
      host: session.host ?? '',
      port: session.port ?? 23,
    }
    store.addTab('telnet', undefined, info)
    sessionStore.updateLastConnected(session.id)
  } else if (session.session_type === 'serial') {
    const info: SerialConnectionInfo = {
      portName: session.host ?? '',
      baudRate: session.port ?? 115200,
      dataBits: session.data_bits ?? 8,
      stopBits: session.stop_bits ?? 1,
      parity: session.parity ?? 'None',
      flowControl: session.flow_control ?? 'None',
    }
    store.addTab('serial', undefined, undefined, undefined, info)
    sessionStore.updateLastConnected(session.id)
  }
}

function onNewSession() {
  editingSession.value = undefined
  showSessionDialog.value = true
}

function onEditSession(session: SavedSession) {
  editingSession.value = session
  showSessionDialog.value = true
}

const leftDragging = ref(false)

function onLeftDividerDown(e: MouseEvent) {
  leftDragging.value = true
  const startX = e.clientX
  const startW = ui.leftSidebarWidth
  function onMove(ev: MouseEvent) {
    const delta = ev.clientX - startX
    const newW = Math.min(Math.max(startW + delta, 200), 500)
    ui.leftSidebarWidth = newW
  }
  function onUp() {
    leftDragging.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

function onSaveSession(data: { name: string; type: 'ssh' | 'telnet' | 'serial'; host: string; port: number; username: string; password: string; savePassword: boolean; groupName: string; dataBits?: number; stopBits?: number; parity?: string; flowControl?: string }) {
  const existing = editingSession.value
  const groupId = sessionStore.ensureGroup(data.groupName)
  const extra: Record<string, any> = {}
  if (data.type === 'serial') {
    extra.data_bits = data.dataBits
    extra.stop_bits = data.stopBits
    extra.parity = data.parity
    extra.flow_control = data.flowControl
  }
  if (existing) {
    sessionStore.updateSession(existing.id, {
      name: data.name,
      session_type: data.type,
      host: data.host,
      port: data.port,
      username: data.username,
      password: data.savePassword ? data.password : null,
      group_id: groupId,
      ...extra,
    })
  } else {
    sessionStore.addSession(data.name, data.type, {
      host: data.host,
      port: data.port,
      username: data.username,
      password: data.savePassword ? data.password : undefined,
      ...extra,
    }, groupId)
  }
  showSessionDialog.value = false
  editingSession.value = undefined
}

function lockApp() { locked.value = true }
function unlockApp() { locked.value = false }

async function toggleFullscreen() {
  const win = getCurrentWindow()
  isFullscreen.value = await win.isFullscreen()
  await win.setFullscreen(!isFullscreen.value)
  isFullscreen.value = !isFullscreen.value
}

async function handleDeepLink(payload: string) {
  try {
    const url = new URL(payload)
    if (url.protocol === 'ssh:') {
      const username = url.username || 'root'
      const host = url.hostname
      const port = parseInt(url.port, 10) || 22
      sshDialogPrefill.value = { host, port, username }
      ui.sshDialog = true
    }
  } catch {
    // ignore invalid urls
  }
}

async function handleCliArgs() {
  try {
    const args = await invoke<string[]>('cli_args')
    if (args && args.length > 0) {
      for (let i = 0; i < args.length; i++) {
        if (args[i] === '--ssh' && i + 1 < args.length) {
          const val = args[i + 1]
          if (val.includes('@')) {
            const [user, hostPort] = val.split('@')
            const [host, portStr] = hostPort.split(':')
            sshDialogPrefill.value = {
              host: host || val,
              port: portStr ? parseInt(portStr, 10) : 22,
              username: user || 'root',
            }
            ui.sshDialog = true
          } else {
            sshDialogPrefill.value = { host: val, port: 22, username: 'root' }
            ui.sshDialog = true
          }
        }
      }
    }
  } catch {
    // ignore if command not available
  }
}

const unlisteners: Array<() => void> = []

onMounted(async () => {
  // show window after Vue has rendered the first frame
  await nextTick()
  try { await getCurrentWindow().show() } catch { /* ignore */ }

  const f11Handler = (e: KeyboardEvent) => {
    if (e.key === 'F11') {
      e.preventDefault()
      toggleFullscreen()
    }
  }
  document.addEventListener('keydown', f11Handler)
  unlisteners.push(() => document.removeEventListener('keydown', f11Handler))

  const f5Handler = (e: KeyboardEvent) => {
    if (e.key === 'F5') {
      e.preventDefault()
    }
  }
  document.addEventListener('keydown', f5Handler)
  unlisteners.push(() => document.removeEventListener('keydown', f5Handler))

  const ctxHandler = (e: MouseEvent) => {
    const el = e.target as HTMLElement
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) {
      showInputCtx(e)
      return
    }
    e.preventDefault()
  }
  document.addEventListener('contextmenu', ctxHandler)
  unlisteners.push(() => document.removeEventListener('contextmenu', ctxHandler))

  document.addEventListener('click', onInputCtxDocClick, true)
  unlisteners.push(() => document.removeEventListener('click', onInputCtxDocClick, true))
  document.addEventListener('keydown', onInputCtxKeydown)
  unlisteners.push(() => document.removeEventListener('keydown', onInputCtxKeydown))

  const un1 = await listen<{ session_id: string }>('zmodem-start', async (event) => {
    const path = await save({ title: 'Save Zmodem file' })
    await invoke('zmodem_respond', {
      sessionId: event.payload.session_id,
      savePath: path,
    })
  })
  unlisteners.push(un1)

  const un2 = await listen<{ session_id: string; error?: string }>('zmodem-end', (event) => {
    if (event.payload.error) {
      console.error('Zmodem error:', event.payload.error)
    }
  })
  unlisteners.push(un2)

  const un3 = await listen<string>('deep-link', (event) => {
    handleDeepLink(event.payload)
  })
  unlisteners.push(un3)

  await handleCliArgs()
})

onUnmounted(() => {
  unlisteners.forEach(fn => fn())
})
</script>

<template>
  <LockScreen v-if="locked" @unlocked="unlockApp" />

  <div class="app" :class="{ 'left-dragging': leftDragging }" :style="appStyle" @contextmenu.prevent>
    <TitleBar @lock="lockApp" />
    <TabBar
      @lock-click="lockApp"
      @quick-ssh="onQuickSsh"
      @quick-telnet="onQuickTelnet"
      @quick-serial="onQuickSerial"
      @connect-session="onConnectSession"
    />
    <div class="content-area">
      <div class="content-body">
        <div v-if="ui.leftSidebar" class="left-sidebar" :style="{ width: ui.leftSidebarWidth + 'px' }">
          <div class="left-sidebar-divider" @mousedown="onLeftDividerDown" />
          <LeftSidebar
            @connect-session="onConnectSession"
            @new-session="onNewSession"
            @edit-session="onEditSession"
            @close="ui.leftSidebar = false"
          />
        </div>
        <div class="terminal-area">
          <FileEditor
            v-if="editorFile"
            :conn-id="editorFile.connId"
            :remote-path="editorFile.remotePath"
            @close="editorFile = null"
          />
          <TerminalPane
            v-for="tab in store.tabs"
            :key="tab.id"
            v-show="store.activeTab && tab.id === store.activeTabId"
            :tab="tab"
            @newSsh="ui.sshDialog = true"
            @edit-file="onEditFile"
          />
        </div>
      </div>
    </div>
    <StatusBar />
  </div>

  <SshDialog
    v-if="ui.sshDialog"
    :initial-host="sshDialogPrefill?.host"
    :initial-port="sshDialogPrefill?.port"
    :initial-username="sshDialogPrefill?.username"
    :initial-password="sshDialogPrefill?.password"
    @connect="onSshConnect"
    @close="ui.sshDialog = false"
  />
  <SerialDialog
    v-if="ui.serialDialog"
    @connect="onSerialConnect"
    @close="ui.serialDialog = false"
  />
  <SettingsDialog
    v-if="ui.settingsDialog"
    @close="ui.settingsDialog = false"
  />
  <SessionDialog
    v-if="showSessionDialog"
    :session="editingSession"
    @save="onSaveSession"
    @close="showSessionDialog = false; editingSession = undefined"
  />

  <Teleport to="body">
    <div v-if="inputCtx.show" class="input-ctx-overlay" @click="hideInputCtx" @contextmenu.prevent="hideInputCtx" />
    <div v-if="inputCtx.show" class="input-ctx-menu" :style="{ left: inputCtx.x + 'px', top: inputCtx.y + 'px' }">
      <button class="ictx-item" @click="inputCtxAction('cut')">{{ $t('editor.cut') }}</button>
      <button class="ictx-item" @click="inputCtxAction('copy')">{{ $t('editor.copy') }}</button>
      <button class="ictx-item" @click="inputCtxAction('paste')">{{ $t('editor.paste') }}</button>
      <div class="ictx-divider" />
      <button class="ictx-item" @click="inputCtxAction('selectAll')">{{ $t('editor.select_all') }}</button>
    </div>
  </Teleport>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html,
body,
#app {
  width: 100%;
  height: 100%;
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

[data-theme="dark"] {
  --bg-base: #1e1e2e;
  --bg-mantle: #181825;
  --bg-surface0: #313244;
  --bg-surface1: #45475a;
  --bg-crust: #11111b;
  --text: #cdd6f4;
  --text-sub0: #a6adc8;
  --text-sub1: #bac2de;
  --text-overlay0: #585b70;
  --text-overlay1: #6c7086;
  --accent: #89b4fa;
  --accent-hover: #74c7ec;
  --accent-glass: rgba(137, 180, 250, 0.15);
  --danger: #f38ba8;
  --success: #a6e3a1;
  --warning: #f9e2af;
  --pink: #f5c2e7;
  --teal: #94e2d5;
  --rosewater: #f5e0dc;
  --overlay: rgba(0, 0, 0, 0.5);
  --overlay-heavy: rgba(24, 24, 37, 0.95);
}

[data-theme="light"] {
  --bg-base: #eff1f5;
  --bg-mantle: #e6e9ef;
  --bg-surface0: #ccd0da;
  --bg-surface1: #bcc0cc;
  --bg-crust: #dce0e8;
  --text: #4c4f69;
  --text-sub0: #6c6f85;
  --text-sub1: #5c5f77;
  --text-overlay0: #9ca0b0;
  --text-overlay1: #8c8fa1;
  --accent: #1e66f5;
  --accent-hover: #2a7cf6;
  --accent-glass: rgba(30, 102, 245, 0.12);
  --danger: #d20f39;
  --success: #40a02b;
  --warning: #df8e1d;
  --pink: #ea76cb;
  --teal: #179299;
  --rosewater: #dc8a78;
  --overlay: rgba(0, 0, 0, 0.3);
  --overlay-heavy: rgba(230, 233, 239, 0.95);
}

[data-theme="dark"],
[data-theme="light"] {
  background: var(--bg-base);
  color: var(--text);
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: var(--bg-mantle);
}

::-webkit-scrollbar-thumb {
  background: var(--bg-surface1);
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: var(--text-overlay0);
}

</style>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
}

.content-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.content-body {
  flex: 1;
  display: flex;
  flex-direction: row;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.left-sidebar {
  display: flex;
  flex-direction: row;
  height: 100%;
  flex-shrink: 0;
  position: relative;
}

.left-sidebar-divider {
  width: 4px;
  cursor: col-resize;
  background: transparent;
  flex-shrink: 0;
  position: relative;
  z-index: 10;
  order: 1;
}

.left-sidebar-divider:hover {
  background: var(--accent-glass);
}

.app.left-dragging {
  user-select: none;
}

.left-sidebar > :not(.left-sidebar-divider) {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.terminal-area {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.input-ctx-overlay {
  position: fixed;
  inset: 0;
  z-index: 99999;
}
.input-ctx-menu {
  position: fixed;
  z-index: 100000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 120px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}
.ictx-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  border: none;
  background: none;
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
  border-radius: 4px;
}
.ictx-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.ictx-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
}
</style>
