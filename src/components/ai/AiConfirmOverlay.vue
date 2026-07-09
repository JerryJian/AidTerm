<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  command: string
  aiMessage: string
}>()

const emit = defineEmits<{
  confirm: []
  cancel: []
  modify: [newCommand: string]
}>()

const escHandler = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('cancel') }
onMounted(() => document.addEventListener('keydown', escHandler))
onUnmounted(() => document.removeEventListener('keydown', escHandler))

const editing = ref(false)
const editedCommand = ref(props.command)

function doConfirm() {
  emit('confirm')
}

function doCancel() {
  emit('cancel')
}

function startEdit() {
  editing.value = true
  editedCommand.value = props.command
}

function submitEdit() {
  emit('modify', editedCommand.value)
  editing.value = false
}
</script>

<template>
  <div class="confirm-overlay">
    <div class="confirm-box" @click.stop>
      <div class="confirm-header">
        <span class="confirm-icon">🤖</span>
        <span class="confirm-title">{{ t('ai.suggest_command') }}</span>
      </div>
      <div v-if="aiMessage" class="ai-message">{{ aiMessage }}</div>
      <div v-if="!editing" class="command-box">
        <pre class="command-text">{{ command }}</pre>
      </div>
      <div v-else class="command-box">
        <input v-model="editedCommand" class="command-input" @keydown.enter="submitEdit" />
      </div>
      <div class="confirm-actions">
        <button class="btn btn-cancel" @click="doCancel">{{ t('ai.cancel') }}</button>
        <button class="btn btn-edit" @click="startEdit">{{ t('ai.modify') }}</button>
        <button class="btn btn-confirm" @click="doConfirm">{{ t('ai.execute') }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.confirm-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 50;
}

.confirm-box {
  background: var(--bg-base);
  border: 1px solid var(--accent);
  border-radius: 8px;
  padding: 20px;
  min-width: 400px;
  max-width: 600px;
  box-shadow: 0 8px 32px var(--overlay);
}

.confirm-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}

.confirm-icon {
  font-size: 20px;
}

.confirm-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--accent);
}

.ai-message {
  font-size: 13px;
  color: var(--text-sub0);
  margin-bottom: 12px;
  line-height: 1.5;
}

.command-box {
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  padding: 10px;
  margin-bottom: 12px;
}

.command-text {
  font-family: Consolas, "Courier New", monospace;
  font-size: 13px;
  color: var(--success);
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
}

.command-input {
  width: 100%;
  padding: 8px;
  background: var(--bg-crust);
  border: 1px solid var(--accent);
  border-radius: 4px;
  color: var(--success);
  font-family: Consolas, "Courier New", monospace;
  font-size: 13px;
  outline: none;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  font-weight: 500;
}

.btn-cancel {
  background: var(--bg-surface1);
  color: var(--text);
}
.btn-cancel:hover {
  background: var(--text-overlay0);
}

.btn-edit {
  background: var(--bg-surface0);
  color: var(--warning);
  border: 1px solid var(--warning);
}
.btn-edit:hover {
  background: var(--bg-surface1);
}

.btn-confirm {
  background: var(--success);
  color: var(--bg-base);
  font-weight: 600;
}
.btn-confirm:hover {
  background: var(--teal);
}
</style>
