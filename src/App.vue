<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useTerminalStore } from './stores/terminal'
import { useSessionStore } from './stores/sessionStore'
import { useSettingsStore } from './stores/settingsStore'
import type { SshConnectionInfo, TelnetConnectionInfo, SavedSession } from './types'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { save } from '@tauri-apps/plugin-dialog'
import { isRegistered, register, unregister } from '@tauri-apps/plugin-global-shortcut'
import TabBar from './components/terminal/TabBar.vue'
import TerminalPane from './components/terminal/TerminalPane.vue'
import SshDialog from './components/session/SshDialog.vue'
import QuickConnectBar from './components/session/QuickConnectBar.vue'
import SessionPanel from './components/session/SessionPanel.vue'
import SftpPanel from './components/sftp/SftpPanel.vue'
import TunnelPanel from './components/tunnel/TunnelPanel.vue'
import ProxyPanel from './components/proxy/ProxyPanel.vue'
import SnippetPanel from './components/snippet/SnippetPanel.vue'
import TriggerPanel from './components/trigger/TriggerPanel.vue'
import FileEditor from './components/editor/FileEditor.vue'
import SettingsPanel from './components/settings/SettingsPanel.vue'
import LockScreen from './components/lock/LockScreen.vue'
import { useTriggerWatcher } from './hooks/useTriggerWatcher'

const store = useTerminalStore()
const sessionStore = useSessionStore()
const settings = useSettingsStore()

// Panel visibility
const sshDialogVisible = ref(false)
const quickConnectVisible = ref(false)
const sessionPanelVisible = ref(false)
const sftpPanelVisible = ref(false)
const tunnelPanelVisible = ref(false)
const proxyPanelVisible = ref(false)
const snippetPanelVisible = ref(false)
const triggerPanelVisible = ref(false)
const settingsPanelVisible = ref(false)
const sshDialogPrefill = ref<{ host: string; port: number; username: string }>()
const editorFile = ref<{ connId: string; remotePath: string } | null>(null)

// Lock screen state
const locked = ref(false)

// Transparency & background style
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

// Fullscreen state
const isFullscreen = ref(false)

useTriggerWatcher()

if (store.tabs.length === 0) {
  store.addTab('local')
}

function onSshConnect(info: SshConnectionInfo) {
  sshDialogVisible.value = false
  store.addTab('ssh', info)
}

function onQuickSsh(host: string, port: number, username: string) {
  sshDialogPrefill.value = { host, port, username }
  sshDialogVisible.value = true
}

function onQuickTelnet(host: string, port: number) {
  quickConnectVisible.value = false
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
    sshDialogVisible.value = true
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

function lockApp() {
  locked.value = true
}

function unlockApp() {
  locked.value = false
}

async function toggleFullscreen() {
  const win = getCurrentWindow()
  isFullscreen.value = await win.isFullscreen()
  await win.setFullscreen(!isFullscreen.value)
  isFullscreen.value = !isFullscreen.value
}

async function handleDeepLink(payload: string) {
  // ssh://user@host:port
  try {
    const url = new URL(payload)
    if (url.protocol === 'ssh:') {
      const username = url.username || 'root'
      const host = url.hostname
      const port = parseInt(url.port, 10) || 22
      sshDialogPrefill.value = { host, port, username }
      sshDialogVisible.value = true
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
            sshDialogVisible.value = true
          } else {
            sshDialogPrefill.value = { host: val, port: 22, username: 'root' }
            sshDialogVisible.value = true
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
  // Guake mode - global shortcut
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
    // ignore registration errors in dev
  }

  // Fullscreen F11 handler
  const f11Handler = (e: KeyboardEvent) => {
    if (e.key === 'F11') {
      e.preventDefault()
      toggleFullscreen()
    }
  }
  document.addEventListener('keydown', f11Handler)
  unlisteners.push(() => document.removeEventListener('keydown', f11Handler))

  // Zmodem events
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

  // Deep link
  const un3 = await listen<string>('deep-link', (event) => {
    handleDeepLink(event.payload)
  })
  unlisteners.push(un3)

  // CLI args
  await handleCliArgs()
})

onUnmounted(() => {
  unlisteners.forEach(fn => fn())
  // Unregister global shortcut
  try {
    unregister('Ctrl+2')
  } catch {
    // ignore
  }
})
</script>

<template>
  <LockScreen v-if="locked" @unlocked="unlockApp" />

  <div class="app" :style="appStyle">
    <TabBar
      @ssh-click="quickConnectVisible = !quickConnectVisible"
      @sessions-click="sessionPanelVisible = !sessionPanelVisible"
      @sftp-click="sftpPanelVisible = !sftpPanelVisible"
      @tunnel-click="tunnelPanelVisible = !tunnelPanelVisible"
      @proxy-click="proxyPanelVisible = !proxyPanelVisible"
      @snippet-click="snippetPanelVisible = !snippetPanelVisible"
      @trigger-click="triggerPanelVisible = !triggerPanelVisible"
      @settings-click="settingsPanelVisible = !settingsPanelVisible"
      @lock-click="lockApp"
    />
    <QuickConnectBar
      :visible="quickConnectVisible"
      @ssh-connect="onQuickSsh"
      @telnet-connect="onQuickTelnet"
      @close="quickConnectVisible = false"
    />
    <div class="content-area">
      <SessionPanel
        v-if="sessionPanelVisible"
        @connect-session="onConnectSession"
        @close="sessionPanelVisible = false"
      />
      <div class="terminal-area">
        <FileEditor
          v-if="editorFile"
          :conn-id="editorFile.connId"
          :remote-path="editorFile.remotePath"
          @close="editorFile = null"
        />
        <TerminalPane
          v-else-if="store.activeTab"
          :key="store.activeTab.id"
          :tab="store.activeTab"
          @newSsh="sshDialogVisible = true"
        />
      </div>
      <TriggerPanel
        v-if="triggerPanelVisible"
        @close="triggerPanelVisible = false"
      />
      <SnippetPanel
        v-if="snippetPanelVisible"
        @close="snippetPanelVisible = false"
      />
      <ProxyPanel
        v-if="proxyPanelVisible"
        @close="proxyPanelVisible = false"
      />
      <TunnelPanel
        v-if="tunnelPanelVisible"
        @close="tunnelPanelVisible = false"
      />
      <SettingsPanel
        v-if="settingsPanelVisible"
        @close="settingsPanelVisible = false"
      />
      <SftpPanel
        v-if="sftpPanelVisible"
        @edit-file="onEditFile"
        @close="sftpPanelVisible = false"
      />
    </div>
  </div>
  <SshDialog
    v-if="sshDialogVisible"
    :initial-host="sshDialogPrefill?.host"
    :initial-port="sshDialogPrefill?.port"
    :initial-username="sshDialogPrefill?.username"
    @connect="onSshConnect"
    @close="sshDialogVisible = false"
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
  background: #1e1e2e;
  color: #cdd6f4;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: #181825;
}

::-webkit-scrollbar-thumb {
  background: #45475a;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #585b70;
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
