<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTerminalStore } from '../../stores/terminal'
import { useUiStore } from '../../stores/uiStore'
import QuickConnectBar from '../session/QuickConnectBar.vue'

const store = useTerminalStore()
const ui = useUiStore()

const emit = defineEmits<{
  lockClick: []
  quickSsh: [host: string, port: number, username: string]
  quickTelnet: [host: string, port: number]
}>()

const menuOpen = ref(false)
const quickConnectVisible = ref(false)
const newTabMenuOpen = ref(false)
const batchInput = ref('')
const batchFocused = ref(false)

const availableShells = ref<string[]>([])
onMounted(async () => {
  try {
    const shells = await invoke<string[]>('detect_shells')
    availableShells.value = shells
  } catch { /* ignore */ }
})

const shellDisplayNames: Record<string, string> = {
  'cmd.exe': 'cmd',
  'powershell.exe': 'PowerShell 5.1',
  'pwsh.exe': 'PowerShell 7',
  'wsl.exe': 'WSL',
  'bash.exe': 'Bash',
  'bash': 'Bash',
  'zsh': 'Zsh',
  'sh': 'Sh',
  'fish': 'Fish',
}

function openLocalShell(shell: string) {
  newTabMenuOpen.value = false
  store.addTab('local', undefined, undefined, shell)
}

function openSsh() {
  newTabMenuOpen.value = false
  ui.sshDialog = true
}

function openTelnet() {
  newTabMenuOpen.value = false
  quickConnectVisible.value = true
}

function onNewTabClick() {
  newTabMenuOpen.value = !newTabMenuOpen.value
}

function onDocClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (!target.closest('.new-tab-wrapper') && !target.closest('.menu-wrapper')) {
    newTabMenuOpen.value = false
  }
}

onMounted(() => document.addEventListener('click', onDocClick, true))
onUnmounted(() => document.removeEventListener('click', onDocClick, true))

function onKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.key === 't') {
    e.preventDefault()
    store.addTab()
  }
  if (e.ctrlKey && e.key === 'w') {
    e.preventDefault()
    if (store.activeTabId) {
      store.closeTab(store.activeTabId)
    }
  }
  if (e.ctrlKey && e.key === 'Tab') {
    e.preventDefault()
    const currentIdx = store.tabs.findIndex(t => t.id === store.activeTabId)
    const nextIdx = (currentIdx + 1) % store.tabs.length
    store.setActiveTab(store.tabs[nextIdx].id)
  }
  if (e.ctrlKey && e.key === 'Tab' && e.shiftKey) {
    e.preventDefault()
    const currentIdx = store.tabs.findIndex(t => t.id === store.activeTabId)
    const prevIdx = (currentIdx - 1 + store.tabs.length) % store.tabs.length
    store.setActiveTab(store.tabs[prevIdx].id)
  }
}

function onBatchInput(e: KeyboardEvent) {
  if (e.key !== 'Enter' || !batchInput.value.trim()) return
  const ids = store.getBatchSessionIds()
  const data = batchInput.value
  batchInput.value = ''
  for (const sid of ids) {
    invoke('write_terminal', { sessionId: sid, data: data + '\n' })
  }
}

function selectAllBatch() {
  for (const tab of store.tabs) {
    store.setBatchTabId(tab.id, true)
  }
}

function onSshConnect(host: string, port: number, username: string) {
  quickConnectVisible.value = false
  emit('quickSsh', host, port, username)
}

function onTelnetConnect(host: string, port: number) {
  quickConnectVisible.value = false
  emit('quickTelnet', host, port)
}

defineExpose({ onKeydown })
</script>

