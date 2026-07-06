<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useTerminalStore } from './stores/terminal'
import { useSessionStore } from './stores/sessionStore'
import { useSettingsStore } from './stores/settingsStore'
import { useUiStore } from './stores/uiStore'
import { useThemeStore } from './stores/themeStore'
import type { SshConnectionInfo, TelnetConnectionInfo, SavedSession } from './types'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { save } from '@tauri-apps/plugin-dialog'
import { isRegistered, register, unregister } from '@tauri-apps/plugin-global-shortcut'
import { Splitpanes, Pane } from 'splitpanes'
import 'splitpanes/dist/splitpanes.css'
import TabBar from './components/terminal/TabBar.vue'
import TerminalPane from './components/terminal/TerminalPane.vue'
import SessionPanel from './components/session/SessionPanel.vue'
import ToolPanel from './components/tools/ToolPanel.vue'
import StatusBar from './components/status/StatusBar.vue'
import SshDialog from './components/session/SshDialog.vue'
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

const sshDialogPrefill = ref<{ host: string; port: number; username: string }>()
const showSessionDialog = ref(false)
const editingSession = ref<SavedSession | undefined>()
const editorFile = ref<{ connId: string; remotePath: string } | null>(null)
const locked = ref(false)
const isFullscreen = ref(false)

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

function onEditFile(remotePath: string, connId: string) {
  editorFile.value = { connId, remotePath }
}

function onConnectSession(session: SavedSession) {
  if (session.session_type === 'ssh') {
    sshDialogPrefill.value = {
      host: session.host ?? '',
      port: session.port ?? 22,
      username: session.username ?? '',
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

function onSaveSession(data: { name: string; type: 'ssh' | 'telnet'; host: string; port: number; username: string; groupName: string }) {
  const existing = editingSession.value
  const groupId = sessionStore.ensureGroup(data.groupName)
  if (existing) {
    sessionStore.updateSession(existing.id, {
      name: data.name,
      session_type: data.type,
      host: data.host,
      port: data.port,
      username: data.username,
      group_id: groupId,
    })
  } else {
    sessionStore.addSession(data.name, data.type, {
      host: data.host,
      port: data.port,
      username: data.username,
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
  try {
    if (!(await isRegistered('Ctrl+2'))) {
      await register('Ctrl+2', async () => {
        const win = getCurrentWindow()
        if (await win.isVisible()) {
          await win.hide()
        } else {
          await win.show()
          await win.setFocus()
        }
      })
    }
  } catch {
    // ignore
  }

  const f11Handler = (e: KeyboardEvent) => {
    if (e.key === 'F11') {
      e.preventDefault()
      toggleFullscreen()
    }
  }
  document.addEventListener('keydown', f11Handler)
  unlisteners.push(() => document.removeEventListener('keydown', f11Handler))

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
  try { unregister('Ctrl+2') } catch { /* ignore */ }
})
</script>

<template>
  <LockScreen v-if="locked" @unlocked="unlockApp" />

  <div class="app" :style="appStyle">
    <TabBar
      @lock-click="lockApp"
      @quick-ssh="onQuickSsh"
      @quick-telnet="onQuickTelnet"
    />
    <div class="content-area">
      <Splitpanes>
        <Pane v-if="ui.leftSidebar" :size="ui.leftSidebarPct" :min-size="15" :max-size="50">
          <SessionPanel
            @connect-session="onConnectSession"
            @new-session="onNewSession"
            @edit-session="onEditSession"
            @close="ui.leftSidebar = false"
          />
        </Pane>
        <Pane>
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
          />
          </div>
        </Pane>
        <Pane v-if="ui.rightSidebar" :size="ui.rightSidebarPct" :min-size="17" :max-size="50">
          <ToolPanel
            @edit-file="onEditFile"
          />
        </Pane>
      </Splitpanes>
    </div>
    <StatusBar />
  </div>

  <SshDialog
    v-if="ui.sshDialog"
    :initial-host="sshDialogPrefill?.host"
    :initial-port="sshDialogPrefill?.port"
    :initial-username="sshDialogPrefill?.username"
    @connect="onSshConnect"
    @close="ui.sshDialog = false"
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

.splitpanes__pane {
  display: flex;
  flex-direction: column;
}

.splitpanes--vertical > .splitpanes__splitter {
  background: transparent;
  border: none;
  min-width: 6px;
}
.splitpanes--vertical > .splitpanes__splitter:hover {
  background: var(--accent-glass);
}
.splitpanes__splitter {
  position: relative;
}
.splitpanes--vertical > .splitpanes__splitter::before {
  content: '';
  position: absolute;
  left: 2px;
  top: 0;
  bottom: 0;
  width: 2px;
  background: var(--bg-surface0);
  pointer-events: none;
}
.splitpanes--vertical > .splitpanes__splitter:hover::before {
  background: var(--accent);
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
  min-height: 0;
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
</style>
