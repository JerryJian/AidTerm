<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
  command: string
  aiMessage: string
}>()

const emit = defineEmits<{
  confirm: []
  cancel: []
  modify: [newCommand: string]
}>()

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
  <div class="confirm-overlay" @click.self="doCancel">
    <div class="confirm-box" @click.stop>
      <div class="confirm-header">
        <span class="confirm-icon">🤖</span>
        <span class="confirm-title">AI 建议执行命令</span>
      </div>
      <div v-if="aiMessage" class="ai-message">{{ aiMessage }}</div>
      <div v-if="!editing" class="command-box">
        <pre class="command-text">{{ command }}</pre>
      </div>
      <div v-else class="command-box">
        <input v-model="editedCommand" class="command-input" @keydown.enter="submitEdit" />
      </div>
      <div class="confirm-actions">
        <button class="btn btn-cancel" @click="doCancel">取消</button>
        <button class="btn btn-edit" @click="startEdit">修改</button>
        <button class="btn btn-confirm" @click="doConfirm">执行</button>
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
  background: #1e1e2e;
  border: 1px solid #89b4fa;
  border-radius: 8px;
  padding: 20px;
  min-width: 400px;
  max-width: 600px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
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
  color: #89b4fa;
}

.ai-message {
  font-size: 13px;
  color: #a6adc8;
  margin-bottom: 12px;
  line-height: 1.5;
}

.command-box {
  background: #181825;
  border: 1px solid #45475a;
  border-radius: 4px;
  padding: 10px;
  margin-bottom: 12px;
}

.command-text {
  font-family: Consolas, "Courier New", monospace;
  font-size: 13px;
  color: #a6e3a1;
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
}

.command-input {
  width: 100%;
  padding: 8px;
  background: #11111b;
  border: 1px solid #89b4fa;
  border-radius: 4px;
  color: #a6e3a1;
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
  background: #45475a;
  color: #cdd6f4;
}
.btn-cancel:hover {
  background: #585b70;
}

.btn-edit {
  background: #313244;
  color: #f9e2af;
  border: 1px solid #f9e2af;
}
.btn-edit:hover {
  background: #45475a;
}

.btn-confirm {
  background: #a6e3a1;
  color: #1e1e2e;
  font-weight: 600;
}
.btn-confirm:hover {
  background: #94e2d5;
}
</style>
