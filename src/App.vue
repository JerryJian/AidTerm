<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useTerminalStore } from './stores/terminal'
import { useSessionStore } from './stores/sessionStore'
import type { SshConnectionInfo, TelnetConnectionInfo, SavedSession } from './types'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { save } from '@tauri-apps/plugin-dialog'
import TabBar from './components/terminal/TabBar.vue'
import TerminalPane from './components/terminal/TerminalPane.vue'
import SshDialog from './components/session/SshDialog.vue'
import QuickConnectBar from './components/session/QuickConnectBar.vue'
import SessionPanel from './components/session/SessionPanel.vue'
import SftpPanel from './components/sftp/SftpPanel.vue'

const store = useTerminalStore()
const sessionStore = useSessionStore()
const sshDialogVisible = ref(false)
const quickConnectVisible = ref(false)
const sessionPanelVisible = ref(false)
const sftpPanelVisible = ref(false)
const sshDialogPrefill = ref<{ host: string; port: number; username: string }>()

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

const zmodemUnlisten: Array<() => void> = []

onMounted(async () => {
  const un1 = await listen<{ session_id: string }>('zmodem-start', async (event) => {
    const path = await save({ title: 'Save Zmodem file' })
    await invoke('zmodem_respond', {
      sessionId: event.payload.session_id,
      savePath: path,
    })
  })
  zmodemUnlisten.push(un1)

  const un2 = await listen<{ session_id: string; error?: string }>('zmodem-end', (event) => {
    if (event.payload.error) {
      console.error('Zmodem error:', event.payload.error)
    }
  })
  zmodemUnlisten.push(un2)
})

onUnmounted(() => {
  zmodemUnlisten.forEach(fn => fn())
})
</script>

<template>
  <div class="app">
    <TabBar
      @ssh-click="quickConnectVisible = !quickConnectVisible"
      @sessions-click="sessionPanelVisible = !sessionPanelVisible"
      @sftp-click="sftpPanelVisible = !sftpPanelVisible"
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
        <TerminalPane
          v-if="store.activeTab"
          :key="store.activeTab.id"
          :tab="store.activeTab"
          @newSsh="sshDialogVisible = true"
        />
      </div>
      <SftpPanel
        v-if="sftpPanelVisible"
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
