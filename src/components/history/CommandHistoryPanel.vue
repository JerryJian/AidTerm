<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@/api'
import { useI18n } from 'vue-i18n'
import { useTerminalStore } from '../../stores/terminal'
import { useCommandHistoryStore } from '../../stores/commandHistoryStore'

const { t } = useI18n()
const termStore = useTerminalStore()
const historyStore = useCommandHistoryStore()

const searchQuery = ref('')

const activeLeafId = computed(() => termStore.activeLeafId)

const entries = computed(() => {
  const leafId = activeLeafId.value
  if (!leafId) return []
  const list = historyStore.historyFor(leafId)
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return list
  return list.filter(e => e.command.toLowerCase().includes(q))
})

function sessionIdOf(): string | null {
  const leafId = activeLeafId.value
  const leaf = leafId ? termStore.findTab(leafId) : null
  return leaf?.session?.id ?? null
}

async function execute(cmd: string) {
  const sessionId = sessionIdOf()
  if (!sessionId) return
  try {
    await invoke('connection_write', { sessionId, data: cmd + '\r' })
  } catch (e) {
    console.error('Failed to execute history command:', e)
  }
}

function remove(entryId: string) {
  const leafId = activeLeafId.value
  if (leafId) historyStore.removeEntry(leafId, entryId)
}

function clearAll() {
  const leafId = activeLeafId.value
  if (leafId) historyStore.clear(leafId)
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
</script>

<template>
  <div class="panel">
    <div class="toolbar">
      <span class="toolbar-title">{{ t('history.title') }}</span>
      <button class="tb-btn" :disabled="entries.length === 0" @click="clearAll">{{ t('history.clear') }}</button>
    </div>

    <div class="search-row">
      <input
        v-model="searchQuery"
        type="text"
        class="search-input"
        :placeholder="t('history.search')"
      />
    </div>

    <div class="panel-body">
      <div v-for="e in entries" :key="e.id" class="history-item">
        <div class="history-info" @click="execute(e.command)" :title="e.command">
          <span class="history-cmd">{{ e.command }}</span>
          <span class="history-time">{{ formatTime(e.timestamp) }}</span>
        </div>
        <div class="history-actions">
          <button class="btn-sm" @click="execute(e.command)" :title="t('history.execute')">▶</button>
          <button class="btn-sm btn-danger" @click="remove(e.id)" :title="t('common.delete')">✕</button>
        </div>
      </div>
      <div v-if="entries.length === 0" class="empty">
        {{ searchQuery ? t('history.no_match') : t('history.empty') }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  flex: 1;
  min-height: 0;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
  background: var(--bg-mantle);
  flex-shrink: 0;
}
.toolbar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.tb-btn {
  padding: 3px 10px;
  border: 1px solid var(--border, var(--bg-surface0));
  border-radius: 4px;
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  font-size: 11px;
}
.tb-btn:hover:not(:disabled) { background: var(--bg-surface1); }
.tb-btn:disabled { opacity: 0.4; cursor: default; }

.search-row {
  padding: 8px;
  border-bottom: 1px solid var(--bg-surface0);
  flex-shrink: 0;
}
.search-input {
  width: 100%;
  padding: 5px 8px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  font-size: 12px;
  outline: none;
}
.search-input:focus { border-color: var(--accent); }

.panel-body {
  flex: 1;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
}

.history-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px;
  background: var(--bg-mantle);
  border-radius: 4px;
  font-size: 12px;
}

.history-info {
  flex: 1;
  min-width: 0;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.history-info:hover .history-cmd { color: var(--accent); }

.history-cmd {
  font-family: Consolas, "Courier New", monospace;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-time {
  color: var(--text-overlay0);
  font-size: 11px;
}

.history-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
  margin-left: 8px;
}

.btn-sm {
  padding: 4px 8px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.btn-sm:hover { background: var(--bg-surface1); }
.btn-danger:hover {
  border-color: var(--danger);
  color: var(--danger);
}

.empty {
  color: var(--text-overlay0);
  font-size: 12px;
  text-align: center;
  padding: 20px;
}
</style>