<template>
  <div class="tab-bar" @keydown="onKeydown">
    <div class="tab-bar-left">
      <div class="menu-wrapper">
        <button class="menu-btn" @click="menuOpen = !menuOpen" title="Menu" @blur="menuOpen = false">{{ '\u2630' }}</button>
        <div v-if="menuOpen" class="menu-dropdown" @mousedown.prevent>
          <button @click="ui.settingsDialog = true; menuOpen = false" class="menu-item">{{ '\u2699' }} Settings</button>
          <button @click="emit('lockClick'); menuOpen = false" class="menu-item">{{ '\uD83D\uDD12' }} Lock</button>
          <button @click="store.toggleBatch(); menuOpen = false" class="menu-item" :class="{ active: store.batchMode }">{{ '\uD83D\uDCE1' }} Batch Mode</button>
          <div class="menu-divider" />
          <button @click="quickConnectVisible = !quickConnectVisible; menuOpen = false" class="menu-item">{{ '\uD83D\uDD0C' }} Quick Connect</button>
          <button @click="ui.sshDialog = true; menuOpen = false" class="menu-item">{{ '\uD83D\uDD12' }} New SSH...</button>
        </div>
      </div>
      <div
        v-for="tab in store.tabs"
        :key="tab.id"
        class="tab"
        :class="{ active: tab.id === store.activeTabId }"
        @click="store.setActiveTab(tab.id)"
        @mouseup.middle="store.closeTab(tab.id)"
      >
        <input
          v-if="store.batchMode"
          type="checkbox"
          class="tab-checkbox"
          :checked="store.batchTabIds.has(tab.id)"
          @click.stop
          @change="(e: Event) => store.setBatchTabId(tab.id, (e.target as HTMLInputElement).checked)"
        />
        <span class="tab-status" :class="tab.session?.status" />
        <span class="tab-title">{{ tab.title }}</span>
        <button class="tab-close" @click.stop="store.closeTab(tab.id)">{{ '\u2715' }}</button>
      </div>
      <div class="new-tab-wrapper">
        <button class="tab-add" @click="onNewTabClick">+</button>
        <div v-if="newTabMenuOpen" class="new-tab-menu" @mousedown.prevent>
          <div class="new-tab-section-title">Local Shell</div>
          <button v-for="s in availableShells" :key="s" class="menu-item" @click="openLocalShell(s)">{{ shellDisplayNames[s] || s }}</button>
          <div class="menu-divider" />
          <div class="new-tab-section-title">Remote Connection</div>
          <button class="menu-item" @click="openSsh()">{{ '\uD83D\uDD12' }} SSH...</button>
          <button class="menu-item" @click="openTelnet()">{{ '\uD83D\uDD0C' }} Telnet...</button>
        </div>
      </div>
    </div>
    <div class="tab-bar-right">
      <button
        class="tb-btn"
        :class="{ active: ui.leftSidebar }"
        @click="ui.leftSidebar = !ui.leftSidebar"
        title="Sessions"
      >{{ '\uD83D\uDCCB' }}</button>
      <button
        class="tb-btn"
        :class="{ active: ui.rightSidebar }"
        @click="ui.rightSidebar = !ui.rightSidebar"
        title="Tools"
      >{{ '\uD83D\uDD27' }}</button>
    </div>
  </div>
  <!-- Quick connect / batch bars -->
  <QuickConnectBar
    :visible="quickConnectVisible"
    @ssh-connect="onSshConnect"
    @telnet-connect="onTelnetConnect"
    @close="quickConnectVisible = false"
  />
  <div v-if="store.batchMode" class="batch-bar">
    <input
      v-model="batchInput"
      class="batch-input"
      placeholder="批量输入命令到选中终端..."
      @keydown="onBatchInput"
      @focus="batchFocused = true"
      @blur="batchFocused = false"
    />
    <button class="batch-select-btn" @click="selectAllBatch">全选</button>
    <span class="batch-count">{{ store.batchTabIds.size }} / {{ store.tabs.length }}</span>
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  user-select: none;
  min-height: 32px;
}

.tab-bar-left {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
}

.tab-bar-right {
  display: flex;
  align-items: center;
  gap: 2px;
  padding-right: 8px;
  flex-shrink: 0;
}

.menu-wrapper {
  position: relative;
}

.menu-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  border-radius: 4px;
  font-size: 14px;
  margin: 0 2px;
}
.menu-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.menu-dropdown {
  position: absolute;
  top: 100%;
  left: 2px;
  z-index: 1000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 180px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.menu-item {
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
  white-space: nowrap;
}
.menu-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.menu-item.active {
  color: var(--success);
}

.menu-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  cursor: pointer;
  color: var(--text-sub0);
  font-size: 13px;
  border-right: 1px solid var(--bg-surface0);
  min-width: 0;
  position: relative;
}

.tab:hover {
  background: var(--bg-base);
  color: var(--text);
}

.tab.active {
  background: var(--bg-base);
  color: var(--text);
  border-bottom: 2px solid var(--accent);
}

.tab-checkbox {
  margin: 0;
  accent-color: var(--accent);
  width: 14px;
  height: 14px;
}

.tab-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.tab-status.connected {
  background: var(--success);
}

.tab-status.connecting {
  background: var(--warning);
}

.tab-status.disconnected {
  background: var(--bg-surface1);
}

.tab-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 150px;
}

.tab-close {
  visibility: hidden;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 4px;
  font-size: 12px;
  border-radius: 4px;
  width: 22px;
  text-align: center;
  flex-shrink: 0;
}

.tab:hover .tab-close {
  visibility: visible;
}

.tab-close:hover {
  background: var(--bg-surface1);
  color: var(--danger);
}

.tab-add,
.tb-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  border-radius: 4px;
}

.tab-add {
  width: 28px;
  height: 28px;
  font-size: 16px;
  margin-left: 4px;
  flex-shrink: 0;
}

.tab-add:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.tb-btn {
  width: 30px;
  height: 26px;
  font-size: 14px;
}
.tb-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}
.tb-btn.active {
  background: var(--bg-surface0);
  color: var(--accent);
}

.batch-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  background: var(--bg-base);
  border-bottom: 1px solid var(--bg-surface0);
}

.batch-input {
  flex: 1;
  max-width: 400px;
  padding: 6px 10px;
  background: var(--bg-mantle);
  border: 1px solid var(--success);
  border-radius: 4px;
  color: var(--text);
  font-size: 13px;
  outline: none;
}
.batch-input:focus {
  border-color: var(--accent);
}

.batch-select-btn {
  padding: 4px 10px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.batch-select-btn:hover {
  background: var(--bg-surface1);
}

.batch-count {
  font-size: 11px;
  color: var(--text-sub0);
}

.new-tab-wrapper {
  position: relative;
}

.new-tab-menu {
  position: absolute;
  top: 100%;
  left: 2px;
  z-index: 1000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 160px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.new-tab-section-title {
  padding: 6px 10px 3px;
  font-size: 10px;
  text-transform: uppercase;
  color: var(--text-overlay0);
  font-weight: 600;
  letter-spacing: 0.5px;
}
</style>
