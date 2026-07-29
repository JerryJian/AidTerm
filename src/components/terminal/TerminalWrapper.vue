<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SearchAddon } from '@xterm/addon-search'
import { invoke, listen } from '@/api'
import { useTerminal } from '../../hooks/useTerminal'
import { useTerminalStore } from '../../stores/terminal'
import { useThemeStore } from '../../stores/themeStore'
import { useAiConversation } from '../../hooks/useAiConversation'
import type { SshConnectionInfo, TelnetConnectionInfo, SerialConnectionInfo, SystemInfo } from '../../types'
import { useAiStore } from '../../stores/aiStore'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
  serialInfo?: SerialConnectionInfo
  aiSessionId?: string
}>()

const emit = defineEmits<{
  titleChange: [title: string]
  newSsh: []
}>()

const terminalRef = ref<HTMLDivElement>()
const searchAddon = ref<SearchAddon>()
const searchVisible = ref(false)
const searchQuery = ref('')
const ctxMenu = ref<{ x: number; y: number }>({ x: 0, y: 0 })
const ctxVisible = ref(false)

const { t } = useI18n()
const store = useTerminalStore()
const aiStore = useAiStore()
const themeStore = useThemeStore()

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let unlisten: (() => void) | null = null
const unlisteners: (() => void)[] = []
let resizeObserver: ResizeObserver | null = null
let fallbackTimer: ReturnType<typeof setTimeout> | null = null
let lastSize = { w: 0, h: 0 }

const { createSession, sshConnect, telnetConnect, serialConnect, writeInput, resize, onOutput, killSession } = useTerminal()

const aiConv = useAiConversation(() => terminal, writeInput, onOutput, props.aiSessionId)

function getXtermTheme() {
  const s = getComputedStyle(document.documentElement)
  const v = (name: string, fallback: string) => s.getPropertyValue(name).trim() || fallback
  return {
    background: v('--bg-base', '#1e1e1e'),
    foreground: v('--text-sub1', '#cccccc'),
    cursor: v('--text', '#d4d4d4'),
    selectionBackground: v('--term-selection', '#264f78'),
    black: v('--term-black', '#000000'),
    red: v('--term-red', '#cd3131'),
    green: v('--term-green', '#00bc00'),
    yellow: v('--term-yellow', '#949800'),
    blue: v('--term-blue', '#0451a5'),
    magenta: v('--term-magenta', '#bc05bc'),
    cyan: v('--term-cyan', '#0598bc'),
    white: v('--term-white', '#555555'),
    brightBlack: v('--term-bright-black', '#666666'),
    brightRed: v('--term-bright-red', '#cd3131'),
    brightGreen: v('--term-bright-green', '#14ce14'),
    brightYellow: v('--term-bright-yellow', '#b5ba00'),
    brightBlue: v('--term-bright-blue', '#0451a5'),
    brightMagenta: v('--term-bright-magenta', '#bc05bc'),
    brightCyan: v('--term-bright-cyan', '#0598bc'),
    brightWhite: v('--term-bright-white', '#a5a5a5'),
  }
}

function handleTerminalData(data: string) {
  writeInput(data)
}

function doFit(): boolean {
  if (!fitAddon || !terminalRef.value || !terminal) return false

  const w = terminalRef.value.clientWidth
  const h = terminalRef.value.clientHeight

  if (w < 50 || h < 50) return false
  if (Math.abs(w - lastSize.w) < 2 && Math.abs(h - lastSize.h) < 2) return false

  try {
    fitAddon.fit()
    lastSize = { w, h }
    return true
  } catch {
    return false
  }
}

function scheduleFit(maxAttempts = 10) {
  let n = 0
  const tryFit = () => {
    n++
    if (doFit()) return
    if (n < maxAttempts) requestAnimationFrame(tryFit)
  }
  requestAnimationFrame(tryFit)
}

