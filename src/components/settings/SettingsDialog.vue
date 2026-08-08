<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { setLanguage } from '../../i18n'
import { useSettingsStore } from '../../stores/settingsStore'
import { useThemeStore } from '../../stores/themeStore'
import { useAiStore } from '../../stores/aiStore'
import { useProxyStore } from '../../stores/proxyStore'
import { getCurrentWindow, openDialog as open } from '@/api'
import type { ProxyConfig, ProxyType } from '../../types'

const { t, locale } = useI18n()
const settings = useSettingsStore()
const theme = useThemeStore()
const ai = useAiStore()
const proxyStore = useProxyStore()

const emit = defineEmits<{
  close: []
}>()

const escHandler = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
onMounted(() => document.addEventListener('keydown', escHandler))
onUnmounted(() => document.removeEventListener('keydown', escHandler))

onMounted(() => proxyStore.refresh())

const activeTab = ref<'general' | 'ai' | 'proxy'>('general')
const showProxyForm = ref(false)
const editProxyId = ref<string | null>(null)
const proxyForm = ref({
  name: '',
  proxy_type: 'Http' as ProxyType,
  host: '',
  port: 3128,
  username: '',
  password: '',
  private_key_path: '',
})

function resetProxyForm() {
  editProxyId.value = null
  proxyForm.value = { name: '', proxy_type: 'Http', host: '', port: 3128, username: '', password: '', private_key_path: '' }
  showProxyForm.value = true
}

function editProxy(p: ProxyConfig) {
  editProxyId.value = p.id
  proxyForm.value = { name: p.name, proxy_type: p.proxy_type, host: p.host, port: p.port, username: p.username ?? '', password: p.password ?? '', private_key_path: p.private_key_path ?? '' }
  showProxyForm.value = true
}

async function saveProxy() {
  const f = proxyForm.value
  if (!f.name || !f.host || !f.port) return
  const config: ProxyConfig = {
    id: editProxyId.value ?? crypto.randomUUID(),
    name: f.name,
    proxy_type: f.proxy_type,
    host: f.host,
    port: f.port,
    username: f.username || null,
    password: f.password || null,
    private_key_path: f.private_key_path || null,
  }
  await proxyStore.save(config)
  showProxyForm.value = false
}

async function deleteProxy(id: string) {
  await proxyStore.remove(id)
}

function proxyTypeLabel(t: ProxyType): string {
  if (t === 'Http') return 'HTTP CONNECT'
  if (t === 'Socks5') return 'SOCKS5'
  return 'Jump Host'
}
const showAiKey = ref(false)
const aiKeyBuffer = ref(ai.config.api_key)

async function onLanguageChange(e: Event) {
  const lang = (e.target as HTMLSelectElement).value
  setLanguage(locale.value = lang as any)
}

function onBackgroundOpacityChange(e: Event) {
  settings.backgroundOpacity = parseFloat((e.target as HTMLInputElement).value)
}

async function selectBackgroundImage() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif'] }],
  })
  if (selected) {
    settings.backgroundImage = Array.isArray(selected) ? selected[0] : selected
  }
}

function clearBackgroundImage() {
  settings.backgroundImage = ''
}

async function toggleFullscreen() {
  const win = getCurrentWindow()
  const isFull = await win.isFullscreen()
  await win.setFullscreen(!isFull)
}
</script>

