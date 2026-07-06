<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useSnippetStore } from '../../stores/snippetStore'
import { useTerminalStore } from '../../stores/terminal'

const { t } = useI18n()

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
      <span>{{ t('snippet.title') }}</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>

    <div class="panel-body">
      <button class="btn btn-add" @click="resetForm">+ {{ t('snippet.add') }}</button>

      <div v-for="s in store.snippets.value" :key="s.id" class="snippet-item">
        <div class="snippet-info" @click="sendSnippet(s.command)" :title="s.command">
          <strong>{{ s.name }}</strong>
          <span class="snippet-cmd">{{ s.command }}</span>
        </div>
        <div class="snippet-actions">
          <button class="btn-sm" @click="sendSnippet(s.command)" :title="t('snippet.send')">▶</button>
          <button class="btn-sm" @click="editSnippet(s)">✎</button>
          <button class="btn-sm btn-danger" @click="store.remove(s.id)" :title="t('common.delete')">✕</button>
        </div>
      </div>
      <div v-if="store.snippets.value.length === 0 && !showForm" class="empty">
        {{ t('snippet.no_snippets') }}
      </div>
    </div>

    <!-- Add/Edit form -->
    <div v-if="showForm" class="form-overlay" @click.self="showForm = false">
      <div class="form-card">
        <div class="form-header">
          <span>{{ editId ? t('snippet.edit') : t('snippet.add') }}</span>
          <button class="panel-close" @click="showForm = false">✕</button>
        </div>
        <form class="form-body" @submit.prevent="submitForm">
          <label class="field">
            <span class="field-label">{{ t('snippet.name') }}</span>
            <input v-model="formName" type="text" class="input" required />
          </label>
          <label class="field">
            <span class="field-label">{{ t('snippet.command') }}</span>
            <textarea v-model="formCommand" class="input textarea" rows="3" placeholder="ssh root@{{host}}" required />
            <span class="field-hint">{{ t('snippet.variable_usage_hint') }}</span>
          </label>
          <div class="form-actions">
            <button type="button" class="btn btn-cancel" @click="showForm = false">{{ t('snippet.cancel') }}</button>
            <button type="submit" class="btn btn-save">{{ t('snippet.confirm') }}</button>
          </div>
        </form>
      </div>
    </div>

    <!-- Variable input dialog -->
    <div v-if="varDialogVisible" class="form-overlay" @click.self="varDialogVisible = false">
      <div class="form-card">
        <div class="form-header">
          <span>{{ t('snippet.variable_hint') }}</span>
          <button class="panel-close" @click="varDialogVisible = false">✕</button>
        </div>
        <div class="form-body">
          <label v-for="k in varKeys" :key="k" class="field">
            <span class="field-label">{{ k }}</span>
            <input v-model="varMap[k]" type="text" class="input" :placeholder="k" />
          </label>
          <div class="form-actions">
            <button type="button" class="btn btn-cancel" @click="varDialogVisible = false">{{ t('snippet.cancel') }}</button>
            <button type="button" class="btn btn-save" @click="submitVariables">{{ t('snippet.send') }}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--bg-surface0);
  font-size: 13px;
  font-weight: 600;
}

.panel-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 14px;
}
.panel-close:hover {
  background: var(--bg-surface0);
  color: var(--text);
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
  background: var(--bg-mantle);
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
  color: var(--accent);
}

.snippet-cmd {
  color: var(--text-overlay0);
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
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.btn-sm:hover {
  background: var(--bg-surface1);
}
.btn-danger:hover {
  border-color: var(--danger);
  color: var(--danger);
}

.empty {
  color: var(--text-overlay0);
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
  background: var(--bg-surface0);
  color: var(--accent);
  border: 1px solid var(--bg-surface1);
  width: 100%;
}
.btn-add:hover {
  background: var(--bg-surface1);
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

.form-overlay {
  position: fixed;
  inset: 0;
  background: var(--overlay);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
}

.form-card {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  min-width: 380px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}

.form-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--bg-surface0);
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
  color: var(--text-sub0);
}
.field-hint {
  font-size: 11px;
  color: var(--text-overlay0);
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
