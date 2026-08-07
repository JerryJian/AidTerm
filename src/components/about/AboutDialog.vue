<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { checkForUpdate, downloadUpdate, installUpdate, getAppVersion, listen, isElectron } from '@/api'
import type { UpdateInfo } from '@/types'

const { t } = useI18n()

const emit = defineEmits<{
  close: []
}>()

const version = ref('')
const runtime = ref<'tauri' | 'electron'>('tauri')

const checking = ref(false)
const checkError = ref('')
const updateInfo = ref<UpdateInfo | null>(null)
const downloading = ref(false)
const downloadProgress = ref(0)
const downloadedPath = ref('')
const installError = ref('')

let stopProgressListener: (() => void) | null = null

async function checkUpdate() {
  checking.value = true
  checkError.value = ''
  try {
    updateInfo.value = await checkForUpdate()
  } catch (e: any) {
    checkError.value = e?.message || String(e)
    updateInfo.value = null
  } finally {
    checking.value = false
  }
}

function installerLabel(type: string): string {
  if (type === 'msi') return 'MSI'
  if (type === 'nsis') return 'EXE (NSIS)'
  return type || 'unknown'
}

async function doDownload() {
  if (!updateInfo.value?.asset_url) return
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
    downloadedPath.value = await downloadUpdate(updateInfo.value.asset_url)
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

onMounted(async () => {
  try {
    version.value = await getAppVersion()
    runtime.value = isElectron ? 'electron' : 'tauri'
  } catch {
    version.value = ''
  }
  document.addEventListener('keydown', escHandler)
  checkUpdate()
})
onUnmounted(() => {
  document.removeEventListener('keydown', escHandler)
  stopProgressListener?.()
})

const escHandler = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close') }
</script>

<template>
  <div class="modal-overlay" @click.self="emit('close')">
    <div class="modal-dialog">
      <div class="modal-header">
        <span class="modal-title">{{ t('about.title') }}</span>
        <button class="modal-close" @click="emit('close')">&#x2715;</button>
      </div>
      <div class="modal-body">
        <div class="about-hero">
          <img src="/src-tauri/icons/128x128.png" alt="AidTerm" class="about-icon" draggable="false" />
          <div class="about-name">AidTerm</div>
          <div v-if="version" class="about-version">v{{ version }}</div>
        </div>

        <div class="about-desc">{{ t('about.description') }}</div>

        <div class="about-update">
          <div class="update-row">
            <button class="update-btn" :disabled="checking || downloading" @click="checkUpdate">
              {{ checking ? t('about.checking') : t('about.check_update') }}
            </button>
          </div>

          <div class="update-result">
            <div v-if="checkError" class="update-status update-error">{{ checkError }}</div>

            <template v-else-if="updateInfo">
              <div v-if="!updateInfo.has_update" class="update-status update-ok">
                {{ t('about.up_to_date', { version: updateInfo.current_version }) }}
              </div>

              <template v-else>
                <div class="update-status update-new">
                  {{ t('about.update_available', { version: updateInfo.latest_version }) }}
                </div>
                <div v-if="updateInfo.published_at" class="update-meta">
                  {{ t('about.published') }}: {{ new Date(updateInfo.published_at).toLocaleDateString() }}
                </div>
                <div class="update-meta">
                  {{ t('about.installer') }}: {{ installerLabel(updateInfo.installer_type) }}
                </div>
                <div v-if="updateInfo.body" class="update-body">{{ updateInfo.body }}</div>

                <div v-if="downloading" class="update-progress">
                  <div class="progress-bar">
                    <div class="progress-fill" :style="{ width: downloadProgress + '%' }" />
                  </div>
                  <span class="progress-text">{{ downloadProgress }}%</span>
                </div>

                <div class="update-row">
                  <button v-if="!downloadedPath" class="update-btn primary" :disabled="downloading || !updateInfo.asset_url" @click="doDownload">
                    {{ downloading ? t('about.downloading') + '...' : t('about.download_install') }}
                  </button>
                  <button v-else class="update-btn primary" @click="doInstall">
                    {{ t('about.install_restart') }}
                  </button>
                </div>
                <div v-if="installError" class="update-status update-error">{{ installError }}</div>
              </template>
            </template>
          </div>
        </div>

        <div class="about-links">
          <a class="about-link" href="https://github.com/jwlsn/aidterm" target="_blank" rel="noopener">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
              <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
            </svg>
            <span>{{ t('about.github') }}</span>
          </a>
        </div>

        <div class="about-tech">
          <span>{{ runtime === 'tauri' ? 'Tauri 2' : 'Electron' }}</span>
          <span class="about-dot">&middot;</span>
          <span>Vue 3</span>
          <span class="about-dot">&middot;</span>
          <span>{{ runtime === 'tauri' ? 'Rust' : 'Node.js' }}</span>
          <span class="about-dot">&middot;</span>
          <span>TypeScript</span>
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
  background: var(--overlay);
}

.modal-dialog {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
  border-radius: 8px;
  width: 420px;
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

.modal-body {
  padding: 32px 24px 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 20px;
}

.about-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.about-icon {
  width: 72px;
  height: 72px;
  border-radius: 16px;
}

.about-name {
  font-size: 22px;
  font-weight: 700;
  color: var(--text);
  letter-spacing: -0.3px;
}

.about-version {
  font-size: 13px;
  color: var(--text-sub0);
  background: var(--bg-surface0);
  padding: 2px 10px;
  border-radius: 10px;
}

.about-desc {
  font-size: 13px;
  color: var(--text-sub0);
  text-align: center;
  line-height: 1.5;
  max-width: 320px;
}

.about-update {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  width: 100%;
  max-width: 320px;
}

.update-row {
  display: flex;
  justify-content: center;
}

.update-btn {
  padding: 6px 16px;
  border: 1px solid var(--bg-surface0);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 6px;
  cursor: pointer;
  font-size: 12px;
  transition: background 0.15s, border-color 0.15s;
}
.update-btn:hover:not(:disabled) {
  background: var(--bg-surface1);
  border-color: var(--bg-surface1);
}
.update-btn.primary {
  background: var(--accent);
  color: var(--bg-base);
  border-color: var(--accent);
  font-weight: 600;
}
.update-btn.primary:hover:not(:disabled) {
  background: var(--accent-hover);
}
.update-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.update-result {
  width: 100%;
  min-height: 18px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}

.update-status {
  font-size: 12px;
  text-align: center;
  line-height: 1.5;
}
.update-ok {
  color: var(--success);
}
.update-new {
  color: var(--accent);
  font-weight: 600;
}
.update-error {
  color: var(--danger);
  word-break: break-word;
}

.update-meta {
  font-size: 11px;
  color: var(--text-sub0);
}

.update-body {
  font-size: 11px;
  color: var(--text-sub0);
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  padding: 8px 10px;
  max-height: 140px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
  text-align: left;
  width: 100%;
}

.update-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
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

.about-links {
  display: flex;
  gap: 12px;
}

.about-link {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  color: var(--text);
  text-decoration: none;
  font-size: 12px;
  transition: background 0.15s, border-color 0.15s;
}

.about-link:hover {
  background: var(--bg-surface0);
  border-color: var(--bg-surface1);
}

.about-tech {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-overlay0);
}

.about-dot {
  color: var(--bg-surface1);
}
</style>
