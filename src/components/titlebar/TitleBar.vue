<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'

const { t } = useI18n()

const win = getCurrentWindow()
const isMaximized = ref(false)
let lastClickTime = 0

onMounted(async () => {
  try {
    isMaximized.value = await win.isMaximized()
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
  if ((e.target as HTMLElement).closest('.titlebar-actions')) return

  const now = Date.now()
  if (now - lastClickTime < 300) {
    lastClickTime = 0
    onMaximize()
    return
  }
  lastClickTime = now
  win.startDragging()
}
</script>

<template>
  <div class="titlebar" @pointerdown="onPointerDown">
    <div class="titlebar-left">
      <svg class="titlebar-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="4 17 10 11 4 5" />
        <line x1="12" y1="19" x2="20" y2="19" />
      </svg>
      <span class="titlebar-title">TndTerm</span>
    </div>
    <div class="titlebar-center" />
    <div class="titlebar-actions">
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
    </div>
  </div>
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
</style>
