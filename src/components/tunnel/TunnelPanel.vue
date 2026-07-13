<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { openDialog as open } from '@/api'
import { useI18n } from 'vue-i18n'
import { useTunnelStore } from '../../stores/tunnelStore'
import type { TunnelCreateRequest, TerminalTab } from '../../types'

const { t } = useI18n()

const props = defineProps<{
  tabId: string
  tab: TerminalTab
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

const useCurrentSession = computed(() => props.tab?.sshInfo != null)

if (useCurrentSession.value) {
  const info = props.tab.sshInfo!
  form.value.host = info.host
  form.value.port = info.port
  form.value.username = info.username
  form.value.password = info.password || ''
  form.value.privateKeyPath = info.privateKeyPath || ''
}

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
  if (useCurrentSession.value && props.tab?.sshInfo) {
    const info = props.tab.sshInfo
    form.value = {
      host: info.host,
      port: info.port,
      username: info.username,
      password: info.password || '',
      privateKeyPath: info.privateKeyPath || '',
      tunnelType: 'Local',
      bindAddr: '127.0.0.1',
      bindPort: 8080,
      targetHost: '',
      targetPort: 0,
    }
  } else {
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
}

async function handleRemove(id: string) {
  await remove(id)
}

function statusClass(status: unknown): string {
  if (status === 'Running') return 'running'
  if (status === 'Stopped') return 'stopped'
  return ''
}

let _tick: ReturnType<typeof setInterval> | undefined
onMounted(() => {
  refresh()
  _tick = setInterval(refresh, 2000)
})
onUnmounted(() => { if (_tick) clearInterval(_tick) })
</script>

<template>
  <div class="tunnel-panel">
    <div class="toolbar">
      <span class="toolbar-title">{{ t('tunnel.title') }}</span>
      <button class="tb-btn" @click="showForm = true">
        + {{ t('tunnel.new_btn') }}
      </button>
    </div>

    <Teleport to="body">
      <div v-if="showForm" class="tunnel-overlay">
        <div class="tunnel-dialog">
          <div class="dialog-header">
            <span>{{ t('tunnel.create') }}</span>
            <button class="dialog-close" @click="showForm = false">✕</button>
          </div>
          <div class="dialog-body">
      <div v-if="useCurrentSession" class="session-hint">
        {{ t('tunnel.using_session', [form.username + '@' + form.host + ':' + form.port]) }}
      </div>
      <template v-if="!useCurrentSession">
        <div class="form-group">
          <label>{{ t('tunnel.ssh_server') }}</label>
          <div class="form-row">
            <input v-model="form.host" :placeholder="t('common.host')" class="input flex-1" />
            <input v-model.number="form.port" type="number" class="input w-20" />
          </div>
        </div>
        <div class="form-group">
          <label>{{ t('tunnel.authentication') }}</label>
          <input v-model="form.username" :placeholder="t('common.username')" class="input" />
          <input v-model="form.password" type="password" :placeholder="t('tunnel.password_optional')" class="input" />
          <div class="form-row">
            <input v-model="form.privateKeyPath" :placeholder="t('tunnel.private_key_path')" class="input flex-1" />
            <button class="btn btn-sm" @click="pickKey">{{ t('tunnel.browse') }}</button>
          </div>
        </div>
      </template>
      <div class="form-group">
        <label>{{ t('tunnel.forward_type') }}</label>
        <select v-model="form.tunnelType" class="input">
          <option value="Local">{{ t('tunnel.local_label') }}</option>
          <option value="Remote">{{ t('tunnel.remote_label') }}</option>
          <option value="Dynamic">{{ t('tunnel.dynamic_label') }}</option>
        </select>
      </div>
      <div class="form-group">
        <label>{{ t('tunnel.bind_address') }}</label>
        <div class="form-row">
          <input v-model="form.bindAddr" placeholder="127.0.0.1" class="input flex-1" />
          <input v-model.number="form.bindPort" type="number" :placeholder="t('common.port')" class="input w-24" />
        </div>
      </div>
      <div v-if="form.tunnelType !== 'Dynamic'" class="form-group">
        <label>{{ t('tunnel.target') }}</label>
        <div class="form-row">
          <input v-model="form.targetHost" :placeholder="t('tunnel.dst_host')" class="input flex-1" />
          <input v-model.number="form.targetPort" type="number" :placeholder="t('common.port')" class="input w-24" />
        </div>
      </div>
      <button class="btn btn-primary btn-full" @click="handleCreate">{{ t('tunnel.create_forward') }}</button>
        </div>
      </div>
    </div>
  </Teleport>

    <div class="tunnel-list">
      <div v-if="tunnels.length === 0" class="empty-hint">
        {{ t('tunnel.no_tunnels') }}
      </div>
      <div v-for="tn in tunnels" :key="tn.id" class="tunnel-item">
        <div class="tunnel-info">
          <span class="tunnel-type-badge" :class="tn.tunnel_type.toLowerCase()">
            {{ tn.tunnel_type === 'Local' ? 'L' : tn.tunnel_type === 'Remote' ? 'R' : 'D' }}
          </span>
          <div class="tunnel-desc">
            <div class="tunnel-endpoint">
              <template v-if="tn.tunnel_type === 'Local'">
                {{ tn.bind_addr }}:{{ tn.bind_port }} → {{ tn.target_host }}:{{ tn.target_port }}
              </template>
              <template v-else-if="tn.tunnel_type === 'Remote'">
                {{ tn.bind_addr }}:{{ tn.bind_port }} → {{ tn.target_host }}:{{ tn.target_port }}
              </template>
              <template v-else>
                SOCKS5 {{ tn.bind_addr }}:{{ tn.bind_port }}
              </template>
            </div>
            <div class="tunnel-meta">{{ tn.host }}:{{ tn.port }}@{{ tn.username }}</div>
          </div>
        </div>
        <div class="tunnel-status" :class="statusClass(tn.status)">
          {{ tunnelStatusText(tn.status) }}
        </div>
        <button class="btn btn-sm btn-danger" @click="handleRemove(tn.id)">{{ t('common.delete') }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tunnel-panel {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--panel-bg, var(--bg-base));
  color: var(--fg, var(--text));
  font-size: 13px;
  overflow: hidden;
}
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
  background: var(--bg-mantle);
}
.toolbar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.tb-btn {
  padding: 3px 10px;
  border: 1px solid var(--border, var(--bg-surface0));
  border-radius: 4px;
  background: var(--bg-surface0);
  color: var(--text);
  cursor: pointer;
  font-size: 11px;
}
.tb-btn:hover { background: var(--bg-surface1); }
.session-hint {
  font-size: 11px;
  color: var(--text-sub0);
  padding: 6px 8px;
  background: var(--accent-glass);
  border-radius: 4px;
  margin-bottom: 8px;
}
.btn {
  padding: 4px 10px;
  border: 1px solid var(--border, var(--bg-surface0));
  border-radius: 4px;
  background: var(--btn-bg, var(--bg-surface0));
  color: var(--fg, var(--text));
  cursor: pointer;
  font-size: 12px;
}
.btn:hover { background: var(--btn-hover, var(--bg-surface1)); }
.btn-sm { padding: 2px 8px; font-size: 11px; }
.btn-primary {
  background: var(--accent, var(--accent));
  color: var(--bg-base);
  border-color: var(--accent, var(--accent));
}
.btn-primary:hover { opacity: 0.9; }
.btn-danger { border-color: var(--danger); color: var(--danger); }
.btn-danger:hover { background: var(--danger); color: var(--bg-base); }
.btn-full { width: 100%; margin-top: 8px; }
.input {
  padding: 5px 8px;
  border: 1px solid var(--border, var(--bg-surface0));
  border-radius: 4px;
  background: var(--input-bg, var(--bg-mantle));
  color: var(--fg, var(--text));
  font-size: 12px;
  outline: none;
}
.input:focus { border-color: var(--accent, var(--accent)); }
.w-20 { width: 70px; }
.w-24 { width: 80px; }
.flex-1 { flex: 1; }
.form-group { margin-bottom: 8px; }
.form-group label { display: block; font-size: 11px; color: var(--fg-dim, var(--text-sub0)); margin-bottom: 3px; }
.form-row { display: flex; gap: 6px; }
.tunnel-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
}
.tunnel-dialog {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  width: 420px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px var(--overlay);
}
.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--bg-surface0);
  font-size: 13px;
  font-weight: 600;
}
.dialog-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 14px;
}
.dialog-close:hover { background: var(--bg-surface1); color: var(--text); }
.dialog-body {
  padding: 14px;
  overflow-y: auto;
}
.tunnel-list { flex: 1; overflow-y: auto; }
.tunnel-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
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
.tunnel-type-badge.local { background: var(--accent); color: var(--bg-base); }
.tunnel-type-badge.remote { background: var(--success); color: var(--bg-base); }
.tunnel-type-badge.dynamic { background: var(--warning); color: var(--bg-base); }
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
  color: var(--fg-dim, var(--text-overlay1));
}
.tunnel-status {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 3px;
  white-space: nowrap;
}
.tunnel-status.running { background: rgba(166,227,161,0.15); color: var(--success); }
.tunnel-status.stopped { background: rgba(108,112,134,0.15); color: var(--text-overlay1); }
select.input {
  appearance: auto;
}
.empty-hint {
  text-align: center;
  padding: 32px;
  color: var(--fg-dim, var(--text-overlay1));
  font-size: 12px;
}
</style>