<template>
  <div class="modal-overlay">
    <div class="modal-dialog">
      <div class="modal-header">
        <span class="modal-title">{{ t('settings.title') }}</span>
        <button class="modal-close" @click="emit('close')">✕</button>
      </div>
      <div class="settings-tabs">
        <button class="st-tab" :class="{ active: activeTab === 'general' }" @click="activeTab = 'general'">⚙️ {{ t('settings.general') }}</button>
        <button class="st-tab" :class="{ active: activeTab === 'ai' }" @click="activeTab = 'ai'">🤖 {{ t('ai.title') }}</button>
        <button class="st-tab" :class="{ active: activeTab === 'proxy' }" @click="activeTab = 'proxy'">🌐 {{ t('proxy.title') }}</button>
      </div>
      <div class="modal-body">
        <template v-if="activeTab === 'general'">
        <div class="section">
          <h3>{{ t('settings.general') }}</h3>
          <div class="setting-row">
            <label>{{ t('settings.language') }}</label>
            <select :value="locale" @change="onLanguageChange">
              <option value="zh-CN">中文</option>
              <option value="en-US">English</option>
            </select>
          </div>
          <div class="setting-row">
            <label>{{ t('settings.fullscreen') }}</label>
            <button class="action-btn" @click="toggleFullscreen">{{ t('settings.fullscreen') }} (F11)</button>
          </div>
          <div class="setting-row">
            <label class="toggle-label">
              <span>{{ t('settings.adb_auto_kill') }}</span>
              <input
                type="checkbox"
                :checked="settings.adbAutoKill"
                @change="(e: any) => settings.adbAutoKill = e.target.checked"
                class="toggle-input"
              />
              <span class="toggle-switch" />
            </label>
          </div>
          <span class="field-desc">{{ t('settings.adb_auto_kill_desc') }}</span>
        </div>

        <div class="section">
          <h3>{{ t('settings.appearance') }}</h3>
          <div class="setting-row">
            <label>{{ t('settings.theme') }}</label>
            <select :value="theme.mode" @change="theme.setMode(($event.target as HTMLSelectElement).value as any)">
              <option value="dark">{{ t('settings.dark') }}</option>
              <option value="light">{{ t('settings.light') }}</option>
            </select>
          </div>
          <div class="setting-row">
            <label>{{ t('settings.scrollback') }}</label>
            <select :value="settings.scrollback" @change="(e: any) => settings.scrollback = parseInt(e.target.value, 10)">
              <option :value="1000">1000</option>
              <option :value="5000">5000</option>
              <option :value="10000">10000</option>
              <option :value="50000">50000</option>
              <option :value="100000">100000</option>
              <option :value="500000">500000</option>
              <option :value="1000000">1000000</option>
            </select>
          </div>
          <div class="setting-row">
            <label>{{ t('settings.background_image') }}</label>
            <div class="row-actions">
              <button class="action-btn" @click="selectBackgroundImage">{{ t('settings.select_image') }}</button>
              <button v-if="settings.backgroundImage" class="action-btn danger" @click="clearBackgroundImage">{{ t('settings.clear_image') }}</button>
            </div>
          </div>
          <div v-if="settings.backgroundImage" class="setting-row">
            <label>{{ t('settings.background_opacity') }}: {{ Math.round(settings.backgroundOpacity * 100) }}%</label>
            <input
              type="range"
              min="0.1"
              max="1"
              step="0.05"
              :value="settings.backgroundOpacity"
              @input="onBackgroundOpacityChange"
            />
          </div>
        </div>
        </template>

        <template v-if="activeTab === 'ai'">
        <div class="section">
          <h3>🤖 {{ t('ai.title') }}</h3>

          <div class="setting-row col">
            <label>{{ t('settings.ai_provider') }}</label>
            <select
              :value="ai.currentProviderId"
              @change="(e: any) => ai.setProvider(e.target.value)"
              class="text-input"
            >
              <option v-for="p in ai.providerList" :key="p.id" :value="p.id">{{ p.label }}</option>
            </select>
          </div>

          <div class="setting-row col">
            <label>{{ t('ai.base_url') }}</label>
            <input
              :value="ai.config.base_url"
              @input="(e: any) => ai.updateConfig({ base_url: e.target.value })"
              class="text-input"
              placeholder="https://api.openai.com/v1"
            />
          </div>

          <div class="setting-row col">
            <label>{{ t('ai.api_key') }}</label>
            <div class="input-with-toggle">
              <input
                :type="showAiKey ? 'text' : 'password'"
                :value="aiKeyBuffer"
                @input="(e: any) => { aiKeyBuffer = e.target.value; ai.updateConfig({ api_key: e.target.value }) }"
                placeholder="sk-..."
                class="text-input"
              />
              <button class="toggle-btn" @click="showAiKey = !showAiKey">{{ showAiKey ? t('settings.ai_hide') : t('settings.ai_show') }}</button>
            </div>
          </div>

          <div class="setting-row col">
            <label>{{ t('ai.model') }}</label>
            <div class="model-select-row">
              <select
                :value="ai.config.model"
                @change="(e: any) => ai.updateConfig({ model: e.target.value })"
                class="text-input model-select"
              >
              <option v-if="ai.config.model && !ai.modelList.includes(ai.config.model)" :value="ai.config.model">{{ ai.config.model }}</option>
              <option v-for="m in ai.modelList" :key="m" :value="m">{{ m }}</option>
              </select>
              <button class="action-btn" @click="ai.fetchModels()" :disabled="ai.loadingModels">
                {{ ai.loadingModels ? '...' : '🔄' }}
              </button>
            </div>
          </div>

        </div>
        <div class="section">
          <h3>⚙️ {{ t('ai.auto_execute_section') }}</h3>
          <div class="setting-row">
            <label class="toggle-label">
              <span>{{ t('ai.auto_execute') }}</span>
              <input
                type="checkbox"
                :checked="ai.config.auto_execute || false"
                @change="(e: any) => ai.updateConfig({ auto_execute: e.target.checked })"
                class="toggle-input"
              />
              <span class="toggle-switch" />
            </label>
          </div>
          <span class="field-desc">{{ t('ai.auto_execute_desc') }}</span>
        </div>
        <div class="section">
          <h3>{{ t('ai.status') }}</h3>
          <div class="setting-row">
            <span :class="ai.enabled ? 'status-ok' : 'status-ko'">
              {{ ai.enabled ? t('settings.ai_configured') : t('settings.ai_not_configured') }}
            </span>
          </div>
          <div class="setting-row">
            <button class="action-btn" @click="ai.clearHistory()">{{ t('settings.ai_clear_history') }}</button>
          </div>
        </div>
        </template>

        <template v-if="activeTab === 'proxy'">
          <div class="section">
            <div class="section-header">
              <h3>🌐 {{ t('proxy.title') }}</h3>
              <button class="action-btn" @click="resetProxyForm">+ {{ t('proxy.add') }}</button>
            </div>
            <div v-for="p in proxyStore.proxies.value" :key="p.id" class="proxy-item">
              <div class="proxy-info">
                <strong>{{ p.name }}</strong>
                <span class="proxy-meta">{{ proxyTypeLabel(p.proxy_type) }} — {{ p.host }}:{{ p.port }}</span>
              </div>
              <div class="proxy-actions">
                <button class="action-btn" @click="editProxy(p)">✎</button>
                <button class="action-btn danger" @click="deleteProxy(p.id)">🗑</button>
              </div>
            </div>
            <div v-if="proxyStore.proxies.value.length === 0" class="empty-item">{{ t('proxy.no_proxies') }}</div>
          </div>

          <div v-if="showProxyForm" class="proxy-overlay">
            <div class="proxy-dialog" @click.stop>
              <div class="dialog-hdr">
                <span>{{ editProxyId ? t('common.edit') : t('proxy.add') }}</span>
                <button class="dialog-close" @click="showProxyForm = false">✕</button>
              </div>
              <div class="dialog-bd">
                <label class="fld-label">{{ t('proxy.name') }}</label>
                <input v-model="proxyForm.name" class="fld-input" />
                <label class="fld-label">{{ t('proxy.type') }}</label>
                <select v-model="proxyForm.proxy_type" class="fld-input">
                  <option value="Http">HTTP</option>
                  <option value="Socks5">SOCKS5</option>
                  <option value="JumpHost">Jump Host</option>
                </select>
                <label class="fld-label">{{ t('proxy.host') }}</label>
                <input v-model="proxyForm.host" class="fld-input" />
                <label class="fld-label">{{ t('proxy.port') }}</label>
                <input v-model.number="proxyForm.port" type="number" class="fld-input" min="1" max="65535" />
                <label class="fld-label">{{ t('proxy.username') }}</label>
                <input v-model="proxyForm.username" class="fld-input" />
                <label class="fld-label">{{ t('proxy.password') }}</label>
                <input v-model="proxyForm.password" type="password" class="fld-input" />
                <div class="dialog-actions">
                  <button class="action-btn" @click="showProxyForm = false">{{ t('common.cancel') }}</button>
                  <button class="btn-primary" @click="saveProxy">{{ t('common.save') }}</button>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
}