async function initTerminal() {
  if (!terminalRef.value) return

  fitAddon = new FitAddon()
  searchAddon.value = new SearchAddon()

  terminal = new Terminal({
    cursorBlink: true,
    cursorStyle: 'block',
    fontSize: 14,
    fontFamily: 'Consolas, "Courier New", Menlo, "SF Mono", monospace',
    allowTransparency: true,
    cols: 80,
    rows: 24,
    theme: getXtermTheme(),
  })

  terminal.loadAddon(fitAddon)
  terminal.loadAddon(searchAddon.value)
  terminal.loadAddon(new WebLinksAddon())

  terminal.onData((data: string) => {
    handleTerminalData(data)
  })

  terminal.onTitleChange((title: string) => {
    emit('titleChange', title)
  })

  terminal.open(terminalRef.value)

  await nextTick()

  doFit()

  terminal.onResize(({ rows, cols }) => {
    resize(rows, cols)
  })

  scheduleFit(10)

  fallbackTimer = setTimeout(() => doFit(), 300)

  resizeObserver = new ResizeObserver(() => {
    requestAnimationFrame(() => doFit())
  })
  resizeObserver.observe(terminalRef.value)

  const rows = terminal.rows
  const cols = terminal.cols

  let sessionId: string | null = null
  const unsubStatus = await listen<{ session_id: string; status: string; error?: string }>('session-status', event => {
    if (event.payload.session_id === sessionId) {
      if (event.payload.status === 'connected') {
        store.updateSessionStatus(store.activeTabId ?? '', 'connected')
      } else if (event.payload.status === 'disconnected') {
        store.updateSessionStatus(store.activeTabId ?? '', 'disconnected')
      }
    }
  })
  if (unsubStatus) unlisteners.push(unsubStatus)

  try {
    const id = props.sshInfo
      ? await sshConnect(
          props.sshInfo.host,
          props.sshInfo.port,
          props.sshInfo.username,
          props.sshInfo.password,
          props.sshInfo.privateKeyPath,
          props.sshInfo.proxyId,
          props.sshInfo.agentForwarding,
          props.sshInfo.x11Forwarding,
          rows,
          cols,
        )
      : props.telnetInfo
        ? await telnetConnect(props.telnetInfo.host, props.telnetInfo.port)
        : props.serialInfo
          ? await serialConnect(props.serialInfo)
          : await createSession(rows, cols, store.activeTab?.session?.command, store.activeTab?.session?.workingDir)

    sessionId = id
    store.updateSessionId(store.activeTabId ?? '', id)
    if (!props.sshInfo) {
      store.updateSessionStatus(store.activeTabId ?? '', 'connected')
    }
    const unsub = await onOutput((data: string) => {
      terminal?.write(data)
    })
    if (unsub) unlisten = unsub

    if (props.sshInfo) {
      const tabId = store.activeTabId
      if (tabId) {
        invoke<SystemInfo>('get_remote_system_info', {
          sessionId: id,
        }).then(info => {
          store.updateSystemInfo(tabId, info)
          store.updateTabTitle(tabId, `${info.os} | ${info.hostname}`)
        }).catch((e: any) => {
          console.error('get_remote_system_info error:', e)
          terminal?.writeln(`\r\n\x1b[1;31m[System Info Error: ${typeof e === 'string' ? e : e?.message ?? e}]\x1b[0m`)
        })
      }
    }
  } catch (e) {
    const errMsg = typeof e === 'string' ? e : e instanceof Error ? e.message : 'Connection failed'
    terminal?.writeln(`\r\n\x1b[1;31mError:\x1b[0m ${errMsg}`)
    store.updateSessionStatus(store.activeTabId ?? '', 'disconnected')
  }
}

onMounted(() => {
  initTerminal()

  const stopWatch = watch(() => themeStore.mode, () => {
    if (terminal) {
      terminal.options.theme = getXtermTheme()
    }
  })
  onUnmounted(() => stopWatch())
})

onUnmounted(() => {
  killSession()
  if (fallbackTimer) clearTimeout(fallbackTimer)
  if (resizeObserver) resizeObserver.disconnect()
  if (unlisten) unlisten()
  unlisteners.forEach(fn => fn())
  terminal?.dispose()
  terminal = null
  fitAddon = null
})

function focusSearch() {
  searchVisible.value = true
  nextTick(() => {
    const input = document.querySelector('.search-input') as HTMLInputElement
    input?.focus()
  })
}

function findNext() {
  searchAddon.value?.findNext(searchQuery.value)
}

function findPrevious() {
  searchAddon.value?.findPrevious(searchQuery.value)
}

function closeSearch() {
  searchVisible.value = false
  searchQuery.value = ''
  terminal?.focus()
}

function onTerminalKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.key === 'c' && !e.altKey && !e.metaKey) {
    if (terminal?.hasSelection()) {
      e.preventDefault()
      e.stopPropagation()
      copyText(terminal.getSelection())
    }
    return
  }
  if (e.ctrlKey && e.key === 'v' && !e.altKey && !e.metaKey) {
    e.preventDefault()
    e.stopPropagation()
    pasteOrSend()
    return
  }
}

function showContextMenu(e: MouseEvent) {
  e.preventDefault()
  ctxMenu.value = { x: e.clientX, y: e.clientY }
  ctxVisible.value = true
}

function closeContextMenu() {
  ctxVisible.value = false
}

