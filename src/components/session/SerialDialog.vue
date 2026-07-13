<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@/api'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const emit = defineEmits<{
  connect: [config: {
    portName: string
    baudRate: number
    dataBits: number
    stopBits: number
    parity: string
    flowControl: string
  }]
  close: []
}>()

const availablePorts = ref<{ port_name: string }[]>([])
const portName = ref('')
const baudRate = ref(115200)
const dataBits = ref(8)
const stopBits = ref(1)
const parity = ref('None')
const flowControl = ref('None')
const refreshing = ref(false)

const baudRates = [300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600]
const dataBitsOptions = [5, 6, 7, 8]
const stopBitsOptions = [1, 2]
const parityOptions = ['None', 'Odd', 'Even']
const flowControlOptions = ['None', 'Software', 'Hardware']

async function refreshPorts() {
  refreshing.value = true
  try {
    const ports = await invoke<{ port_name: string }[]>('serial_list_ports')
    availablePorts.value = ports
    if (ports.length > 0 && !portName.value) {
      portName.value = ports[0].port_name
    }
  } catch (e) {
    console.error('Failed to list serial ports:', e)
  } finally {
    refreshing.value = false
  }
}

onMounted(() => {
  refreshPorts()
  const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
  document.addEventListener('keydown', onKey)
  onUnmounted(() => document.removeEventListener('keydown', onKey))
})

function onSubmit() {
  if (!portName.value) return
  emit('connect', {
    portName: portName.value,
    baudRate: baudRate.value,
    dataBits: dataBits.value,
    stopBits: stopBits.value,
    parity: parity.value,
    flowControl: flowControl.value,
  })
}
</script>

<template>
  <div class="overlay">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ t('serial_dialog.title') }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">{{ t('serial_dialog.port') }}</span>
          <div class="port-select-row">
            <select v-model="portName" class="input" required>
              <option value="" disabled>{{ t('serial_dialog.select_port') }}</option>
              <option v-for="p in availablePorts" :key="p.port_name" :value="p.port_name">{{ p.port_name }}</option>
            </select>
            <button type="button" class="refresh-btn" @click="refreshPorts" :disabled="refreshing">
              {{ refreshing ? '...' : '↻' }}
            </button>
          </div>
        </label>
        <div class="field-row">
          <label class="field">
            <span class="field-label">{{ t('serial_dialog.baud_rate') }}</span>
            <select v-model.number="baudRate" class="input">
              <option v-for="r in baudRates" :key="r" :value="r">{{ r }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">{{ t('serial_dialog.data_bits') }}</span>
            <select v-model.number="dataBits" class="input">
              <option v-for="d in dataBitsOptions" :key="d" :value="d">{{ d }}</option>
            </select>
          </label>
        </div>
        <div class="field-row">
          <label class="field">
            <span class="field-label">{{ t('serial_dialog.stop_bits') }}</span>
            <select v-model.number="stopBits" class="input">
              <option v-for="s in stopBitsOptions" :key="s" :value="s">{{ s }}</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">{{ t('serial_dialog.parity') }}</span>
            <select v-model="parity" class="input">
              <option v-for="p in parityOptions" :key="p" :value="p">{{ p }}</option>
            </select>
          </label>
        </div>
        <label class="field">
          <span class="field-label">{{ t('serial_dialog.flow_control') }}</span>
          <select v-model="flowControl" class="input">
            <option v-for="f in flowControlOptions" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">{{ t('serial_dialog.cancel') }}</button>
          <button type="submit" class="btn btn-connect">{{ t('serial_dialog.connect') }}</button>
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
  flex: 1;
}

.field-label {
  font-size: 12px;
  color: var(--text-sub0);
}

.field-row {
  display: flex;
  gap: 12px;
}

.port-select-row {
  display: flex;
  gap: 6px;
}

.port-select-row .input {
  flex: 1;
}

.refresh-btn {
  padding: 8px 10px;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
}
.refresh-btn:hover {
  background: var(--bg-surface1);
}
.refresh-btn:disabled {
  opacity: 0.5;
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

.btn-connect {
  background: var(--accent);
  color: var(--bg-base);
  font-weight: 600;
}
.btn-connect:hover {
  background: var(--accent-hover);
}
</style>
