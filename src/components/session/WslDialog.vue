<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@/api'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const emit = defineEmits<{
  connect: [config: { distro?: string; workingDir?: string }]
  close: []
}>()

const availableDistros = ref<string[]>([])
const distro = ref('')
const workingDir = ref('')
const refreshing = ref(false)

async function refreshDistros() {
  refreshing.value = true
  try {
    const list = await invoke<string[]>('wsl_list_distros')
    availableDistros.value = list
    if (list.length > 0 && !distro.value) {
      distro.value = list[0]
    }
  } catch (e) {
    console.error('Failed to list WSL distros:', e)
  } finally {
    refreshing.value = false
  }
}

onMounted(() => {
  refreshDistros()
  const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
  document.addEventListener('keydown', onKey)
  onUnmounted(() => document.removeEventListener('keydown', onKey))
})

function onSubmit() {
  emit('connect', {
    distro: distro.value || undefined,
    workingDir: workingDir.value.trim() || undefined,
  })
}
</script>

<template>
  <div class="overlay">
    <div class="dialog">
      <div class="dialog-header">
        <span>{{ t('wsl_dialog.title') }}</span>
        <button class="dialog-close" @click="emit('close')">✕</button>
      </div>
      <form class="dialog-body" @submit.prevent="onSubmit">
        <label class="field">
          <span class="field-label">{{ t('wsl_dialog.distro') }}</span>
          <div class="distro-select-row">
            <select v-model="distro" class="input">
              <option value="">{{ t('wsl_dialog.distro_placeholder') }}</option>
              <option v-for="d in availableDistros" :key="d" :value="d">{{ d }}</option>
            </select>
            <button type="button" class="refresh-btn" @click="refreshDistros" :disabled="refreshing">
              {{ refreshing ? '...' : '↻' }}
            </button>
          </div>
        </label>
        <label class="field">
          <span class="field-label">{{ t('wsl_dialog.working_dir') }}</span>
          <input v-model="workingDir" class="input" type="text" :placeholder="t('wsl_dialog.working_dir_placeholder')" />
        </label>
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" @click="emit('close')">{{ t('wsl_dialog.cancel') }}</button>
          <button type="submit" class="btn btn-connect">{{ t('wsl_dialog.connect') }}</button>
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

.distro-select-row {
  display: flex;
  gap: 6px;
}

.distro-select-row .input {
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
