<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SearchAddon } from '@xterm/addon-search'
import { invoke } from '@tauri-apps/api/core'
import { useTerminal } from '../../hooks/useTerminal'
import { useTerminalStore } from '../../stores/terminal'
import { useThemeStore } from '../../stores/themeStore'
import { useAiConversation } from '../../hooks/useAiConversation'
import type { SshConnectionInfo, TelnetConnectionInfo, SystemInfo } from '../../types'
import AiConfirmOverlay from '../ai/AiConfirmOverlay.vue'
import { useAiStore } from '../../stores/aiStore'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
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
let resizeObserver: ResizeObserver | null = null
let fallbackTimer: ReturnType<typeof setTimeout> | null = null
let lastSize = { w: 0, h: 0 }
const suppressOutput = ref(false)

const { createSession, sshConnect, telnetConnect, writeInput, resize, onOutput, killSession } = useTerminal()

function getXtermTheme() {
  const s = getComputedStyle(document.documentElement)
  return {
    background: s.getPropertyValue('--bg-base').trim() || '#1e1e2e',
    foreground: s.getPropertyValue('--text').trim() || '#cdd6f4',
    cursor: s.getPropertyValue('--rosewater').trim() || '#f5e0dc',
    selectionBackground: s.getPropertyValue('--text-overlay0').trim() || '#585b70',
    black: s.getPropertyValue('--bg-surface1').trim() || '#45475a',
    red: s.getPropertyValue('--danger').trim() || '#f38ba8',
    green: s.getPropertyValue('--success').trim() || '#a6e3a1',
    yellow: s.getPropertyValue('--warning').trim() || '#f9e2af',
    blue: s.getPropertyValue('--accent').trim() || '#89b4fa',
    magenta: s.getPropertyValue('--pink').trim() || '#f5c2e7',
    cyan: s.getPropertyValue('--teal').trim() || '#94e2d5',
    white: s.getPropertyValue('--text-sub1').trim() || '#bac2de',
    brightBlack: s.getPropertyValue('--text-overlay0').trim() || '#585b70',
    brightRed: s.getPropertyValue('--danger').trim() || '#f38ba8',
    brightGreen: s.getPropertyValue('--success').trim() || '#a6e3a1',
    brightYellow: s.getPropertyValue('--warning').trim() || '#f9e2af',
    brightBlue: s.getPropertyValue('--accent').trim() || '#89b4fa',
    brightMagenta: s.getPropertyValue('--pink').trim() || '#f5c2e7',
    brightCyan: s.getPropertyValue('--teal').trim() || '#94e2d5',
    brightWhite: s.getPropertyValue('--text-sub0').trim() || '#a6adc8',
  }
}

