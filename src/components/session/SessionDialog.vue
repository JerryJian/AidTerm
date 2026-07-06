<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useSessionStore } from '../../stores/sessionStore'
import type { SavedSession } from '../../types'

const props = defineProps<{
  session?: SavedSession
}>()

const emit = defineEmits<{
  save: [data: { name: string; type: 'ssh' | 'telnet'; host: string; port: number; username: string; groupName: string }]
  close: []
}>()

const store = useSessionStore()

const name = ref(props.session?.name || '')
const sessionType = ref<'ssh' | 'telnet'>(props.session?.session_type === 'telnet' ? 'telnet' : 'ssh')
const host = ref(props.session?.host || '')
const port = ref(props.session?.port || 22)
const username = ref(props.session?.username || '')
const groupName = ref('')

const existingGroupNames = computed(() => store.groups.map(g => g.name))
const isEditing = computed(() => !!props.session)

const firstInput = ref<HTMLInputElement>()

onMounted(() => {
  firstInput.value?.focus()
  if (props.session?.group_id) {
    const g = store.groups.find(gg => gg.id === props.session!.group_id)
    if (g) groupName.value = g.name
  }
})

function onSubmit() {
  if (!name.value.trim() || !host.value.trim()) return
  emit('save', {
    name: name.value.trim(),
    type: sessionType.value,
    host: host.value.trim(),
    port: port.value,
    username: username.value.trim(),
    groupName: groupName.value.trim(),
  })
}

function onBackdropClick(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close')
}
</script>

<template>
  <div class="overlay" @click="onBackdropClick">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ isEditing ? 'Edit Session' : 'New Session' }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">Name</span>
          <input ref="firstInput" v-model="name" type="text" class="input" placeholder="My Server" required />
        </label>
        <label class="field">
          <span class="field-label">Type</span>
          <select v-model="sessionType" class="input">
            <option value="ssh">SSH</option>
            <option value="telnet">Telnet</option>
          </select>
        </label>
        <label class="field">
          <span class="field-label">Host</span>
          <input v-model="host" type="text" class="input" placeholder="192.168.1.1" required />
        </label>
        <label class="field">
          <span class="field-label">Port</span>
          <input v-model.number="port" type="number" class="input" min="1" max="65535" />
        </label>
        <label class="field">
          <span class="field-label">Username</span>
          <input v-model="username" type="text" class="input" placeholder="root" />
        </label>
        <label class="field">
          <span class="field-label">Group</span>
          <input v-model="groupName" type="text" class="input" placeholder="Select or type new group name" list="group-list" />
          <datalist id="group-list">
            <option v-for="g in existingGroupNames" :key="g" :value="g" />
          </datalist>
        </label>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">Cancel</button>
          <button type="submit" class="btn btn-save">{{ isEditing ? 'Save' : 'Create' }}</button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.dialog {
  background: #1e1e2e;
  border: 1px solid #313244;
  border-radius: 8px;
  min-width: 360px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid #313244;
  font-size: 14px;
  font-weight: 600;
}

.dialog-close {
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
}
.dialog-close:hover {
  background: #313244;
  color: #cdd6f4;
}

.dialog-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field-label {
  font-size: 12px;
  color: #a6adc8;
}

.input {
  padding: 8px 10px;
  background: #181825;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: #89b4fa;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}

.btn-cancel {
  background: #313244;
  color: #cdd6f4;
}
.btn-cancel:hover {
  background: #45475a;
}

.btn-save {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}
.btn-save:hover {
  background: #74c7ec;
}
</style>
