<script setup lang="ts">
import { ref } from 'vue'
import { useTerminalStore } from './stores/terminal'
import type { SshConnectionInfo } from './types'
import TabBar from './components/terminal/TabBar.vue'
import TerminalPane from './components/terminal/TerminalPane.vue'
import SshDialog from './components/session/SshDialog.vue'

const store = useTerminalStore()
const sshDialogVisible = ref(false)

if (store.tabs.length === 0) {
  store.addTab('local')
}

function onSshConnect(info: SshConnectionInfo) {
  sshDialogVisible.value = false
  store.addTab('ssh', info)
}
</script>

<template>
  <div class="app">
    <TabBar @ssh-click="sshDialogVisible = true" />
    <div class="terminal-area">
      <TerminalPane
        v-if="store.activeTab"
        :key="store.activeTab.id"
        :tab="store.activeTab"
      />
    </div>
  </div>
  <SshDialog
    v-if="sshDialogVisible"
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

.terminal-area {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
</style>
