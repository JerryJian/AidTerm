<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { downloadUpdate, installUpdate, listen } from '@/api'
import type { UpdateInfo } from '@/types'
import { marked } from 'marked'

const props = defineProps<{
  updateInfo: UpdateInfo
}>()

const emit = defineEmits<{
  close: []
}>()

const { t } = useI18n()

const downloading = ref(false)
const downloadProgress = ref(0)
const downloadedPath = ref('')
const installError = ref('')

let stopProgressListener: (() => void) | null = null

const bodyHtml = computed(() => {
  if (!props.updateInfo.body) return ''
  try {
    return marked.parse(props.updateInfo.body, { async: false }) as string
  } catch {
    return props.updateInfo.body
  }
})

function installerLabel(type: string): string {
  if (type === 'msi') return 'MSI'
  if (type === 'nsis') return 'EXE (NSIS)'
  return type || 'unknown'
}

async function doDownload() {
  if (!props.updateInfo.asset_url) return
  downloading.value = true
  downloadProgress.value = 0
  installError.value = ''
  if (!stopProgressListener) {
    stopProgressListener = await listen<{ received: number; total: number }>('update-progress', (ev) => {
      const { received, total } = ev.payload
      downloadProgress.value = total > 0 ? Math.round((received / total) * 100) : 0
    })
  }
  try {
    downloadedPath.value = await downloadUpdate(props.updateInfo.asset_url)
  } catch (e: any) {
    installError.value = e?.message || String(e)
    downloadedPath.value = ''
  } finally {
    downloading.value = false
  }
}

async function doInstall() {
  if (!downloadedPath.value) return
  installError.value = ''
  try {
    await installUpdate(downloadedPath.value)
  } catch (e: any) {
    installError.value = e?.message || String(e)
  }
}

const escHandler = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }

onMounted(() => document.addEventListener('keydown', escHandler))
onUnmounted(() => {
  document.removeEventListener('keydown', escHandler)
  stopProgressListener?.()
})
</script>

<template>
  <div class="ud-overlay" @click.self="emit('close')">
    <div class="ud-dialog">
      <div class="ud-header">
        <span class="ud-title">
          {{ t('about.update_available', { version: updateInfo.latest_version }) }}
        </span>
        <button class="ud-close" @click="emit('close')">&#x2715;</button>
      </div>

      <div class="ud-body">
        <div class="ud-meta">
          <div class="ud-versions">
            <span class="ud-version current">v{{ updateInfo.current_version }}</span>
            <span class="ud-arrow">&#8594;</span>
            <span class="ud-version latest">v{{ updateInfo.latest_version }}</span>
          </div>
          <div class="ud-meta-row">
            <span v-if="updateInfo.published_at">
              {{ t('about.published') }}: {{ new Date(updateInfo.published_at).toLocaleDateString() }}
            </span>
            <span>{{ t('about.installer') }}: {{ installerLabel(updateInfo.installer_type) }}</span>
          </div>
        </div>

        <div class="ud-changelog" v-html="bodyHtml" />

        <div v-if="downloading" class="ud-progress">
          <div class="progress-bar">
            <div class="progress-fill" :style="{ width: downloadProgress + '%' }" />
          </div>
          <span class="progress-text">{{ downloadProgress }}%</span>
        </div>

        <div class="ud-actions">
          <button v-if="!downloadedPath" class="ud-btn primary" :disabled="downloading || !updateInfo.asset_url" @click="doDownload">
            {{ downloading ? t('about.downloading') + '...' : t('about.download_install') }}
          </button>
          <button v-else class="ud-btn primary" @click="doInstall">
            {{ t('about.install_restart') }}
          </button>
          <a v-if="updateInfo.release_url" class="ud-btn" :href="updateInfo.release_url" target="_blank" rel="noopener">
            {{ t('about.view_release') }}
          </a>
        </div>

        <div v-if="installError" class="ud-status ud-error">{{ installError }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ud-overlay {
  position: fixed;
  inset: 0;
  z-index: 1100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
}

.ud-dialog {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  width: 560px;
  max-width: 92vw;
  max-height: 84vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px var(--overlay);
}

.ud-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  border-radius: 8px 8px 0 0;
  flex-shrink: 0;
}

