<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTerminalStore } from '../../stores/terminal'

const store = useTerminalStore()

const emit = defineEmits<{
  sshClick: []
  sessionsClick: []
  sftpClick: []
  tunnelClick: []
  proxyClick: []
  snippetClick: []
  triggerClick: []
  settingsClick: []
  lockClick: []
}>()

const batchInput = ref('')
const batchFocused = ref(false)

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

defineExpose({ onKeydown })
</script>

<template>
  <div class="tab-bar" @keydown="onKeydown">
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
      <button class="tab-close" @click.stop="store.closeTab(tab.id)">✕</button>
    </div>
    <button class="tab-add" @click="store.addTab()" title="New Tab (Ctrl+T)">+</button>
    <button class="tab-btn" :class="{ active: store.batchMode }" @click="store.toggleBatch()" title="Batch Input">📡</button>
    <button class="tab-btn" @click="emit('tunnelClick')" title="Port Forwarding">🔌</button>
    <button class="tab-btn" @click="emit('proxyClick')" title="Proxy Settings">🌐</button>
    <button class="tab-btn" @click="emit('snippetClick')" title="Quick Commands">⚡</button>
    <button class="tab-btn" @click="emit('triggerClick')" title="Triggers">🔫</button>
    <button class="tab-btn" @click="emit('sftpClick')" title="SFTP">📂</button>
    <button class="tab-btn" @click="emit('settingsClick')" title="Settings">⚙</button>
    <button class="tab-btn" @click="emit('lockClick')" title="Lock">🔒</button>
    <button class="tab-btn tab-ssh" @click="emit('sshClick')" title="SSH Connection">SSH</button>
  </div>
  <!-- Batch input bar -->
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
  background: #181825;
  border-bottom: 1px solid #313244;
  user-select: none;
  min-height: 32px;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  cursor: pointer;
  color: #a6adc8;
  font-size: 13px;
  border-right: 1px solid #313244;
  min-width: 0;
  position: relative;
}

.tab:hover {
  background: #1e1e2e;
  color: #cdd6f4;
}

.tab.active {
  background: #1e1e2e;
  color: #cdd6f4;
  border-bottom: 2px solid #89b4fa;
}

.tab-checkbox {
  margin: 0;
  accent-color: #89b4fa;
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
  background: #a6e3a1;
}

.tab-status.connecting {
  background: #f9e2af;
}

.tab-status.disconnected {
  background: #45475a;
}

.tab-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 150px;
}

.tab-close {
  display: none;
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  padding: 2px 4px;
  font-size: 12px;
  border-radius: 4px;
}

.tab:hover .tab-close {
  display: block;
}

.tab-close:hover {
  background: #45475a;
  color: #f38ba8;
}

.tab-add,
.tab-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  border-radius: 4px;
}

.tab-add {
  width: 28px;
  height: 28px;
  font-size: 16px;
  margin-left: 4px;
}

.tab-add:hover {
  background: #313244;
  color: #cdd6f4;
}

.tab-btn {
  padding: 4px 8px;
  font-size: 13px;
  margin-left: 2px;
}

.tab-btn:hover {
  background: #313244;
  color: #cdd6f4;
}

.tab-btn.active {
  background: #313244;
  color: #a6e3a1;
  border: 1px solid #a6e3a1;
}

.tab-ssh {
  font-size: 11px;
  font-weight: 600;
  border: 1px solid #45475a;
  background: #1e1e2e;
  color: #89b4fa;
}

.tab-ssh:hover {
  background: #313244;
  color: #74c7ec;
}

.batch-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  background: #1e1e2e;
  border-bottom: 1px solid #313244;
}

.batch-input {
  flex: 1;
  max-width: 400px;
  padding: 6px 10px;
  background: #181825;
  border: 1px solid #a6e3a1;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 13px;
  outline: none;
}
.batch-input:focus {
  border-color: #89b4fa;
}

.batch-select-btn {
  padding: 4px 10px;
  border: 1px solid #45475a;
  background: #313244;
  color: #cdd6f4;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.batch-select-btn:hover {
  background: #45475a;
}

.batch-count {
  font-size: 11px;
  color: #a6adc8;
}
</style>
