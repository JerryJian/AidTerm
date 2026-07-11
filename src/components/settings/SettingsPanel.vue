<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { setLanguage } from '../../i18n'
import { useSettingsStore } from '../../stores/settingsStore'
import { useAiStore } from '../../stores/aiStore'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'

const { t, locale } = useI18n()
const settings = useSettingsStore()
const ai = useAiStore()

const emit = defineEmits<{
  close: []
}>()

const showAiKey = ref(false)
const aiKeyBuffer = ref(ai.config.api_key)

async function onLanguageChange(e: Event) {
  const lang = (e.target as HTMLSelectElement).value
  setLanguage(locale.value = lang as any)
}

function onTransparencyChange(e: Event) {
  settings.transparency = parseFloat((e.target as HTMLInputElement).value)
}

async function selectBackgroundImage() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif'] }],
  })
  if (selected) {
    settings.backgroundImage = selected
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
  <div class="settings-panel">
    <div class="panel-header">
      <span class="panel-title">{{ t('settings.title') }}</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>
    <div class="panel-body">
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
      </div>

      <div class="section">
        <h3>{{ t('settings.appearance') }}</h3>
        <div class="setting-row">
          <label>{{ t('settings.transparency') }}: {{ Math.round(settings.transparency * 100) }}%</label>
          <input
            type="range"
            min="0.3"
            max="1"
            step="0.05"
            :value="settings.transparency"
            @input="onTransparencyChange"
          />
        </div>
        <div class="setting-row">
          <label>{{ t('settings.background_image') }}</label>
          <div class="row-actions">
            <button class="action-btn" @click="selectBackgroundImage">{{ t('settings.select_image') }}</button>
            <button v-if="settings.backgroundImage" class="action-btn danger" @click="clearBackgroundImage">{{ t('settings.clear_image') }}</button>
          </div>
        </div>
      </div>

      <div class="section">
        <h3>{{ t('settings.tray') }}</h3>
        <div class="setting-row">
          <label>
            <input type="checkbox" v-model="settings.minimizeToTray" />
            {{ t('settings.minimize_to_tray') }}
          </label>
        </div>
        <div class="setting-row">
          <label>
            <input type="checkbox" v-model="settings.closeToTray" />
            {{ t('settings.close_to_tray') }}
          </label>
        </div>
      </div>

      <div class="section">
        <h3>🤖 {{ t('ai.title') }}</h3>

        <div class="setting-row col">
          <label>{{ t('ai.provider') }}</label>
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
        <h3>🌐 {{ t('ai.mode') }}</h3>
        <div class="setting-row col">
          <select
            :value="ai.config.mode || 'auto'"
            @change="(e: any) => ai.updateConfig({ mode: e.target.value })"
            class="text-input"
          >
            <option value="auto">{{ t('ai.mode_auto') }}</option>
            <option value="prefix">{{ t('ai.mode_prefix') }}</option>
            <option value="keybinding">{{ t('ai.mode_keybinding') }}</option>
          </select>
        </div>
        <div class="setting-row col" v-if="ai.config.mode === 'prefix'">
          <label>{{ t('ai.prefix') }}</label>
          <input
            :value="ai.config.prefix || ':'"
            @input="(e: any) => ai.updateConfig({ prefix: e.target.value })"
            class="text-input"
            placeholder=":"
          />
          <span class="field-desc">{{ t('ai.prefix_desc') }}</span>
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
            {{ ai.enabled ? t('ai.configured') : t('ai.not_configured') }}
          </span>
        </div>
        <div class="setting-row">
          <button class="action-btn" @click="ai.clearHistory()">{{ t('ai.clear_history') }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-panel {
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

.panel-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
}
.panel-close:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
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

.status-ok {
  color: var(--success);
  font-size: 12px;
}
.status-ko {
  color: var(--danger);
  font-size: 12px;
}

.model-select-row {
  display: flex;
  gap: 4px;
}
.model-select-row .model-select {
  flex: 1;
}
</style>
