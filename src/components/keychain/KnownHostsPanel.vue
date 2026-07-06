<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface KnownHostEntry {
  host: string
  key_type: string
  fingerprint: string
  line: string
}

const emit = defineEmits<{
  close: []
}>()

const entries = ref<KnownHostEntry[]>([])
const loading = ref(true)
const error = ref('')
const notification = ref('')

onMounted(async () => {
  await loadEntries()
})

async function loadEntries() {
  loading.value = true
  error.value = ''
  try {
    entries.value = await invoke<KnownHostEntry[]>('known_hosts_list')
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function doRemove(host: string, keyType: string) {
  error.value = ''
  try {
    await invoke('known_hosts_remove', { host, keyType })
    entries.value = entries.value.filter(e => !(e.host === host && e.key_type === keyType))
    notification.value = `已移除: ${host}`
  } catch (e: any) {
    error.value = String(e)
  }
}
</script>

<template>
  <div class="kh-panel">
    <div class="panel-header">
      <span class="panel-title">🖂 known_hosts</span>
      <div class="panel-actions">
        <button class="panel-btn" @click="loadEntries">刷新</button>
        <button class="panel-btn" @click="emit('close')">✕</button>
      </div>
    </div>

    <div v-if="notification" class="notification">{{ notification }}</div>
    <div v-if="error" class="error">{{ error }}</div>

    <div class="kh-list">
      <div v-if="loading" class="loading">加载中...</div>
      <div v-else-if="entries.length === 0" class="empty">~/.ssh/known_hosts 为空</div>
      <div v-for="entry in entries" :key="entry.host + entry.key_type" class="kh-item">
        <div class="kh-host">{{ entry.host }}</div>
        <div class="kh-fingerprint">{{ entry.fingerprint }}</div>
        <div class="kh-actions">
          <button class="action-btn danger" title="删除" @click="doRemove(entry.host, entry.key_type)">🗑</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.kh-panel {
  width: 320px;
  min-width: 320px;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  height: 100%;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.panel-actions {
  display: flex;
  gap: 4px;
}

.panel-btn {
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  padding: 4px 10px;
  border-radius: 4px;
  font-size: 11px;
}
.panel-btn:hover {
  background: var(--bg-surface1);
}

.notification {
  padding: 8px 12px;
  background: #1e3a2e;
  color: var(--success);
  font-size: 12px;
}

.error {
  padding: 8px 12px;
  background: #3a1e1e;
  color: var(--danger);
  font-size: 12px;
}

.kh-list {
  flex: 1;
  overflow-y: auto;
}

.loading, .empty {
  padding: 24px 12px;
  text-align: center;
  color: var(--text-overlay0);
  font-size: 12px;
}

.kh-item {
  display: flex;
  align-items: center;
  padding: 8px 12px;
  border-bottom: 1px solid var(--bg-surface0);
  gap: 8px;
}

.kh-host {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  min-width: 100px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.kh-fingerprint {
  flex: 1;
  font-size: 11px;
  color: var(--text-overlay0);
  font-family: monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.kh-actions {
  flex-shrink: 0;
}

.action-btn {
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
}
.action-btn:hover {
  background: var(--bg-surface1);
}
.action-btn.danger:hover {
  background: var(--danger);
  color: var(--bg-base);
}
</style>
