<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke, getCurrentWindow } from '@/api'
import { useI18n } from 'vue-i18n'
import { useUiStore } from '../../stores/uiStore'

const { t } = useI18n()
const ui = useUiStore()

const emit = defineEmits<{
  lock: []
}>()

const win = getCurrentWindow()
const isMaximized = ref(false)
const isWindows = ref(false)
const isMacOS = ref(false)
let lastClickTime = 0

const ctxMenu = ref<{ x: number; y: number } | null>(null)

onMounted(async () => {
  try {
    isMaximized.value = await win.isMaximized()
    const platform = await invoke<string>('get_platform')
    isWindows.value = platform === 'windows'
    isMacOS.value = platform === 'darwin'
  } catch { /* ignore */ }
})

async function onMinimize() {
  await win.minimize()
}

async function onMaximize() {
  await win.toggleMaximize()
  isMaximized.value = await win.isMaximized()
}

async function onClose() {
  await win.close()
}

function onPointerDown(e: PointerEvent) {
  if (e.button !== 0) return
  if ((e.target as HTMLElement).closest('.titlebar-actions, .traffic-lights')) return

  const now = Date.now()
  if (now - lastClickTime < 300) {
    lastClickTime = 0
    onMaximize()
    return
  }
  lastClickTime = now
  win.startDragging()
}

function onCtxMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  ctxMenu.value = { x: e.clientX, y: e.clientY }
}

function closeCtxMenu() {
  ctxMenu.value = null
}

async function onRestore() {
  ctxMenu.value = null
  try {
    if (isMaximized.value) await win.toggleMaximize()
  } catch { /* ignore */ }
}

async function onInspect() {
  ctxMenu.value = null
  try {
    await invoke('open_devtools')
  } catch { /* ignore */ }
}
</script>

<template>
  <div class="titlebar" @pointerdown="onPointerDown" @contextmenu.prevent="onCtxMenu">
    <!-- macOS: traffic lights on the left -->
    <div v-if="isMacOS" class="traffic-lights">
      <button class="tl-btn tl-close" @click="onClose" :title="t('titlebar.close')">
        <svg viewBox="0 0 12 12" width="12" height="12" class="tl-icon">
          <line x1="3" y1="3" x2="9" y2="9" stroke="#4D0000" stroke-width="1.2" />
          <line x1="9" y1="3" x2="3" y2="9" stroke="#4D0000" stroke-width="1.2" />
        </svg>
      </button>
      <button class="tl-btn tl-minimize" @click="onMinimize" :title="t('titlebar.minimize')">
        <svg viewBox="0 0 12 12" width="12" height="12" class="tl-icon">
          <line x1="2" y1="6" x2="10" y2="6" stroke="#995700" stroke-width="1.2" />
        </svg>
      </button>
      <button class="tl-btn tl-maximize" @click="onMaximize" :title="isMaximized ? t('titlebar.restore') : t('titlebar.maximize')">
        <svg viewBox="0 0 12 12" width="12" height="12" class="tl-icon">
          <path v-if="!isMaximized" d="M3 3h6v6H3z" fill="none" stroke="#006500" stroke-width="1.2" />
          <template v-else>
            <path d="M4 4h6v6H4z" fill="none" stroke="#006500" stroke-width="1.2" />
            <path d="M2 2h6v6H2z" fill="var(--bg-mantle)" stroke="#006500" stroke-width="1.2" />
          </template>
        </svg>
      </button>
    </div>

    <div class="titlebar-left">
      <!-- Windows: logo + title -->
      <template v-if="isWindows">
        <svg class="titlebar-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="4 17 10 11 4 5" />
          <line x1="12" y1="19" x2="20" y2="19" />
        </svg>
      </template>
      <template v-if="isMacOS">
        <svg class="titlebar-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="4 17 10 11 4 5" />
          <line x1="12" y1="19" x2="20" y2="19" />
        </svg>
      </template>
      <span class="titlebar-title">AidTerm</span>
    </div>
    <div class="titlebar-center" />

    <div class="titlebar-actions">
      <button class="tb-btn" @click="ui.settingsDialog = true" :title="t('menu.settings')">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
          <path d="M17.5 2.47363L23 11.9999L17.5 21.5262H6.5L1 11.9999L6.5 2.47363H17.5ZM16.3453 4.47363H7.6547L3.3094 11.9999L7.6547 19.5262H16.3453L20.6906 11.9999L16.3453 4.47363ZM8.63398 8.16979L10.366 7.16979L15.366 15.83L13.634 16.83L8.63398 8.16979Z"/>
        </svg>
      </button>
      <button class="tb-btn" @click="emit('lock')" :title="t('menu.lock')">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
          <path d="M19 10H20C20.5523 10 21 10.4477 21 11V21C21 21.5523 20.5523 22 20 22H4C3.44772 22 3 21.5523 3 21V11C3 10.4477 3.44772 10 4 10H5V9C5 5.13401 8.13401 2 12 2C15.866 2 19 5.13401 19 9V10ZM5 12V20H19V12H5ZM11 14H13V18H11V14ZM17 10V9C17 6.23858 14.7614 4 12 4C9.23858 4 7 6.23858 7 9V10H17Z"/>
        </svg>
      </button>
      <button class="tb-btn" @click="ui.aboutDialog = true" :title="t('about.title')">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
          <path d="M12 22C6.477 22 2 17.523 2 12S6.477 2 12 2s10 4.477 10 10-4.477 10-10 10zm0-2a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM11 7h2v2h-2V7zm0 4h2v6h-2v-6z"/>
        </svg>
      </button>
      <!-- Windows: show minimize/maximize/close buttons on the right -->
      <template v-if="isWindows">
        <div class="titlebar-sep" />
        <button class="tb-btn minimize" @click="onMinimize" :title="t('titlebar.minimize')">
          <svg viewBox="0 0 12 12" width="12" height="12">
            <rect x="2" y="5.5" width="8" height="1" fill="currentColor" />
          </svg>
        </button>
        <button class="tb-btn maximize" @click="onMaximize" :title="isMaximized ? t('titlebar.restore') : t('titlebar.maximize')">
          <svg v-if="!isMaximized" viewBox="0 0 12 12" width="12" height="12">
            <rect x="2" y="2" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
          </svg>
          <svg v-else viewBox="0 0 12 12" width="12" height="12">
            <rect x="3" y="0.5" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
            <rect x="0.5" y="3" width="8" height="8" rx="1" fill="var(--bg-mantle)" stroke="currentColor" stroke-width="1" />
          </svg>
        </button>
        <button class="tb-btn close" @click="onClose" :title="t('titlebar.close')">
          <svg viewBox="0 0 12 12" width="12" height="12">
            <line x1="2" y1="2" x2="10" y2="10" stroke="currentColor" stroke-width="1.5" />
            <line x1="10" y1="2" x2="2" y2="10" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </template>
    </div>
  </div>
  <div v-if="ctxMenu" class="ctx-backdrop" @click="closeCtxMenu" @contextmenu.prevent="closeCtxMenu" />
  <Teleport to="body">
    <div v-if="ctxMenu" class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }">
      <template v-if="isWindows">
        <button class="ctx-item" :disabled="!isMaximized" @click="onRestore()">{{ t('titlebar.restore') }}</button>
        <button class="ctx-item" @click="onMinimize(); ctxMenu = null">{{ t('titlebar.minimize') }}</button>
        <button class="ctx-item" @click="onMaximize(); ctxMenu = null">{{ isMaximized ? t('titlebar.restore') : t('titlebar.maximize') }}</button>
        <div class="ctx-divider" />
        <button class="ctx-item" @click="onClose(); ctxMenu = null">{{ t('titlebar.close') }}</button>
        <div class="ctx-divider" />
      </template>
      <button class="ctx-item" @click="onInspect">{{ t('titlebar.inspect') }}</button>
      <div class="ctx-divider" />
      <button class="ctx-item" @click="ctxMenu = null; ui.aboutDialog = true">{{ t('about.title') }}</button>
    </div>
  </Teleport>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  height: 32px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  user-select: none;
  flex-shrink: 0;
  -webkit-app-region: drag;
}

