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
  <div class="modal-overlay" @click.self="emit('close')">
    <div class="modal-dialog">
      <div class="modal-header">
        <span class="modal-title">{{ t('settings.title') }}</span>
        <button class="modal-close" @click="emit('close')">✕</button>
      </div>
      <div class="modal-body">
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
          <h3>🤖 AI</h3>
          <div class="setting-row">
            <label>提供商</label>
            <div class="provider-group">
              <button
                v-for="(_, name) in ai.defaultProviders"
                :key="name"
                class="provider-chip"
                :class="{ active: ai.config.provider === name }"
                @click="ai.setProvider(name)"
              >{{ name }}</button>
            </div>
          </div>
          <div class="setting-row col">
            <label>API Key</label>
            <div class="input-with-toggle">
              <input
                :type="showAiKey ? 'text' : 'password'"
                :value="aiKeyBuffer"
                @input="(e: any) => { aiKeyBuffer = e.target.value; ai.updateConfig({ api_key: e.target.value }) }"
                placeholder="sk-..."
                class="text-input"
              />
              <button class="toggle-btn" @click="showAiKey = !showAiKey">{{ showAiKey ? '隐藏' : '显示' }}</button>
            </div>
          </div>
          <div class="setting-row col">
            <label>Model</label>
            <input
              :value="ai.config.model"
              @input="(e: any) => ai.updateConfig({ model: e.target.value })"
              class="text-input"
            />
          </div>
          <div class="setting-row col">
            <label>Base URL</label>
            <input
              :value="ai.config.base_url"
              @input="(e: any) => ai.updateConfig({ base_url: e.target.value })"
              class="text-input"
            />
          </div>
          <div class="setting-row">
            <label>状态</label>
            <span :class="ai.enabled ? 'status-ok' : 'status-ko'">
              {{ ai.enabled ? '✅ 已配置' : '❌ 未配置' }}
            </span>
          </div>
          <div class="setting-row">
            <button class="action-btn" @click="ai.clearHistory()">清除对话历史</button>
          </div>
        </div>
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
  background: rgba(0, 0, 0, 0.5);
}

.modal-dialog {
  background: #1e1e2e;
  border: 1px solid #313244;
  border-radius: 8px;
  width: 480px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: #181825;
  border-bottom: 1px solid #313244;
  border-radius: 8px 8px 0 0;
}

.modal-title {
  font-size: 14px;
  font-weight: 600;
  color: #cdd6f4;
}

.modal-close {
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
}
.modal-close:hover {
  background: #45475a;
  color: #cdd6f4;
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
  color: #89b4fa;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid #313244;
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
  color: #cdd6f4;
  display: flex;
  align-items: center;
  gap: 6px;
}

.setting-row select,
.setting-row input[type="range"] {
  background: #313244;
  color: #cdd6f4;
  border: 1px solid #45475a;
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 12px;
}

.setting-row input[type="range"] {
  padding: 0;
  width: 120px;
}

.setting-row input[type="checkbox"] {
  accent-color: #89b4fa;
}

.row-actions {
  display: flex;
  gap: 4px;
}

.action-btn {
  padding: 4px 10px;
  border: 1px solid #45475a;
  background: #313244;
  color: #cdd6f4;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.action-btn:hover {
  background: #45475a;
}
.action-btn.danger:hover {
  background: #f38ba8;
  color: #1e1e2e;
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
  border: 1px solid #45475a;
  background: #313244;
  color: #cdd6f4;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  text-transform: capitalize;
}
.provider-chip:hover {
  background: #45475a;
}
.provider-chip.active {
  border-color: #89b4fa;
  background: #181825;
  color: #89b4fa;
}

.text-input {
  width: 100%;
  padding: 6px 8px;
  background: #181825;
  border: 1px solid #45475a;
  border-radius: 4px;
  color: #cdd6f4;
  font-size: 12px;
  outline: none;
}
.text-input:focus {
  border-color: #89b4fa;
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
  border: 1px solid #45475a;
  background: #313244;
  color: #a6adc8;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  white-space: nowrap;
}
.toggle-btn:hover {
  background: #45475a;
}

.status-ok {
  color: #a6e3a1;
  font-size: 12px;
}
.status-ko {
  color: #f38ba8;
  font-size: 12px;
}
</style>
