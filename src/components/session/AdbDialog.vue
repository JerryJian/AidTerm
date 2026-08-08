<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@/api'
import { useI18n } from 'vue-i18n'
import type { AdbDevice } from '../../types'

const { t } = useI18n()

const emit = defineEmits<{
  connect: [info: { serial: string; model: string; product: string }]
  close: []
}>()

const devices = ref<AdbDevice[]>([])
const selectedSerial = ref('')
const refreshing = ref(false)
const error = ref('')

const connectedDevices = computed(() => devices.value.filter(d => d.state === 'device'))

const selected = computed(() => devices.value.find(d => d.serial === selectedSerial.value))

async function refreshDevices() {
  refreshing.value = true
  error.value = ''
  try {
    devices.value = await invoke<AdbDevice[]>('adb_list_devices')
    const firstReady = connectedDevices.value[0]
    if (firstReady && !devices.value.some(d => d.serial === selectedSerial.value)) {
      selectedSerial.value = firstReady.serial
    }
  } catch (e) {
    error.value = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Failed to list devices'
    console.error('Failed to list adb devices:', e)
  } finally {
    refreshing.value = false
  }
}

function onSelect(serial: string) {
  if (devices.value.find(d => d.serial === serial)?.state === 'device') {
    selectedSerial.value = serial
  }
}

function onSubmit() {
  const dev = selected.value
  if (!dev || dev.state !== 'device') return
  emit('connect', { serial: dev.serial, model: dev.model, product: dev.product })
}

function deviceLabel(d: AdbDevice): string {
  const model = d.model.replace(/_/g, ' ')
  return d.serial === model || !model ? d.serial : `${d.serial} · ${model}`
}

onMounted(() => {
  refreshDevices()
  const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
  document.addEventListener('keydown', onKey)
  onUnmounted(() => document.removeEventListener('keydown', onKey))
})
</script>

<template>
  <div class="overlay">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ t('adb_dialog.title') }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <div class="dialog-body">
        <div v-if="error" class="adb-error">{{ error }}</div>
        <div class="port-select-row">
          <button class="refresh-btn" @click="refreshDevices" :disabled="refreshing">
            {{ refreshing ? '...' : '↻' }} {{ t('adb_dialog.refresh') }}
          </button>
        </div>
        <div v-if="devices.length === 0 && !refreshing" class="empty-hint">
          {{ t('adb_dialog.no_devices') }}
        </div>
        <div class="device-list">
          <button
            v-for="d in devices"
            :key="d.serial"
            type="button"
            class="device-item"
            :class="{
              selected: d.serial === selectedSerial,
              disabled: d.state !== 'device',
            }"
            @click="onSelect(d.serial)"
          >
            <span class="dev-icon">📱</span>
            <span class="dev-info">
              <span class="dev-serial">{{ d.serial }}</span>
              <span class="dev-meta">
                <span class="dev-state" :class="d.state">{{ d.state }}</span>
                <span v-if="d.model && d.model !== d.serial">{{ deviceLabel(d) }}</span>
              </span>
            </span>
          </button>
        </div>
        <div class="caveat-hint">{{ t('adb_dialog.caveat') }}</div>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">{{ t('adb_dialog.cancel') }}</button>
          <button type="button" class="btn btn-connect" :disabled="!selected || selected.state !== 'device'" @click="onSubmit">
            {{ t('adb_dialog.connect') }}
          </button>
        </div>
      </div>
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
  min-width: 420px;
  max-width: 480px;
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

.adb-error {
  padding: 8px 10px;
  border: 1px solid var(--danger);
  background: color-mix(in srgb, var(--danger) 12%, transparent);
  border-radius: 4px;
  color: var(--danger);
  font-size: 12px;
  word-break: break-all;
}

.port-select-row {
  display: flex;
  gap: 6px;
}

.refresh-btn {
  padding: 6px 12px;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
}
.refresh-btn:hover {
  background: var(--bg-surface1);
}
.refresh-btn:disabled {
  opacity: 0.5;
}

.empty-hint {
  padding: 12px;
  color: var(--text-overlay0);
  font-size: 12px;
  text-align: center;
  border: 1px dashed var(--bg-surface1);
  border-radius: 4px;
}

.device-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 260px;
  overflow-y: auto;
}

.device-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  text-align: left;
  font-size: 13px;
}
.device-item:hover {
  border-color: var(--accent);
}
.device-item.selected {
  border-color: var(--accent);
  background: var(--accent-glass);
}
.device-item.disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.dev-icon {
  font-size: 16px;
  flex-shrink: 0;
}

.dev-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.dev-serial {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dev-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-sub0);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dev-state {
  padding: 1px 6px;
  border-radius: 8px;
  font-size: 10px;
  text-transform: uppercase;
  flex-shrink: 0;
}
.dev-state.device {
  background: color-mix(in srgb, var(--success) 20%, transparent);
  color: var(--success);
}
.dev-state.unauthorized {
  background: color-mix(in srgb, var(--warning) 20%, transparent);
  color: var(--warning);
}
.dev-state.offline {
  background: var(--bg-surface1);
  color: var(--text-overlay0);
}

.caveat-hint {
  font-size: 11px;
  color: var(--text-overlay0);
  line-height: 1.5;
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

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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
</style>
