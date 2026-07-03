<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useTunnelStore } from '../../stores/tunnelStore'
import type { TunnelCreateRequest } from '../../types'

const emit = defineEmits<{
  close: []
}>()

const { tunnels, create, remove, refresh, tunnelStatusText } = useTunnelStore()

const showForm = ref(false)
const form = ref({
  host: '',
  port: 22,
  username: '',
  password: '',
  privateKeyPath: '',
  tunnelType: 'Local' as TunnelCreateRequest['tunnel_type'],
  bindAddr: '127.0.0.1',
  bindPort: 8080,
  targetHost: '',
  targetPort: 0,
})

async function pickKey() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'SSH Keys', extensions: ['pem', 'key', 'id_rsa', 'id_ed25519', '*'] }],
  })
  if (selected) form.value.privateKeyPath = selected
}

async function handleCreate() {
  const req: TunnelCreateRequest = {
    host: form.value.host,
    port: form.value.port,
    username: form.value.username,
    password: form.value.password || null,
    private_key_path: form.value.privateKeyPath || null,
    tunnel_type: form.value.tunnelType,
    bind_addr: form.value.bindAddr,
    bind_port: form.value.bindPort,
    target_host: form.value.tunnelType === 'Dynamic' ? null : form.value.targetHost,
    target_port: form.value.tunnelType === 'Dynamic' ? null : form.value.targetPort,
  }
  await create(req)
  showForm.value = false
  resetForm()
}

function resetForm() {
  form.value = {
    host: '',
    port: 22,
    username: '',
    password: '',
    privateKeyPath: '',
    tunnelType: 'Local',
    bindAddr: '127.0.0.1',
    bindPort: 8080,
    targetHost: '',
    targetPort: 0,
  }
}

async function handleRemove(id: string) {
  await remove(id)
}

function statusClass(status: unknown): string {
  if (status === 'Running') return 'running'
  if (status === 'Stopped') return 'stopped'
  return ''
}

onMounted(refresh)
</script>

