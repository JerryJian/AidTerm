<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTerminalStore } from '../../stores/terminal'
import { useSessionStore } from '../../stores/sessionStore'
import { useUiStore } from '../../stores/uiStore'
import type { SavedSession, ToolTab } from '../../types'
import QuickConnectBar from '../session/QuickConnectBar.vue'

const store = useTerminalStore()
const sessionStore = useSessionStore()
const ui = useUiStore()

const emit = defineEmits<{
  quickSsh: [host: string, port: number, username: string]
  quickTelnet: [host: string, port: number]
  quickSerial: []
  connectSession: [session: SavedSession]
}>()

const quickConnectVisible = ref(false)
const newTabMenuOpen = ref(false)
const batchInput = ref('')
const batchFocused = ref(false)

const availableShells = ref<string[]>([])
const toolsMenuOpen = ref(false)

const activeTabSessionType = computed(() => store.activeTab?.session?.type)

const toolTabs = computed<{ id: ToolTab; icon: string }[]>(() => {
  const all: { id: ToolTab; icon: string }[] = [
    { id: 'sftp', icon: '\uD83D\uDCC2' },
    { id: 'tunnel', icon: '\uD83D\uDD0C' },
  ]
  if (activeTabSessionType.value === 'ssh') {
    return all
  }
  return []
})

function toggleToolsMenu() {
  toolsMenuOpen.value = !toolsMenuOpen.value
  newTabMenuOpen.value = false
}

function openToolTab(tab: ToolTab) {
  if (store.activeTabId) {
    store.addToolTab(store.activeTabId, tab)
  }
  toolsMenuOpen.value = false
}

function isToolOpen(tab: ToolTab): boolean {
  if (!store.activeTabId) return false
  return store.isToolOpen(store.activeTabId, tab)
}

const savedSessions = computed(() => sessionStore.sessions)

function onSavedSessionClick(session: SavedSession) {
  newTabMenuOpen.value = false
  emit('connectSession', session)
}

onMounted(async () => {
  if (!sessionStore.loaded) await sessionStore.load()
  try {
    const shells = await invoke<string[]>('detect_shells')
    availableShells.value = shells
  } catch { /* ignore */ }
})

