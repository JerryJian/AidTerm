<script setup lang="ts">
import { useTerminalStore } from '../../stores/terminal'

const store = useTerminalStore()

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
      <span class="tab-status" :class="tab.session?.status" />
      <span class="tab-title">{{ tab.title }}</span>
      <button class="tab-close" @click.stop="store.closeTab(tab.id)">✕</button>
    </div>
    <button class="tab-add" @click="store.addTab()" title="New Tab (Ctrl+T)">+</button>
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

.tab-add {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  font-size: 16px;
  margin-left: 4px;
  border-radius: 4px;
}

.tab-add:hover {
  background: #313244;
  color: #cdd6f4;
}
</style>