/* macOS traffic lights */
.traffic-lights {
  display: flex;
  align-items: center;
  padding-left: 12px;
  gap: 8px;
  height: 100%;
  flex-shrink: 0;
  -webkit-app-region: no-drag;
}

.tl-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: none;
  cursor: pointer;
  padding: 0;
  transition: filter 0.1s;
}

.tl-close {
  background: #FF5F57;
}

.tl-minimize {
  background: #FEBC2E;
}

.tl-maximize {
  background: #28C840;
}

.tl-icon {
  opacity: 0;
  transition: opacity 0.1s;
  width: 8px;
  height: 8px;
}

.traffic-lights:hover .tl-icon {
  opacity: 1;
}

.tl-close:hover {
  background: #FF4040;
}

.tl-minimize:hover {
  background: #F5A623;
}

.tl-maximize:hover {
  background: #1EAD2D;
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  height: 100%;
}

.titlebar-icon {
  color: var(--accent);
  flex-shrink: 0;
}

.titlebar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-sub0);
  letter-spacing: 0.3px;
}

.titlebar-center {
  flex: 1;
  height: 100%;
}

.titlebar-actions {
  display: flex;
  height: 100%;
  -webkit-app-region: no-drag;
}

.tb-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--text-sub0);
  cursor: pointer;
  transition: background 0.1s;
}

.tb-btn:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.tb-btn.close:hover {
  background: var(--danger);
  color: var(--bg-base);
}

.titlebar-sep {
  width: 1px;
  height: 16px;
  background: var(--bg-surface0);
  margin: auto 2px;
}
</style>

<style>
.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  background: transparent;
}
.ctx-menu {
  position: fixed;
  z-index: 10000;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  min-width: 160px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}
.ctx-menu .ctx-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  border: none;
  background: none;
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
  border-radius: 4px;
  white-space: nowrap;
}
.ctx-menu .ctx-item:hover {
  background: var(--bg-surface0);
  color: var(--accent);
}
.ctx-menu .ctx-item:disabled {
  opacity: 0.4;
  cursor: default;
}
.ctx-menu .ctx-item:disabled:hover {
  background: none;
  color: var(--text);
}
.ctx-menu .ctx-divider {
  height: 1px;
  background: var(--bg-surface0);
  margin: 4px 8px;
}
</style>
