<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import type { SshConnectionInfo } from '../../types'

const props = withDefaults(defineProps<{
  initialHost?: string
  initialPort?: number
  initialUsername?: string
}>(), {
  initialPort: 22,
})

const emit = defineEmits<{
  connect: [info: SshConnectionInfo]
  close: []
}>()

const host = ref(props.initialHost || '')
const port = ref(props.initialPort || 22)
const username = ref(props.initialUsername || '')
const password = ref('')
const privateKeyPath = ref('')

const firstInput = ref<HTMLInputElement>()

onMounted(() => {
  if (host.value) {
    password.value = ''
  }
  firstInput.value?.focus()
})

async function pickKey() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'SSH Keys', extensions: ['pem', 'key', 'id_rsa', 'id_ed25519', '*'] }],
  })
  if (selected) {
    privateKeyPath.value = selected
  }
}

function onSubmit() {
  if (!host.value || !username.value) return
  emit('connect', {
    host: host.value,
    port: port.value,
    username: username.value,
    password: password.value,
    privateKeyPath: privateKeyPath.value || undefined,
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
        <span>SSH Connection</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">Host</span>
          <input ref="firstInput" v-model="host" type="text" class="input" placeholder="192.168.1.1" required />
        </label>
        <label class="field">
          <span class="field-label">Port</span>
          <input v-model.number="port" type="number" class="input" min="1" max="65535" />
        </label>
        <label class="field">
          <span class="field-label">Username</span>
          <input v-model="username" type="text" class="input" placeholder="root" required />
        </label>
        <label class="field">
          <span class="field-label">Password</span>
          <input v-model="password" type="password" class="input" placeholder="password" />
        </label>
        <label class="field">
          <span class="field-label">Private Key (optional)</span>
          <div class="key-row">
            <input v-model="privateKeyPath" type="text" class="input key-input" placeholder="~/.ssh/id_rsa" readonly />
            <button type="button" class="btn btn-browse" @click="pickKey">Browse</button>
          </div>
        </label>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">Cancel</button>
          <button type="submit" class="btn btn-connect">Connect</button>
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
  min-width: 380px;
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

.btn-connect {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}

.btn-connect:hover {
  background: #74c7ec;
}

.key-row {
  display: flex;
  gap: 6px;
}

.key-input {
  flex: 1;
  cursor: default;
}

.btn-browse {
  padding: 8px 12px;
  border: 1px solid #45475a;
  border-radius: 4px;
  background: #313244;
  color: #cdd6f4;
  font-size: 13px;
  cursor: pointer;
  white-space: nowrap;
}

.btn-browse:hover {
  background: #45475a;
}
</style>
