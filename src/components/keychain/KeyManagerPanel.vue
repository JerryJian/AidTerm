<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { KeyInfo } from '../../types'

const emit = defineEmits<{
  close: []
}>()

const keys = ref<KeyInfo[]>([])
const loading = ref(true)
const showGenerate = ref(false)
const showImport = ref(false)

const genType = ref<'RSA' | 'ED25519'>('ED25519')
const genName = ref('')
const genBits = ref(4096)
const genPassphrase = ref('')

const importName = ref('')
const importPath = ref('')

const notification = ref('')
const error = ref('')

onMounted(async () => {
  await loadKeys()
})

async function loadKeys() {
  loading.value = true
  try {
    keys.value = await invoke<KeyInfo[]>('key_list')
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function doGenerate() {
  if (!genName.value.trim()) return
  error.value = ''
  notification.value = ''
  try {
    let result: KeyInfo
    if (genType.value === 'RSA') {
      result = await invoke<KeyInfo>('key_generate_rsa', {
        name: genName.value.trim(),
        bits: genBits.value,
        passphrase: genPassphrase.value || null,
      })
    } else {
      result = await invoke<KeyInfo>('key_generate_ed25519', {
        name: genName.value.trim(),
        passphrase: genPassphrase.value || null,
      })
    }
    keys.value.push(result)
    notification.value = `密钥已生成: ${result.name} (${result.fingerprint})`
    genName.value = ''
    genPassphrase.value = ''
    showGenerate.value = false
  } catch (e: any) {
    error.value = String(e)
  }
}

async function doImport() {
  if (!importName.value.trim() || !importPath.value) return
  error.value = ''
  notification.value = ''
  try {
    const result = await invoke<KeyInfo>('key_import', {
      name: importName.value.trim(),
      privateKeyPath: importPath.value,
    })
    keys.value.push(result)
    notification.value = `已导入: ${result.name}`
    importName.value = ''
    importPath.value = ''
    showImport.value = false
  } catch (e: any) {
    error.value = String(e)
  }
}

async function doDelete(id: string) {
  error.value = ''
  notification.value = ''
  try {
    await invoke('key_delete', { id })
    keys.value = keys.value.filter(k => k.id !== id)
    notification.value = '密钥已删除'
  } catch (e: any) {
    error.value = String(e)
  }
}

async function pickKeyFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'SSH Keys', extensions: ['pem', 'key', 'id_rsa', 'id_ed25519', '*'] }],
  })
  if (selected) {
    importPath.value = selected
  }
}

function copyToClipboard(text: string) {
  navigator.clipboard.writeText(text)
  notification.value = '已复制到剪贴板'
}
</script>

<template>
  <div class="key-panel">
    <div class="panel-header">
      <span class="panel-title">🔑 密钥管理</span>
      <div class="panel-actions">
        <button class="panel-btn" @click="showGenerate = !showGenerate">生成</button>
        <button class="panel-btn" @click="showImport = !showImport">导入</button>
        <button class="panel-btn" @click="emit('close')">✕</button>
      </div>
    </div>

    <div v-if="notification" class="notification">{{ notification }}</div>
    <div v-if="error" class="error">{{ error }}</div>

    <!-- Generate Form -->
    <div v-if="showGenerate" class="form-section">
      <h4>生成密钥对</h4>
      <select v-model="genType" class="input">
        <option value="ED25519">ED25519 (推荐)</option>
        <option value="RSA">RSA</option>
      </select>
      <input v-model="genName" placeholder="密钥名称" class="input" />
      <div v-if="genType === 'RSA'" class="bits-row">
        <label>位数:</label>
        <select v-model="genBits" class="input">
          <option :value="2048">2048</option>
          <option :value="4096">4096</option>
        </select>
      </div>
      <input v-model="genPassphrase" type="password" placeholder="密码短语 (可选)" class="input" />
      <button class="btn btn-primary" @click="doGenerate">生成</button>
    </div>

    <!-- Import Form -->
    <div v-if="showImport" class="form-section">
      <h4>导入密钥</h4>
      <input v-model="importName" placeholder="密钥名称" class="input" />
      <div class="key-row">
        <input v-model="importPath" placeholder="私钥文件路径" class="input key-input" readonly />
        <button class="btn btn-browse" @click="pickKeyFile">浏览</button>
      </div>
      <button class="btn btn-primary" @click="doImport">导入</button>
    </div>

    <!-- Key List -->
    <div class="key-list">
      <div v-if="loading" class="loading">加载中...</div>
      <div v-else-if="keys.length === 0" class="empty">暂无密钥</div>
      <div v-for="key in keys" :key="key.id" class="key-item">
        <div class="key-info">
          <span class="key-name">{{ key.name }}</span>
          <span class="key-type">{{ key.key_type }} {{ key.bits > 0 ? key.bits : '' }}</span>
        </div>
        <div class="key-fingerprint">{{ key.fingerprint }}</div>
        <div class="key-actions">
          <button class="action-btn" title="复制公钥" @click="copyToClipboard(key.public_key)">📋</button>
          <button class="action-btn" title="复制私钥路径" @click="copyToClipboard(key.private_key_path)">📁</button>
          <button class="action-btn danger" title="删除" @click="doDelete(key.id)">🗑</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.key-panel {
  width: 320px;
  min-width: 320px;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  height: 100%;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.panel-actions {
  display: flex;
  gap: 4px;
}

.panel-btn {
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  padding: 4px 10px;
  border-radius: 4px;
  font-size: 11px;
}
.panel-btn:hover {
  background: var(--bg-surface1);
}

.notification {
  padding: 8px 12px;
  background: #1e3a2e;
  color: var(--success);
  font-size: 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.error {
  padding: 8px 12px;
  background: #3a1e1e;
  color: var(--danger);
  font-size: 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.form-section {
  padding: 12px;
  border-bottom: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.form-section h4 {
  font-size: 12px;
  color: var(--accent);
  margin: 0;
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

.bits-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.bits-row label {
  font-size: 12px;
  color: var(--text-sub0);
}
.bits-row select {
  flex: 1;
}

.key-row {
  display: flex;
  gap: 6px;
}
.key-input {
  flex: 1;
  cursor: default;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}
.btn-primary {
  background: var(--accent);
  color: var(--bg-base);
  font-weight: 600;
}
.btn-primary:hover {
  background: var(--accent-hover);
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

.key-list {
  flex: 1;
  overflow-y: auto;
}

.loading, .empty {
  padding: 24px 12px;
  text-align: center;
  color: var(--text-overlay0);
  font-size: 12px;
}

.key-item {
  padding: 10px 12px;
  border-bottom: 1px solid var(--bg-surface0);
}

.key-info {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.key-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.key-type {
  font-size: 11px;
  color: var(--accent);
  background: var(--bg-mantle);
  padding: 2px 6px;
  border-radius: 4px;
}

.key-fingerprint {
  font-size: 11px;
  color: var(--text-overlay0);
  font-family: monospace;
  margin-bottom: 6px;
}

.key-actions {
  display: flex;
  gap: 4px;
}

.action-btn {
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
}
.action-btn:hover {
  background: var(--bg-surface1);
}
.action-btn.danger:hover {
  background: var(--danger);
  color: var(--bg-base);
}
</style>
