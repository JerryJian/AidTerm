<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { setLanguage } from '../../i18n'
import { useSettingsStore } from '../../stores/settingsStore'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'

const { t, locale } = useI18n()
const settings = useSettingsStore()

const emit = defineEmits<{
  close: []
}>()

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
    </div>
  </div>
</template>

<style scoped>
.settings-panel {
  width: 320px;
  min-width: 320px;
  background: #1e1e2e;
  border-left: 1px solid #313244;
  display: flex;
  flex-direction: column;
  height: 100%;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: #181825;
  border-bottom: 1px solid #313244;
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  color: #cdd6f4;
}

.panel-close {
  border: none;
  background: none;
  color: #a6adc8;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
}
.panel-close:hover {
  background: #45475a;
  color: #cdd6f4;
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
</style>