const shellKeyMap: Record<string, string> = {
  'cmd.exe': 'cmd',
  'powershell.exe': 'powershell',
  'pwsh.exe': 'pwsh',
  'wsl.exe': 'wsl',
  'bash.exe': 'bash',
  'bash': 'bash',
  'zsh': 'zsh',
  'sh': 'sh',
  'fish': 'fish',
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

function openSerial() {
  newTabMenuOpen.value = false
  emit('quickSerial')
}

function onNewTabClick() {
  newTabMenuOpen.value = !newTabMenuOpen.value
  toolsMenuOpen.value = false
}

function onDocClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (target?.closest('.tools-wrapper') || target?.closest('.new-tab-wrapper')) return
  newTabMenuOpen.value = false
  toolsMenuOpen.value = false
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
      <button class="sidebar-toggle" :class="{ active: ui.leftSidebar }" @click="ui.leftSidebar = !ui.leftSidebar" title="Toggle Sidebar">{{ '\u2630' }}</button>
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
          <div class="new-tab-section-title">{{ $t('menu.local_shell') }}</div>
          <button v-for="s in availableShells" :key="s" class="menu-item" @click="openLocalShell(s)">{{ $t('shell.' + (shellKeyMap[s] || s)) || s }}</button>
          <div class="menu-divider" />
          <div class="new-tab-section-title">{{ $t('menu.remote_connection') }}</div>
          <button class="menu-item" @click="openSsh()"><span class="mi-icon">{{ '\uD83D\uDD12' }}</span><span>{{ $t('menu.ssh') }}</span></button>
          <button class="menu-item" @click="openTelnet()"><span class="mi-icon">{{ '\uD83D\uDD0C' }}</span><span>{{ $t('menu.telnet') }}</span></button>
          <button class="menu-item" @click="openSerial()"><span class="mi-icon">{{ '\uD83D\uDD04' }}</span><span>{{ $t('menu.serial') }}</span></button>
          <div v-if="savedSessions.length > 0" class="menu-divider" />
          <div v-if="savedSessions.length > 0" class="saved-sessions-list">
            <template v-for="group in sessionStore.groups" :key="group.id">
              <div class="saved-group-label">{{ group.name }}</div>
              <button v-for="s in sessionStore.getSessionsByGroup(group.id)" :key="s.id" class="menu-item" @click="onSavedSessionClick(s)">
                <span class="mi-icon">{{ s.session_type === 'ssh' ? '\uD83D\uDD12' : s.session_type === 'serial' ? '\uD83D\uDD04' : '\uD83D\uDD0C' }}</span>
                <span class="saved-session-name">{{ s.name }}</span>
                <span class="saved-session-meta">{{ s.session_type === 'serial' ? s.host : (s.username ? s.username + '@' : '') + (s.host ?? '') }}</span>
              </button>
            </template>
            <template v-if="sessionStore.getUngroupedSessions().length > 0">
              <div class="saved-group-label">{{ $t('session_panel.ungrouped') }}</div>
              <button v-for="s in sessionStore.getUngroupedSessions()" :key="s.id" class="menu-item" @click="onSavedSessionClick(s)">
                <span class="mi-icon">{{ s.session_type === 'ssh' ? '\uD83D\uDD12' : s.session_type === 'serial' ? '\uD83D\uDD04' : '\uD83D\uDD0C' }}</span>
                <span class="saved-session-name">{{ s.name }}</span>
                <span class="saved-session-meta">{{ s.session_type === 'serial' ? s.host : (s.username ? s.username + '@' : '') + (s.host ?? '') }}</span>
              </button>
            </template>
          </div>
        </div>
      </div>
    </div>
    <div class="tab-bar-right">
      <div class="tools-wrapper">
        <button class="tools-btn" :class="{ active: toolsMenuOpen }" @click="toggleToolsMenu" title="Tools">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
            <path d="M4 21C3.44772 21 3 20.5523 3 20V4C3 3.44772 3.44772 3 4 3H20C20.5523 3 21 3.44772 21 4V20C21 20.5523 20.5523 21 20 21H4ZM8 10H5V19H8V10ZM19 10H10V19H19V10ZM19 5H5V8H19V5Z"/>
          </svg>
        </button>
        <div v-if="toolsMenuOpen" class="tools-backdrop" @click="toolsMenuOpen = false" />
        <div v-if="toolsMenuOpen" class="tools-dropdown">
          <button v-for="t in toolTabs" :key="t.id" class="tools-item" :class="{ active: isToolOpen(t.id) }" @click="openToolTab(t.id)">
            <span class="ti-icon">{{ t.icon }}</span>
            <span>{{ $t('tool_panel.' + t.id) }}</span>
          </button>
        </div>
      </div>
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
      :placeholder="$t('batch.placeholder')"
      @keydown="onBatchInput"
      @focus="batchFocused = true"
      @blur="batchFocused = false"
    />
    <button class="batch-select-btn" @click="selectAllBatch">{{ $t('batch.select_all') }}</button>
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

.sidebar-toggle {
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
  flex-shrink: 0;
}
.sidebar-toggle:hover {
  background: var(--bg-surface0);
  color: var(--text);
}
.sidebar-toggle.active {
  background: var(--accent-glass);
  color: var(--accent);
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
.tab-bar-right {
  display: flex;
  align-items: center;
  padding-right: 6px;
  flex-shrink: 0;
}

.tools-wrapper {
  position: relative;
}

.tools-btn {
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
}
.tools-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}
.tools-btn.active {
  background: var(--accent-glass);
  color: var(--accent);
}

.tools-backdrop {
  position: fixed;
  inset: 0;
  z-index: 999;
  background: transparent;
}

.tools-dropdown {
  position: absolute;
  top: 100%;
  right: 0;
  z-index: 1000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 150px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.tools-item {
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
.tools-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.tools-item.active {
  color: var(--success);
}

.ti-icon {
  width: 18px;
  text-align: center;
  flex-shrink: 0;
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

.menu-item {
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
.menu-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.menu-item.active {
  color: var(--success);
}

.mi-icon {
  width: 18px;
  text-align: center;
  flex-shrink: 0;
}

.menu-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
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

.saved-sessions-list {
  max-height: 200px;
  overflow-y: auto;
}

.saved-session-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.saved-session-meta {
  font-size: 10px;
  color: var(--text-overlay0);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 120px;
  flex-shrink: 0;
}

.saved-group-label {
  padding: 6px 10px 3px;
  font-size: 10px;
  text-transform: uppercase;
  color: var(--text-overlay0);
  font-weight: 600;
  letter-spacing: 0.5px;
}
</style>