.modal-dialog {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  width: 560px;
  height: 540px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px var(--overlay);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  border-radius: 8px 8px 0 0;
}

.modal-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.modal-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
}
.modal-close:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.settings-tabs {
  display: flex;
  border-bottom: 1px solid var(--bg-surface0);
  background: var(--bg-mantle);
}
.st-tab {
  flex: 1;
  padding: 8px 12px;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  font-size: 12px;
  border-bottom: 2px solid transparent;
  transition: none;
}
.st-tab:hover {
  color: var(--text);
  background: var(--bg-base);
}
.st-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  background: var(--bg-base);
}

.modal-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.section {
  margin-bottom: 20px;
}

.section h3 {
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--bg-surface0);
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  gap: 8px;
}

.setting-row label {
  font-size: 13px;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 6px;
}

.setting-row select,
.setting-row input[type="range"] {
  background: var(--bg-surface0);
  color: var(--text);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 12px;
}

.setting-row input[type="range"] {
  padding: 0;
  width: 120px;
}

.setting-row input[type="checkbox"] {
  accent-color: var(--accent);
}

.row-actions {
  display: flex;
  gap: 4px;
}

.action-btn {
  padding: 4px 10px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.action-btn:hover {
  background: var(--bg-surface1);
}
.action-btn.danger:hover {
  background: var(--danger);
  color: var(--bg-base);
}

.setting-row.col {
  flex-direction: column;
  align-items: stretch;
}

.provider-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.provider-chip {
  padding: 4px 10px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  text-transform: capitalize;
}
.provider-chip:hover {
  background: var(--bg-surface1);
}
.provider-chip.active {
  border-color: var(--accent);
  background: var(--bg-mantle);
  color: var(--accent);
}

.preset-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  margin-top: 4px;
  padding-left: 8px;
  border-left: 2px solid var(--bg-surface1);
}

