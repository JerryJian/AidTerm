<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useSessionStore } from '../../stores/sessionStore'
import type { SavedSession } from '../../types'

const store = useSessionStore()

const emit = defineEmits<{
  connectSession: [session: SavedSession]
  newSession: []
  editSession: [session: SavedSession]
  close: []
}>()

onMounted(() => {
  if (!store.loaded) store.load()
})

const sessionTypeIcon = computed(() => (type: string) => {
  switch (type) {
    case 'ssh': return '🔒'
    case 'telnet': return '🔗'
    case 'serial': return '🔌'
    default: return '💻'
  }
})

function onSessionClick(session: SavedSession) {
  emit('connectSession', session)
}

function onSessionEdit(e: MouseEvent, session: SavedSession) {
  e.stopPropagation()
  emit('editSession', session)
}
</script>

<template>
  <div class="session-panel">
    <div class="panel-header">
      <span class="panel-title">Sessions</span>
      <div class="panel-actions">
        <button class="panel-btn" title="New Session" @click="emit('newSession')">+</button>
        <button class="panel-btn" title="Close Panel" @click="emit('close')">✕</button>
      </div>
    </div>

    <div class="session-list">
      <div v-for="group in store.groups" :key="group.id" class="group-section">
        <div class="group-header" @click="group.expanded = !group.expanded">
          <span class="group-arrow">{{ group.expanded ? '▼' : '▶' }}</span>
          <span class="group-name">{{ group.name }}</span>
          <button class="group-del" @click.stop="store.removeGroup(group.id)">✕</button>
        </div>
        <div v-if="group.expanded" class="group-sessions">
          <div
            v-for="s in store.getSessionsByGroup(group.id)"
            :key="s.id"
            class="session-item"
            @click="onSessionClick(s)"
          >
            <span class="sess-icon">{{ sessionTypeIcon(s.session_type) }}</span>
            <span class="sess-name">{{ s.name }}</span>
            <span class="sess-host">{{ s.host }}</span>
            <button class="sess-edit" @click="(e) => onSessionEdit(e, s)" title="Edit">✎</button>
          </div>
          <div v-if="store.getSessionsByGroup(group.id).length === 0" class="empty-hint">(empty)</div>
        </div>
      </div>

      <div class="group-section" v-if="store.getUngroupedSessions().length > 0">
        <div class="group-header">
          <span class="group-arrow">▼</span>
          <span class="group-name">Ungrouped</span>
        </div>
        <div class="group-sessions">
          <div
            v-for="s in store.getUngroupedSessions()"
            :key="s.id"
            class="session-item"
            @click="onSessionClick(s)"
          >
            <span class="sess-icon">{{ sessionTypeIcon(s.session_type) }}</span>
            <span class="sess-name">{{ s.name }}</span>
            <span class="sess-host">{{ s.host }}</span>
            <button class="sess-edit" @click="(e) => onSessionEdit(e, s)" title="Edit">✎</button>
          </div>
        </div>
      </div>

      <div v-if="store.groups.length === 0 && store.getUngroupedSessions().length === 0" class="empty-state">
        No saved sessions yet.
      </div>
    </div>
  </div>
</template>

<style scoped>
.session-panel {
  min-width: 180px;
  height: 100%;
  background: #181825;
  border-right: 1px solid #313244;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid #313244;
}

.panel-title {
  font-weight: 600;
  font-size: 13px;
  color: #cdd6f4;
}

.panel-actions {
  display: flex;
  gap: 4px;
}

.panel-btn {
  background: none;
  border: 1px solid transparent;
  color: #a6adc8;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
}
.panel-btn:hover {
  background: #313244;
  color: #cdd6f4;
}

.session-list {
  flex: 1;
  overflow-y: auto;
}

.group-section {
  border-bottom: 1px solid #313244;
}

.group-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  cursor: pointer;
  user-select: none;
  font-size: 12px;
}
.group-header:hover {
  background: #1e1e2e;
}

.group-arrow {
  color: #585b70;
  font-size: 10px;
  width: 12px;
}

.group-name {
  flex: 1;
  color: #a6adc8;
  font-weight: 600;
}

.group-del {
  background: none;
  border: none;
  color: #585b70;
  cursor: pointer;
  font-size: 10px;
  padding: 2px;
  visibility: hidden;
}
.group-header:hover .group-del {
  visibility: visible;
}
.group-del:hover {
  color: #f38ba8;
}

.group-sessions {
  padding-bottom: 4px;
}

.session-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px 5px 30px;
  cursor: pointer;
  font-size: 12px;
}
.session-item:hover {
  background: #1e1e2e;
}

.sess-icon {
  font-size: 10px;
  width: 16px;
  text-align: center;
}

.sess-name {
  flex: 1;
  color: #cdd6f4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sess-host {
  color: #585b70;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 80px;
}

.sess-edit {
  background: none;
  border: none;
  color: #585b70;
  cursor: pointer;
  font-size: 12px;
  padding: 2px 4px;
  border-radius: 3px;
  visibility: hidden;
}
.session-item:hover .sess-edit {
  visibility: visible;
}
.sess-edit:hover {
  background: #313244;
  color: #89b4fa;
}

.empty-hint {
  padding: 2px 12px 2px 30px;
  color: #585b70;
  font-size: 11px;
  font-style: italic;
}

.empty-state {
  padding: 24px 12px;
  color: #585b70;
  font-size: 12px;
  text-align: center;
}
</style>
