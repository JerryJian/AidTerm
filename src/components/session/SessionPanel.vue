<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useSessionStore } from '../../stores/sessionStore'
import type { SavedSession } from '../../types'

const store = useSessionStore()

const emit = defineEmits<{
  connectSession: [session: SavedSession]
  close: []
}>()

const newGroupName = ref('')
const newSessionName = ref('')
const newSessionType = ref<'ssh' | 'telnet'>('ssh')
const newSessionHost = ref('')
const newSessionPort = ref(22)
const newSessionUsername = ref('')
const showAddForm = ref(false)
const showAddGroupInput = ref(false)

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

function addGroup() {
  const name = newGroupName.value.trim()
  if (!name) return
  store.addGroup(name)
  newGroupName.value = ''
  showAddGroupInput.value = false
}

function addSession() {
  const name = newSessionName.value.trim()
  if (!name) return
  store.addSession(
    name,
    newSessionType.value,
    {
      host: newSessionHost.value.trim() || undefined,
      port: newSessionPort.value,
      username: newSessionUsername.value.trim() || undefined,
    },
  )
  newSessionName.value = ''
  newSessionHost.value = ''
  newSessionPort.value = 22
  newSessionUsername.value = ''
  showAddForm.value = false
}

function onSessionClick(session: SavedSession) {
  emit('connectSession', session)
}
</script>

<template>
  <div class="session-panel">
    <div class="panel-header">
      <span class="panel-title">Sessions</span>
      <div class="panel-actions">
        <button class="panel-btn" title="New Session" @click="showAddForm = !showAddForm">+</button>
        <button class="panel-btn" title="New Group" @click="showAddGroupInput = !showAddGroupInput">📁</button>
        <button class="panel-btn" title="Close Panel" @click="emit('close')">✕</button>
      </div>
    </div>

    <div v-if="showAddGroupInput" class="inline-form">
      <input v-model="newGroupName" placeholder="Group name" @keydown.enter="addGroup" @keydown.escape="showAddGroupInput = false" />
      <button @click="addGroup">OK</button>
    </div>

    <div v-if="showAddForm" class="add-form">
      <input v-model="newSessionName" placeholder="Session name" />
      <select v-model="newSessionType">
        <option value="ssh">SSH</option>
        <option value="telnet">Telnet</option>
      </select>
      <input v-model="newSessionHost" placeholder="Host" />
      <input v-model="newSessionPort" type="number" placeholder="Port" />
      <input v-model="newSessionUsername" placeholder="Username (SSH)" />
      <button @click="addSession">Save</button>
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
  width: 240px;
  min-width: 240px;
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

.inline-form {
  display: flex;
  gap: 4px;
  padding: 6px 12px;
  border-bottom: 1px solid #313244;
}

.inline-form input {
  flex: 1;
  background: #1e1e2e;
  border: 1px solid #45475a;
  color: #cdd6f4;
  padding: 4px 8px;
  font-size: 12px;
  outline: none;
}

.inline-form button {
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  cursor: pointer;
  padding: 4px 8px;
  font-size: 12px;
}

.add-form {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 12px;
  border-bottom: 1px solid #313244;
}

.add-form input,
.add-form select {
  background: #1e1e2e;
  border: 1px solid #45475a;
  color: #cdd6f4;
  padding: 4px 8px;
  font-size: 12px;
  outline: none;
}

.add-form button {
  background: #89b4fa;
  border: none;
  color: #1e1e2e;
  cursor: pointer;
  padding: 6px;
  font-weight: 600;
  border-radius: 4px;
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