.preset-chip {
  padding: 2px 8px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-mantle);
  color: var(--text-sub0);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  text-transform: capitalize;
}
.preset-chip:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.text-input {
  width: 100%;
  padding: 6px 8px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  font-size: 12px;
  outline: none;
}
.text-input:focus {
  border-color: var(--accent);
}

.input-with-toggle {
  display: flex;
  gap: 4px;
}
.input-with-toggle .text-input {
  flex: 1;
}

.toggle-btn {
  padding: 4px 8px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text-sub0);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  white-space: nowrap;
}
.toggle-btn:hover {
  background: var(--bg-surface1);
}

.status-ok {
  color: var(--success);
  font-size: 12px;
}
.status-ko {
  color: var(--danger);
  font-size: 12px;
}
.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.section-header h3 { margin: 0; border: none; padding: 0; }
.proxy-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px;
  background: var(--bg-mantle);
  border-radius: 4px;
  margin-bottom: 4px;
}
.proxy-info { display: flex; flex-direction: column; gap: 2px; }
.proxy-info strong { font-size: 13px; color: var(--text); }
.proxy-meta { font-size: 11px; color: var(--text-sub0); }
.proxy-actions { display: flex; gap: 4px; flex-shrink: 0; }
.empty-item { padding: 16px; text-align: center; color: var(--text-overlay0); font-size: 12px; }
.proxy-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.5);
  display: flex; align-items: center; justify-content: center;
}
.proxy-dialog {
  background: var(--bg-base); border: 1px solid var(--bg-surface0); border-radius: 8px;
  width: 380px; max-height: 80vh;
  display: flex; flex-direction: column;
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}
.dialog-hdr {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 14px; border-bottom: 1px solid var(--bg-surface0);
  font-size: 13px; font-weight: 600;
}
.dialog-close {
  border: none; background: none; color: var(--text-sub0);
  cursor: pointer; padding: 2px 6px; border-radius: 4px; font-size: 14px;
}
.dialog-close:hover { background: var(--bg-surface1); color: var(--text); }
.dialog-bd {
  padding: 14px; display: flex; flex-direction: column; gap: 8px;
}
.fld-label { font-size: 11px; color: var(--text-sub0); }
.fld-input {
  padding: 6px 8px; background: var(--bg-mantle); border: 1px solid var(--bg-surface1);
  border-radius: 4px; color: var(--text); font-size: 12px; outline: none;
}
.fld-input:focus { border-color: var(--accent); }
.dialog-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.dialog-actions .btn-primary {
  padding: 6px 16px; border: none; border-radius: 4px; font-size: 13px;
  cursor: pointer; background: var(--accent); color: var(--bg-base); font-weight: 600;
}
.dialog-actions .btn-primary:hover { opacity: 0.85; }

.model-select-row {
  display: flex;
  gap: 4px;
}
.model-select-row .model-select {
  flex: 1;
}

.field-desc {
  font-size: 11px;
  color: var(--text-sub0);
}

.toggle-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  font-size: 13px;
  color: var(--text);
  width: 100%;
}

.toggle-input {
  display: none;
}

.toggle-switch {
  position: relative;
  width: 36px;
  height: 20px;
  background: var(--bg-surface1);
  border-radius: 10px;
  transition: background 0.2s;
  flex-shrink: 0;
}

.toggle-switch::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  background: var(--text);
  border-radius: 50%;
  transition: transform 0.2s;
}

.toggle-input:checked + .toggle-switch {
  background: var(--accent);
}

.toggle-input:checked + .toggle-switch::after {
  transform: translateX(16px);
  background: var(--bg-base);
}
</style>
