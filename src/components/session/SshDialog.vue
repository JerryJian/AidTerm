<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { useProxyStore } from '../../stores/proxyStore'
import type { SshConnectionInfo } from '../../types'

const props = withDefaults(defineProps<{
  initialHost?: string
  initialPort?: number
  initialUsername?: string
  initialPassword?: string
}>(), {
  initialPort: 22,
})

const emit = defineEmits<{
  connect: [info: SshConnectionInfo]
  close: []
}>()

const { t } = useI18n()
const proxyStore = useProxyStore()

const host = ref(props.initialHost || '')
const port = ref(props.initialPort || 22)
const username = ref(props.initialUsername || '')
const password = ref(props.initialPassword || '')
const privateKeyPath = ref('')
const selectedProxyId = ref<string>('')
const agentForwarding = ref(false)
const x11Forwarding = ref(false)

const firstInput = ref<HTMLInputElement>()

onMounted(async () => {
  firstInput.value?.focus()
  await proxyStore.refresh()
  const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
  document.addEventListener('keydown', onKey)
  onUnmounted(() => document.removeEventListener('keydown', onKey))
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
    proxyId: selectedProxyId.value || undefined,
    agentForwarding: agentForwarding.value,
    x11Forwarding: x11Forwarding.value,
  })
}

</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ t('ssh_dialog.title') }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.host') }}</span>
          <input ref="firstInput" v-model="host" type="text" class="input" placeholder="192.168.1.1" required />
        </label>
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.port') }}</span>
          <input v-model.number="port" type="number" class="input" min="1" max="65535" />
        </label>
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.username') }}</span>
          <input v-model="username" type="text" class="input" placeholder="root" required />
        </label>
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.password') }}</span>
          <input v-model="password" type="password" class="input" placeholder="password" />
        </label>
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.private_key') }}</span>
          <div class="key-row">
            <input v-model="privateKeyPath" type="text" class="input key-input" placeholder="~/.ssh/id_rsa" readonly />
            <button type="button" class="btn btn-browse" @click="pickKey">{{ t('ssh_dialog.browse') }}</button>
          </div>
        </label>
        <label class="field">
          <span class="field-label">{{ t('ssh_dialog.proxy') }}</span>
          <select v-model="selectedProxyId" class="input">
            <option value="">{{ t('ssh_dialog.none') }}</option>
            <option v-for="p in proxyStore.proxies.value" :key="p.id" :value="p.id">
              {{ p.name }} ({{ p.proxy_type === 'Http' ? 'HTTP' : p.proxy_type === 'Socks5' ? 'SOCKS5' : 'Jump' }})
            </option>
          </select>
        </label>
        <div class="checkbox-row">
          <label class="checkbox-label">
            <input type="checkbox" v-model="agentForwarding" />
            {{ t('ssh_dialog.agent_forwarding') }}
          </label>
          <label class="checkbox-label">
            <input type="checkbox" v-model="x11Forwarding" />
            {{ t('ssh_dialog.x11_forwarding') }}
          </label>
        </div>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">{{ t('ssh_dialog.cancel') }}</button>
          <button type="submit" class="btn btn-connect">{{ t('ssh_dialog.connect') }}</button>
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
  min-width: 380px;
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

.btn-connect {
  background: var(--accent);
  color: var(--bg-base);
  font-weight: 600;
}

.btn-connect:hover {
  background: var(--accent-hover);
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
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  background: var(--bg-surface0);
  color: var(--text);
  font-size: 13px;
  cursor: pointer;
  white-space: nowrap;
}

.btn-browse:hover {
  background: var(--bg-surface1);
}

.checkbox-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
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