async function copyText(text: string) {
  try {
    await invoke('plugin:clipboard-manager|write_text', { text })
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    ta.style.pointerEvents = 'none'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
}

async function readClipboard(): Promise<string> {
  try {
    return await invoke('plugin:clipboard-manager|read_text')
  } catch {
    try {
      return await navigator.clipboard.readText()
    } catch {
      return ''
    }
  }
}

async function pasteOrSend() {
  const text = await readClipboard()
  if (text) {
    writeInput(text)
  } else {
    writeInput('\x16')
  }
}

function doCopy() {
  const text = terminal?.getSelection()
  closeContextMenu()
  if (text) copyText(text)
}

async function doPaste() {
  closeContextMenu()
  const text = await readClipboard()
  if (text) writeInput(text)
}

function doSelectAll() {
  closeContextMenu()
  terminal?.selectAll()
}

function doClear() {
  closeContextMenu()
  terminal?.clear()
}

function doToggleSearch() {
  closeContextMenu()
  if (searchVisible.value) {
    closeSearch()
  } else {
    focusSearch()
  }
}

function doAskAi() {
  const sel = terminal?.getSelection()
  closeContextMenu()
  if (!sel) return
  if (store.activeTabId && !store.activeTab?.aiSidebarOpen) {
    store.toggleAiSidebar(store.activeTabId)
  }
  nextTick(() => aiConv.startConversation(sel))
}

function doNewTab() {
  closeContextMenu()
  store.addTab('local')
}

function doNewSsh() {
  closeContextMenu()
  emit('newSsh')
}

function doCloseTab() {
  closeContextMenu()
  store.closeTab(store.activeTabId ?? '')
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && ctxVisible.value) {
    closeContextMenu()
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown)
})

defineExpose({ focusSearch, doFit, aiConv })
</script>

<template>
  <div class="terminal-container" @contextmenu="showContextMenu" @keydown.capture="onTerminalKeydown">
    <div class="search-bar" v-if="searchVisible">
      <input
        v-model="searchQuery"
        class="search-input"
        :placeholder="t('context_menu.search')"
        @keydown.enter="findNext"
        @keydown.shift.enter="findPrevious"
        @keydown.escape="closeSearch"
      />
      <button class="search-btn" @click="findNext">&#x2193;</button>
      <button class="search-btn" @click="findPrevious">&#x2191;</button>
      <button class="search-btn" @click="closeSearch">&#x2715;</button>
    </div>
    <div ref="terminalRef" class="terminal-xterm" />
    <div v-if="store.activeTab?.session?.status === 'connecting'" class="connecting-overlay">
      <div class="spinner" />
      <span>{{ t('terminal.connecting') }}</span>
    </div>

    <!-- Context menu -->
    <teleport to="body">
      <div v-if="ctxVisible" class="ctx-backdrop" @click="closeContextMenu" @contextmenu.prevent="closeContextMenu" />
      <div v-if="ctxVisible" class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }">
        <div class="ctx-item" @click="doCopy">{{ t('context_menu.copy') }}</div>
        <div class="ctx-item" @click="doPaste">{{ t('context_menu.paste') }}</div>
        <div class="ctx-item" @click="doSelectAll">{{ t('context_menu.select_all') }}</div>
        <div class="ctx-sep" />
        <div class="ctx-item" @click="doToggleSearch">{{ t('context_menu.search') }}</div>
        <div class="ctx-item" @click="doClear">{{ t('context_menu.clear') }}</div>
        <div class="ctx-sep" />
        <div class="ctx-item" @click="doNewTab">{{ t('context_menu.new_tab') }}</div>
        <div class="ctx-item" @click="doNewSsh">{{ t('context_menu.new_ssh') }}</div>
        <div class="ctx-sep" />
        <div v-if="aiStore.enabled" class="ctx-item" @click="doAskAi">{{ t('context_menu.ask_ai') }}</div>
        <div class="ctx-sep" />
        <div class="ctx-item ctx-danger" @click="doCloseTab">{{ t('context_menu.close_tab') }}</div>
      </div>
    </teleport>
  </div>
</template>

<style scoped>
.terminal-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  min-width: 0;
  position: relative;
  background: var(--bg-base);
}

.terminal-xterm {
  flex: 1;
  min-height: 0;
  min-width: 0;
  position: relative;
  padding: 4px;
}

.search-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.search-input {
  flex: 1;
  max-width: 200px;
  padding: 4px 8px;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  font-size: 12px;
  outline: none;
}

.search-input:focus {
  border-color: var(--accent);
}

.search-btn {
  padding: 4px 8px;
  background: var(--bg-surface0);
  border: 1px solid var(--bg-surface1);
  color: var(--text);
  cursor: pointer;
  font-size: 12px;
}

.search-btn:hover {
  background: var(--bg-surface1);
}

.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 999;
}

.ctx-menu {
  position: fixed;
  z-index: 1000;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface0);
  border-radius: 6px;
  padding: 4px 0;
  min-width: 160px;
  box-shadow: 0 4px 16px rgba(0,0,0,.4);
}

.ctx-item {
  padding: 6px 16px;
  font-size: 12px;
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
}

.ctx-item:hover {
  background: var(--bg-surface0);
}

.ctx-danger:hover {
  color: var(--danger);
}

.ctx-sep {
  height: 1px;
  margin: 4px 8px;
  background: var(--bg-surface0);
}

.connecting-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  background: var(--bg-base);
  color: var(--text-sub0);
  font-size: 14px;
  z-index: 10;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--bg-surface1);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

</style>
