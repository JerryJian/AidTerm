<script setup lang="ts">
import { ref } from 'vue'
import { useTriggerStore } from '../../stores/triggerStore'

const emit = defineEmits<{ close: [] }>()

const store = useTriggerStore()

const showForm = ref(false)
const editId = ref<string | null>(null)
const formName = ref('')
const formPattern = ref('')
const formResponse = ref('')
const formCooldown = ref(3000)

function resetForm() {
  editId.value = null
  formName.value = ''
  formPattern.value = ''
  formResponse.value = ''
  formCooldown.value = 3000
  showForm.value = true
}

function editTrigger(t: { id: string; name: string; pattern: string; response: string; cooldown_ms: number }) {
  editId.value = t.id
  formName.value = t.name
  formPattern.value = t.pattern
  formResponse.value = t.response
  formCooldown.value = t.cooldown_ms
  showForm.value = true
}

function submitForm() {
  if (!formName.value || !formPattern.value || !formResponse.value) return
  if (editId.value) {
    store.update(editId.value, {
      name: formName.value,
      pattern: formPattern.value,
      response: formResponse.value,
      cooldown_ms: formCooldown.value,
    })
  } else {
    store.add(formName.value, formPattern.value, formResponse.value, formCooldown.value)
  }
  showForm.value = false
}

function toggleEnabled(t: { id: string; enabled: boolean }) {
  store.update(t.id, { enabled: !t.enabled })
}
</script>

<template>
  <div class="panel">
    <div class="panel-header">
      <span>触发器</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>

    <div class="panel-body">
      <button class="btn btn-add" @click="resetForm">+ 添加触发器</button>

      <div v-for="t in store.triggers.value" :key="t.id" class="trigger-item" :class="{ disabled: !t.enabled }">
        <div class="trigger-info">
          <div class="trigger-name-row">
            <strong>{{ t.name }}</strong>
            <span class="trigger-badge" :class="{ on: t.enabled, off: !t.enabled }" @click="toggleEnabled(t)">
              {{ t.enabled ? 'ON' : 'OFF' }}
            </span>
          </div>
          <span class="trigger-detail">匹配: {{ t.pattern }}</span>
          <span class="trigger-detail">响应: {{ t.response }}</span>
        </div>
        <div class="trigger-actions">
          <button class="btn-sm" @click="editTrigger(t)">编辑</button>
          <button class="btn-sm btn-danger" @click="store.remove(t.id)">删除</button>
        </div>
      </div>
      <div v-if="store.triggers.value.length === 0 && !showForm" class="empty">
        暂无触发器
      </div>
    </div>

    <!-- Add/Edit form -->
    <div v-if="showForm" class="form-overlay" @click.self="showForm = false">
      <div class="form-card">
        <div class="form-header">
          <span>{{ editId ? '编辑触发器' : '添加触发器' }}</span>
          <button class="panel-close" @click="showForm = false">✕</button>
        </div>
        <form class="form-body" @submit.prevent="submitForm">
          <label class="field">
            <span class="field-label">名称</span>
            <input v-model="formName" type="text" class="input" placeholder="自动登录" required />
          </label>
          <label class="field">
            <span class="field-label">匹配模式 (正则)</span>
            <input v-model="formPattern" type="text" class="input" placeholder="[Pp]assword:" required />
          </label>
          <label class="field">
            <span class="field-label">响应命令</span>
            <textarea v-model="formResponse" class="input textarea" rows="2" placeholder="mypassword" required />
          </label>
          <label class="field">
            <span class="field-label">冷却时间 (毫秒)</span>
            <input v-model.number="formCooldown" type="number" class="input" min="0" step="500" />
            <span class="field-hint">防止触发器在短时间内重复触发</span>
          </label>
          <div class="form-actions">
            <button type="button" class="btn btn-cancel" @click="showForm = false">取消</button>
            <button type="submit" class="btn btn-save">保存</button>
          </div>
        </form>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  width: 280px;
  background: #1e1e2e;
  border-left: 1px solid #313244;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid #313244;
  font-size: 13px;
  font-weight: 600;
}

.panel-close {
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 14px;
}
.panel-close:hover {
  background: #313244;
  color: #cdd6f4;
}

.panel-body {
  flex: 1;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.trigger-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 8px;
  background: #181825;
  border-radius: 4px;
  font-size: 12px;
}
.trigger-item.disabled {
  opacity: 0.5;
}

.trigger-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.trigger-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.trigger-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 3px;
  cursor: pointer;
  font-weight: 600;
}
.trigger-badge.on {
  background: #1e3a2f;
  color: #a6e3a1;
}
.trigger-badge.off {
  background: #3a1e1e;
  color: #f38ba8;
}

.trigger-detail {
  color: #585b70;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.trigger-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
  margin-left: 8px;
}

.btn-sm {
  padding: 4px 8px;
  border: 1px solid #45475a;
  background: #313244;
  color: #cdd6f4;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.btn-sm:hover {
  background: #45475a;
}
.btn-danger:hover {
  border-color: #f38ba8;
  color: #f38ba8;
}

.empty {
  color: #585b70;
  font-size: 12px;
  text-align: center;
  padding: 20px;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}
.btn-add {
  background: #313244;
  color: #89b4fa;
  border: 1px solid #45475a;
  width: 100%;
}
.btn-add:hover {
  background: #45475a;
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

.form-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
}

.form-card {
  background: #1e1e2e;
  border: 1px solid #313244;
  border-radius: 8px;
  min-width: 380px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}

.form-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid #313244;
  font-size: 14px;
  font-weight: 600;
}

.form-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
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
.field-hint {
  font-size: 11px;
  color: #585b70;
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
.textarea {
  resize: vertical;
  font-family: Consolas, "Courier New", monospace;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}
</style>