function stripAnsi(text: string): string {
  return text.replace(/\x1b\[[\d;]*[a-zA-Z]/g, '').replace(/\r/g, '')
}

function stripLeadingEcho(output: string, cmd: string): string {
  const idx = output.indexOf(cmd)
  if (idx === 0) {
    let rest = output.slice(cmd.length)
    if (rest.startsWith('\r\n')) rest = rest.slice(2)
    else if (rest.startsWith('\n')) rest = rest.slice(1)
    else if (rest.startsWith('\r')) rest = rest.slice(1)
    return rest
  }
  return output
}

function stripTrailingPrompt(output: string, prompt: string): string {
  if (!prompt || prompt === '$ ' || !output) return output
  const plain = output.replace(/\x1b\[[\d;]*[a-zA-Z]/g, '')
  if (plain.endsWith(prompt.trimEnd())) {
    return output.slice(0, -prompt.trimEnd().length)
  }
  return output
}

function stripMarkerFromOutput(text: string, marker: string): string {
  const escaped = marker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const re = new RegExp(`^.*?echo\\s+${escaped}.*$`, 'gm')
  return text.replace(re, '').replace(new RegExp(escaped, 'g'), '')
}

async function executeInTerminal(cmd: string, prompt?: string, silent?: boolean): Promise<string> {
  const p = prompt || '$ '
  return new Promise(async (resolve) => {
    suppressOutput.value = true

    let output = ''
    const unsub = (await onOutput((data: string) => {
      output += data
    })) ?? (() => {})

    if (!silent) terminal?.write(`${cmd}\r\n`)

    const marker = `__CMD_DONE_${Date.now()}__`
    writeInput(`${cmd}\recho ${marker}\r`)

    const t0 = Date.now()
    const MAX_WAIT = 30000

    const poll = () => {
      const idx = output.lastIndexOf(marker)
      if (idx >= 0) {
        unsub()
        output = output.slice(0, idx)
        finish()
        return
      }

      if (Date.now() - t0 > MAX_WAIT) {
        unsub()
        writeInput('\x03')
        finish()
        return
      }

      setTimeout(poll, 50)
    }
    setTimeout(poll, 50)

    function finish() {
      const clean = stripMarkerFromOutput(output, marker)

      if (silent) {
        suppressOutput.value = false
        const raw = stripAnsi(clean)
        const lines = raw.split(/\r?\n/)
        let startIdx = -1
        for (let i = 0; i < lines.length; i++) {
          if (lines[i].trim() === cmd) { startIdx = i; break }
        }
        const result = startIdx >= 0
          ? lines.slice(startIdx + 1).filter(l => l.trim()).slice(0, -1).join('\n')
          : lines.filter(l => l.trim()).slice(0, -1).join('\n')
        const last = raw.split(/\r?\n/).filter(l => l.trim()).pop()
        if (last && /[$#>]/.test(last)) terminal?.write(`\r\n${last}`)
        resolve(result.trim())
        return
      }

      let display = stripLeadingEcho(clean, cmd)
      display = stripTrailingPrompt(display, p)
      terminal?.write(display)

      suppressOutput.value = false
      resolve(stripAnsi(clean))
    }
  })
}

const aiConv = useAiConversation(() => terminal, writeInput, executeInTerminal, (v) => suppressOutput.value = v)

function handleTerminalData(data: string) {
  if (aiConv.interceptInput(data)) return
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
    fontFamily: 'Consolas, "Courier New", monospace',
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
      : await createSession(rows, cols, store.activeTab?.session?.subshell)

  if (id) {
    store.updateSessionId(store.activeTabId ?? '', id)
    store.updateSessionStatus(store.activeTabId ?? '', 'connected')
    const unsub = await onOutput((data: string) => {
      if (!suppressOutput.value) terminal?.write(data)
    })
    if (unsub) unlisten = unsub

    if (props.sshInfo) {
      const tabId = store.activeTabId
      if (tabId) {
        const info = await invoke<SystemInfo>('get_remote_system_info', {
          host: props.sshInfo.host,
          port: props.sshInfo.port,
          username: props.sshInfo.username,
          password: props.sshInfo.password,
          privateKeyPath: props.sshInfo.privateKeyPath ?? null,
        }).catch(() => null)
        if (info) {
          store.updateSystemInfo(tabId, info)
          store.updateTabTitle(tabId, `${info.os} | ${info.hostname}`)
        }
      }
    }
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
    aiConv.clearInputBuffer()
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
  closeContextMenu()
  const sel = terminal?.getSelection()
  if (!sel) return
  aiConv.startConversation(`${t('context_menu.ask_ai')}\n${sel}`)
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

defineExpose({ focusSearch, doFit })
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
      <button class="search-btn" @click="findNext">↓</button>
      <button class="search-btn" @click="findPrevious">↑</button>
      <button class="search-btn" @click="closeSearch">✕</button>
    </div>
    <div ref="terminalRef" class="terminal-xterm" />

    <AiConfirmOverlay
      v-if="aiConv.showConfirm.value"
      :command="aiConv.pendingCommand.value"
      :ai-message="aiConv.pendingAiMsg.value"
      @confirm="aiConv.onConfirmCommand()"
      @cancel="aiConv.onCancelCommand()"
      @modify="(cmd: string) => aiConv.onModifyCommand(cmd)"
    />

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
        <div v-if="aiStore.enabled" class="ctx-item" @click="doAskAi">🤖 {{ t('context_menu.ask_ai') }}</div>
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
  background: var(--bg-base);
}

.terminal-xterm {
  flex: 1;
  min-height: 0;
  min-width: 0;
  position: relative;
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

</style>
