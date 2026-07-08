<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSessionStore } from '../../stores/sessionStore'
import type { SavedSession } from '../../types'

const { t } = useI18n()

const props = defineProps<{
  session?: SavedSession
}>()

const emit = defineEmits<{
  save: [data: { name: string; type: 'ssh' | 'telnet'; host: string; port: number; username: string; password: string; savePassword: boolean; groupName: string }]
  close: []
}>()

const store = useSessionStore()

const name = ref(props.session?.name || '')
const sessionType = ref<'ssh' | 'telnet'>(props.session?.session_type === 'telnet' ? 'telnet' : 'ssh')
const host = ref(props.session?.host || '')
const port = ref(props.session?.port || 22)
const username = ref(props.session?.username || '')
const password = ref(props.session?.password || '')
const savePassword = ref(!!props.session?.password)
const groupName = ref('')
const showNewGroup = ref(false)
const groupSelect = ref('')

const existingGroupNames = computed(() => store.groups.map(g => g.name))
const isEditing = computed(() => !!props.session)

const firstInput = ref<HTMLInputElement>()

onMounted(() => {
  firstInput.value?.focus()
  if (props.session?.group_id) {
    const g = store.groups.find(gg => gg.id === props.session!.group_id)
    if (g) {
      groupName.value = g.name
      groupSelect.value = g.name
    }
  }
})

function onGroupChange() {
  if (groupSelect.value === '__new__') {
    showNewGroup.value = true
    groupName.value = ''
  } else {
    showNewGroup.value = false
    groupName.value = groupSelect.value
  }
}

function onSubmit() {
  if (!name.value.trim() || !host.value.trim()) return
  emit('save', {
    name: name.value.trim(),
    type: sessionType.value,
    host: host.value.trim(),
    port: port.value,
    username: username.value.trim(),
    password: password.value,
    savePassword: savePassword.value,
    groupName: groupName.value.trim(),
  })
}

</script>

<template>
  <div class="overlay">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ isEditing ? t('session_dialog.title_edit') : t('session_dialog.title_new') }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">{{ t('session_dialog.name') }}</span>
          <input ref="firstInput" v-model="name" type="text" class="input" placeholder="My Server" required />
        </label>
        <label class="field">
          <span class="field-label">{{ t('session_dialog.type') }}</span>
          <select v-model="sessionType" class="input">
            <option value="ssh">SSH</option>
            <option value="telnet">Telnet</option>
          </select>
        </label>
        <label class="field">
          <span class="field-label">{{ t('session_dialog.host') }}</span>
          <input v-model="host" type="text" class="input" placeholder="192.168.1.1" required />
        </label>
        <label class="field">
          <span class="field-label">{{ t('session_dialog.port') }}</span>
          <input v-model.number="port" type="number" class="input" min="1" max="65535" />
        </label>
        <label class="field">
          <span class="field-label">{{ t('session_dialog.username') }}</span>
          <input v-model="username" type="text" class="input" placeholder="root" />
        </label>
        <template v-if="sessionType === 'ssh'">
          <label class="field">
            <span class="field-label">{{ t('session_dialog.password') }}</span>
            <input v-model="password" type="password" class="input" placeholder="password" />
          </label>
          <label class="checkbox-label">
            <input type="checkbox" v-model="savePassword" />
            {{ t('session_dialog.remember_password') }}
          </label>
        </template>
        <label class="field">
          <span class="field-label">{{ t('session_dialog.group') }}</span>
          <select v-model="groupSelect" class="input" @change="onGroupChange">
            <option value="">{{ t('session_dialog.no_group') }}</option>
            <option v-for="g in existingGroupNames" :key="g" :value="g">{{ g }}</option>
            <option value="__new__">{{ t('session_dialog.new_group') }}</option>
          </select>
          <input v-if="showNewGroup" v-model="groupName" type="text" class="input" :placeholder="t('session_dialog.new_group_placeholder')" style="margin-top: 4px" />
        </label>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">{{ t('session_dialog.cancel') }}</button>
          <button type="submit" class="btn btn-save">{{ isEditing ? t('session_dialog.save') : t('session_dialog.create') }}</button>
        </div>
      </form>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: var(--overlay);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.dialog {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  min-width: 360px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--bg-surface0);
  font-size: 14px;
  font-weight: 600;
}

.dialog-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
}
.dialog-close:hover {
  background: var(--bg-surface0);
  color: var(--text);
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
  color: var(--text-sub0);
}

.input {
  padding: 8px 10px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: var(--accent);
}
select.input {
  appearance: none;
  cursor: pointer;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23808080' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpolyline points='6 9 12 15 18 9'%3E%3C/polyline%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 8px center;
  padding-right: 28px;
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
  background: var(--bg-surface0);
  color: var(--text);
}
.btn-cancel:hover {
  background: var(--bg-surface1);
}

.btn-save {
  background: var(--accent);
  color: var(--bg-base);
  font-weight: 600;
}
.btn-save:hover {
  background: var(--accent-hover);
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
}
.checkbox-label input[type="checkbox"] {
  accent-color: var(--accent);
}
</style>
