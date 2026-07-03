<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSnippetStore } from '../../stores/snippetStore'
import { useTerminalStore } from '../../stores/terminal'

const emit = defineEmits<{ close: [] }>()

const store = useSnippetStore()
const termStore = useTerminalStore()

const showForm = ref(false)
const editId = ref<string | null>(null)
const formName = ref('')
const formCommand = ref('')

const varDialogVisible = ref(false)
const varMap = ref<Record<string, string>>({})
const varKeys = ref<string[]>([])
const pendingCommand = ref('')

function resetForm() {
  editId.value = null
  formName.value = ''
  formCommand.value = ''
  showForm.value = true
}

function editSnippet(s: { id: string; name: string; command: string }) {
  editId.value = s.id
  formName.value = s.name
  formCommand.value = s.command
  showForm.value = true
}

function submitForm() {
  if (!formName.value || !formCommand.value) return
  if (editId.value) {
    store.update(editId.value, { name: formName.value, command: formCommand.value })
  } else {
    store.add(formName.value, formCommand.value)
  }
  showForm.value = false
}

function parseVariables(cmd: string): string[] {
  const vars = new Set<string>()
  const re = /\{\{(\w+)\}\}/g
  let m
  while ((m = re.exec(cmd)) !== null) {
    vars.add(m[1])
  }
  return [...vars]
}

function replaceVariables(cmd: string, vars: Record<string, string>): string {
  return cmd.replace(/\{\{(\w+)\}\}/g, (_, key) => vars[key] ?? `{{${key}}}`
  )
}

async function sendSnippet(cmd: string) {
  const sessionId = termStore.activeTab?.session?.id
  if (!sessionId) return

  const vars = parseVariables(cmd)
  if (vars.length > 0) {
    const map: Record<string, string> = {}
    for (const v of vars) map[v] = ''
    varMap.value = map
    varKeys.value = vars
    pendingCommand.value = cmd
    varDialogVisible.value = true
    return
  }

  await invoke('write_terminal', { sessionId, data: cmd + '\n' })
}

function submitVariables() {
  const cmd = replaceVariables(pendingCommand.value, varMap.value)
  const sessionId = termStore.activeTab?.session?.id
  if (sessionId) {
    invoke('write_terminal', { sessionId, data: cmd + '\n' })
  }
  varDialogVisible.value = false
}
</script>

<template>
  <div class="panel">
    <div class="panel-header">
      <span>快捷命令</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>

    <div class="panel-body">
      <button class="btn btn-add" @click="resetForm">+ 添加命令</button>

      <div v-for="s in store.snippets.value" :key="s.id" class="snippet-item">
        <div class="snippet-info" @click="sendSnippet(s.command)" :title="s.command">
          <strong>{{ s.name }}</strong>
          <span class="snippet-cmd">{{ s.command }}</span>
        </div>
        <div class="snippet-actions">
          <button class="btn-sm" @click="sendSnippet(s.command)" title="发送">▶</button>
          <button class="btn-sm" @click="editSnippet(s)">✎</button>
          <button class="btn-sm btn-danger" @click="store.remove(s.id)">✕</button>
        </div>
      </div>
      <div v-if="store.snippets.value.length === 0 && !showForm" class="empty">
        暂无快捷命令
      </div>
    </div>

    <!-- Add/Edit form -->
    <div v-if="showForm" class="form-overlay" @click.self="showForm = false">
      <div class="form-card">
        <div class="form-header">
          <span>{{ editId ? '编辑命令' : '添加命令' }}</span>
          <button class="panel-close" @click="showForm = false">✕</button>
        </div>
        <form class="form-body" @submit.prevent="submitForm">
          <label class="field">
            <span class="field-label">名称</span>
            <input v-model="formName" type="text" class="input" placeholder="连接服务器" required />
          </label>
          <label class="field">
            <span class="field-label">命令</span>
            <textarea v-model="formCommand" class="input textarea" rows="3" placeholder="ssh root@{{host}}" required />
            <span class="field-hint" v-pre>使用 {{变量名}} 定义变量，发送时会提示输入</span>
          </label>
          <div class="form-actions">
            <button type="button" class="btn btn-cancel" @click="showForm = false">取消</button>
            <button type="submit" class="btn btn-save">保存</button>
          </div>
        </form>
      </div>
    </div>

    <!-- Variable input dialog -->
    <div v-if="varDialogVisible" class="form-overlay" @click.self="varDialogVisible = false">
      <div class="form-card">
        <div class="form-header">
          <span>输入变量值</span>
          <button class="panel-close" @click="varDialogVisible = false">✕</button>
        </div>
        <div class="form-body">
          <label v-for="k in varKeys" :key="k" class="field">
            <span class="field-label">{{ k }}</span>
            <input v-model="varMap[k]" type="text" class="input" :placeholder="k" />
          </label>
          <div class="form-actions">
            <button type="button" class="btn btn-cancel" @click="varDialogVisible = false">取消</button>
            <button type="button" class="btn btn-save" @click="submitVariables">发送</button>
          </div>
        </div>
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

.snippet-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px;
  background: #181825;
  border-radius: 4px;
  font-size: 12px;
}

.snippet-info {
  flex: 1;
  min-width: 0;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.snippet-info:hover strong {
  color: #89b4fa;
}

.snippet-cmd {
  color: #585b70;
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snippet-actions {
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