<template>
  <div class="tunnel-panel">
    <div class="panel-header">
      <span class="panel-title">端口转发</span>
      <div class="panel-actions">
        <button class="btn btn-sm" @click="showForm = !showForm">
          {{ showForm ? '取消' : '新建' }}
        </button>
        <button class="btn btn-sm btn-close" @click="emit('close')">✕</button>
      </div>
    </div>

    <div v-if="showForm" class="tunnel-form">
      <div class="form-group">
        <label>SSH 服务器</label>
        <div class="form-row">
          <input v-model="form.host" placeholder="主机" class="input flex-1" />
          <input v-model.number="form.port" type="number" class="input w-20" />
        </div>
      </div>
      <div class="form-group">
        <label>认证</label>
        <input v-model="form.username" placeholder="用户名" class="input" />
        <input v-model="form.password" type="password" placeholder="密码（可选）" class="input" />
        <div class="form-row">
          <input v-model="form.privateKeyPath" placeholder="私钥路径（可选）" class="input flex-1" />
          <button class="btn btn-sm" @click="pickKey">浏览</button>
        </div>
      </div>
      <div class="form-group">
        <label>转发类型</label>
        <select v-model="form.tunnelType" class="input">
          <option value="Local">本地 (-L)</option>
          <option value="Remote">远程 (-R)</option>
          <option value="Dynamic">动态 (-D, SOCKS5)</option>
        </select>
      </div>
      <div class="form-group">
        <label>绑定地址</label>
        <div class="form-row">
          <input v-model="form.bindAddr" placeholder="127.0.0.1" class="input flex-1" />
          <input v-model.number="form.bindPort" type="number" placeholder="端口" class="input w-24" />
        </div>
      </div>
      <div v-if="form.tunnelType !== 'Dynamic'" class="form-group">
        <label>目标地址</label>
        <div class="form-row">
          <input v-model="form.targetHost" placeholder="目标主机" class="input flex-1" />
          <input v-model.number="form.targetPort" type="number" placeholder="端口" class="input w-24" />
        </div>
      </div>
      <button class="btn btn-primary btn-full" @click="handleCreate">创建转发</button>
    </div>

    <div class="tunnel-list">
      <div v-if="tunnels.length === 0" class="empty-hint">
        暂无端口转发
      </div>
      <div v-for="t in tunnels" :key="t.id" class="tunnel-item">
        <div class="tunnel-info">
          <span class="tunnel-type-badge" :class="t.tunnel_type.toLowerCase()">
            {{ t.tunnel_type === 'Local' ? 'L' : t.tunnel_type === 'Remote' ? 'R' : 'D' }}
          </span>
          <div class="tunnel-desc">
            <div class="tunnel-endpoint">
              <template v-if="t.tunnel_type === 'Local'">
                {{ t.bind_addr }}:{{ t.bind_port }} → {{ t.target_host }}:{{ t.target_port }}
              </template>
              <template v-else-if="t.tunnel_type === 'Remote'">
                {{ t.bind_addr }}:{{ t.bind_port }} → {{ t.target_host }}:{{ t.target_port }}
              </template>
              <template v-else>
                SOCKS5 {{ t.bind_addr }}:{{ t.bind_port }}
              </template>
            </div>
            <div class="tunnel-meta">{{ t.host }}:{{ t.port }}@{{ t.username }}</div>
          </div>
        </div>
        <div class="tunnel-status" :class="statusClass(t.status)">
          {{ tunnelStatusText(t.status) }}
        </div>
        <button class="btn btn-sm btn-danger" @click="handleRemove(t.id)">删除</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tunnel-panel {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--panel-bg, #1e1e2e);
  color: var(--fg, #cdd6f4);
  font-size: 13px;
  overflow: hidden;
}
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border, #313244);
}
.panel-title { font-weight: 600; }
.panel-actions { display: flex; gap: 6px; }
.btn {
  padding: 4px 10px;
  border: 1px solid var(--border, #313244);
  border-radius: 4px;
  background: var(--btn-bg, #313244);
  color: var(--fg, #cdd6f4);
  cursor: pointer;
  font-size: 12px;
}
.btn:hover { background: var(--btn-hover, #45475a); }
.btn-sm { padding: 2px 8px; font-size: 11px; }
.btn-close { border: none; background: transparent; font-size: 14px; }
.btn-close:hover { background: transparent; color: #f38ba8; }
.btn-primary {
  background: var(--accent, #89b4fa);
  color: #1e1e2e;
  border-color: var(--accent, #89b4fa);
}
.btn-primary:hover { opacity: 0.9; }
.btn-danger { border-color: #f38ba8; color: #f38ba8; }
.btn-danger:hover { background: #f38ba8; color: #1e1e2e; }
.btn-full { width: 100%; margin-top: 8px; }
.input {
  padding: 5px 8px;
  border: 1px solid var(--border, #313244);
  border-radius: 4px;
  background: var(--input-bg, #181825);
  color: var(--fg, #cdd6f4);
  font-size: 12px;
  outline: none;
}
.input:focus { border-color: var(--accent, #89b4fa); }
.w-20 { width: 70px; }
.w-24 { width: 80px; }
.flex-1 { flex: 1; }
.form-group { margin-bottom: 8px; }
.form-group label { display: block; font-size: 11px; color: var(--fg-dim, #a6adc8); margin-bottom: 3px; }
.form-row { display: flex; gap: 6px; }
.tunnel-form {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border, #313244);
  overflow-y: auto;
  max-height: 380px;
}
.tunnel-list { flex: 1; overflow-y: auto; }
.tunnel-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border, #313244);
}
.tunnel-item:hover { background: rgba(255,255,255,0.03); }
.tunnel-info {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}
.tunnel-type-badge {
  width: 22px;
  height: 22px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  flex-shrink: 0;
}
.tunnel-type-badge.local { background: #89b4fa; color: #1e1e2e; }
.tunnel-type-badge.remote { background: #a6e3a1; color: #1e1e2e; }
.tunnel-type-badge.dynamic { background: #f9e2af; color: #1e1e2e; }
.tunnel-desc { min-width: 0; }
.tunnel-endpoint {
  font-size: 12px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.tunnel-meta {
  font-size: 10px;
  color: var(--fg-dim, #6c7086);
}
.tunnel-status {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 3px;
  white-space: nowrap;
}
.tunnel-status.running { background: rgba(166,227,161,0.15); color: #a6e3a1; }
.tunnel-status.stopped { background: rgba(108,112,134,0.15); color: #6c7086; }
select.input {
  appearance: auto;
}
.empty-hint {
  text-align: center;
  padding: 32px;
  color: var(--fg-dim, #6c7086);
  font-size: 12px;
}
</style>