.ud-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.ud-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 14px;
}

.ud-close:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.ud-body {
  padding: 20px 20px 24px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
}

.ud-meta {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.ud-versions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.ud-version {
  padding: 3px 12px;
  border-radius: 12px;
  font-size: 14px;
  font-weight: 600;
}

.ud-version.current {
  color: var(--text-sub0);
  background: var(--bg-surface0);
}

.ud-version.latest {
  color: var(--bg-base);
  background: var(--accent);
}

.ud-arrow {
  color: var(--text-overlay0);
  font-size: 14px;
}

.ud-meta-row {
  display: flex;
  gap: 16px;
  font-size: 12px;
  color: var(--text-sub0);
}

.ud-changelog {
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  padding: 12px 14px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text);
  max-height: 320px;
  overflow-y: auto;
  text-align: left;
  word-break: break-word;
}

.ud-changelog :deep(h1),
.ud-changelog :deep(h2),
.ud-changelog :deep(h3) {
  font-size: 14px;
  margin: 12px 0 6px;
  color: var(--text);
}

.ud-changelog :deep(h1:first-child),
.ud-changelog :deep(h2:first-child),
.ud-changelog :deep(h3:first-child) {
  margin-top: 0;
}

.ud-changelog :deep(p) {
  margin: 6px 0;
}

.ud-changelog :deep(ul),
.ud-changelog :deep(ol) {
  margin: 6px 0;
  padding-left: 20px;
}

.ud-changelog :deep(li) {
  margin: 3px 0;
}

.ud-changelog :deep(a) {
  color: var(--accent);
  text-decoration: none;
}

.ud-changelog :deep(blockquote) {
  margin: 8px 0;
  padding-left: 10px;
  border-left: 3px solid var(--bg-surface1);
  color: var(--text-sub0);
}

.ud-changelog :deep(code) {
  background: var(--bg-surface0);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 11px;
  font-family: Consolas, 'Courier New', monospace;
}

.ud-changelog :deep(pre) {
  background: var(--bg-surface0);
  padding: 10px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 8px 0;
}

.ud-changelog :deep(pre code) {
  background: none;
  padding: 0;
}

.ud-changelog :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 8px 0;
  font-size: 11px;
}

.ud-changelog :deep(th),
.ud-changelog :deep(td) {
  border: 1px solid var(--bg-surface1);
  padding: 4px 8px;
  text-align: left;
}

.ud-changelog :deep(th) {
  background: var(--bg-surface0);
}

.ud-changelog :deep(hr) {
  border: none;
  border-top: 1px solid var(--bg-surface1);
  margin: 10px 0;
}

.ud-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  flex-shrink: 0;
}

.progress-bar {
  flex: 1;
  height: 8px;
  background: var(--bg-surface0);
  border-radius: 4px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 4px;
  transition: width 0.1s;
}

.progress-text {
  font-size: 11px;
  color: var(--text-sub0);
  min-width: 34px;
  text-align: right;
}

.ud-actions {
  display: flex;
  justify-content: center;
  gap: 10px;
  flex-shrink: 0;
}

.ud-btn {
  padding: 7px 18px;
  border: 1px solid var(--bg-surface0);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 6px;
  cursor: pointer;
  font-size: 12px;
  text-decoration: none;
  transition: background 0.15s, border-color 0.15s;
}

.ud-btn:hover:not(:disabled) {
  background: var(--bg-surface1);
  border-color: var(--bg-surface1);
}

.ud-btn.primary {
  background: var(--accent);
  color: var(--bg-base);
  border-color: var(--accent);
  font-weight: 600;
}

.ud-btn.primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.ud-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ud-status {
  font-size: 12px;
  text-align: center;
  line-height: 1.5;
}

.ud-error {
  color: var(--danger);
  word-break: break-word;
}
</style>
