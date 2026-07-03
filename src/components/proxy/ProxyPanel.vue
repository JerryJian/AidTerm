<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useProxyStore } from '../../stores/proxyStore'
import type { ProxyConfig, ProxyType } from '../../types'

const emit = defineEmits<{ close: [] }>()

const store = useProxyStore()

const showForm = ref(false)
const editId = ref<string | null>(null)
const formName = ref('')
const formType = ref<ProxyType>('Http')
const formHost = ref('')
const formPort = ref(3128)
const formUsername = ref('')
const formPassword = ref('')
const formKeyPath = ref('')

onMounted(() => {
  store.refresh()
})

function resetForm() {
  editId.value = null
  formName.value = ''
  formType.value = 'Http'
  formHost.value = ''
  formPort.value = 3128
  formUsername.value = ''
  formPassword.value = ''
  formKeyPath.value = ''
  showForm.value = true
}

function editProxy(p: ProxyConfig) {
  editId.value = p.id
  formName.value = p.name
  formType.value = p.proxy_type
  formHost.value = p.host
  formPort.value = p.port
  formUsername.value = p.username ?? ''
  formPassword.value = p.password ?? ''
  formKeyPath.value = p.private_key_path ?? ''
  showForm.value = true
}

async function pickKey() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'SSH Keys', extensions: ['pem', 'key', 'id_rsa', 'id_ed25519', '*'] }],
  })
  if (selected) formKeyPath.value = selected
}

async function submitForm() {
  if (!formName.value || !formHost.value || !formPort.value) return
  const config: ProxyConfig = {
    id: editId.value ?? store.genId(),
    name: formName.value,
    proxy_type: formType.value,
    host: formHost.value,
    port: formPort.value,
    username: formUsername.value || null,
    password: formPassword.value || null,
    private_key_path: formKeyPath.value || null,
  }
  await store.save(config)
  showForm.value = false
}

async function deleteProxy(id: string) {
  await store.remove(id)
}

function typeLabel(t: ProxyType): string {
  if (t === 'Http') return 'HTTP CONNECT'
  if (t === 'Socks5') return 'SOCKS5'
  return 'Jump Host'
}

function typeHint(t: ProxyType): string {
  if (t === 'Http') return 'HTTP CONNECT 代理端口 (常用 3128, 8080)'
  if (t === 'Socks5') return 'SOCKS5 代理端口 (常用 1080)'
  return 'Jump Host SSH 端口 (常用 22)'
}
</script>

<template>
  <div class="panel">
    <div class="panel-header">
      <span>代理配置</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>

    <div class="panel-body">
      <button class="btn btn-add" @click="resetForm">+ 添加代理</button>

      <div v-for="p in store.proxies.value" :key="p.id" class="proxy-item">
        <div class="proxy-info">
          <strong>{{ p.name }}</strong>
          <span class="proxy-type">{{ typeLabel(p.proxy_type) }}</span>
          <span class="proxy-addr">{{ p.host }}:{{ p.port }}</span>
        </div>
        <div class="proxy-actions">
          <button class="btn-sm" @click="editProxy(p)">编辑</button>
          <button class="btn-sm btn-danger" @click="deleteProxy(p.id)">删除</button>
        </div>
      </div>
      <div v-if="store.proxies.value.length === 0 && !showForm" class="empty">
        暂无代理配置
      </div>
    </div>

    <div v-if="showForm" class="form-overlay" @click.self="showForm = false">
      <div class="form-card">
        <div class="form-header">
          <span>{{ editId ? '编辑代理' : '添加代理' }}</span>
          <button class="panel-close" @click="showForm = false">✕</button>
        </div>
        <form class="form-body" @submit.prevent="submitForm">
          <label class="field">
            <span class="field-label">名称</span>
            <input v-model="formName" type="text" class="input" placeholder="My Proxy" required />
          </label>
          <label class="field">
            <span class="field-label">类型</span>
            <select v-model="formType" class="input">
              <option value="Http">HTTP CONNECT</option>
              <option value="Socks5">SOCKS5</option>
              <option value="JumpHost">Jump Host</option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">代理主机</span>
            <input v-model="formHost" type="text" class="input" placeholder="192.168.1.1" required />
          </label>
          <label class="field">
            <span class="field-label">端口</span>
            <input v-model.number="formPort" type="number" class="input" min="1" max="65535" />
            <span class="field-hint">{{ typeHint(formType) }}</span>
          </label>
          <label class="field">
            <span class="field-label">用户名 (可选)</span>
            <input v-model="formUsername" type="text" class="input" placeholder="root" />
          </label>
          <label class="field">
            <span class="field-label">密码 (可选)</span>
            <input v-model="formPassword" type="password" class="input" placeholder="password" />
          </label>
          <label v-if="formType === 'JumpHost'" class="field">
            <span class="field-label">私钥路径 (可选)</span>
            <div class="key-row">
              <input v-model="formKeyPath" type="text" class="input key-input" readonly />
              <button type="button" class="btn btn-browse" @click="pickKey">选择</button>
            </div>
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

.proxy-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px;
  background: #181825;
  border-radius: 4px;
  font-size: 12px;
}

.proxy-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.proxy-type {
  color: #89b4fa;
  font-size: 11px;
}

.proxy-addr {
  color: #a6adc8;
  font-size: 11px;
}

.proxy-actions {
  display: flex;
  gap: 4px;
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
select.input {
  cursor: pointer;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
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
  border: 1px solid #45475a;
  background: #313244;
  color: #cdd6f4;
  font-size: 13px;
  cursor: pointer;
  white-space: nowrap;
}
.btn-browse:hover {
  background: #45475a;
}
</style>
