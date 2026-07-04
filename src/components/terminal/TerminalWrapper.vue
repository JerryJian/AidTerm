<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SearchAddon } from '@xterm/addon-search'
import { invoke } from '@tauri-apps/api/core'
import { useTerminal } from '../../hooks/useTerminal'
import { useTerminalStore } from '../../stores/terminal'
import { useAiConversation } from '../../hooks/useAiConversation'
import type { SshConnectionInfo, TelnetConnectionInfo } from '../../types'
import AiConfirmOverlay from '../ai/AiConfirmOverlay.vue'
import { useAiStore } from '../../stores/aiStore'

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

const store = useTerminalStore()
const aiStore = useAiStore()

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let unlisten: (() => void) | null = null
let resizeObserver: ResizeObserver | null = null
let fallbackTimer: ReturnType<typeof setTimeout> | null = null
let lastSize = { w: 0, h: 0 }

const aiConv = useAiConversation(() => terminal)

const { createSession, sshConnect, telnetConnect, writeInput, resize, onOutput } = useTerminal()

function handleTerminalData(data: string) {
  const consumed = aiConv.interceptInput(data)
  if (consumed) {
    if (aiConv.justActivated.value) {
      writeInput('\x03')
      aiConv.justActivated.value = false
    }
    return
  }
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
    theme: {
      background: '#1e1e2e',
      foreground: '#cdd6f4',
      cursor: '#f5e0dc',
      selectionBackground: '#585b70',
      black: '#45475a',
      red: '#f38ba8',
      green: '#a6e3a1',
      yellow: '#f9e2af',
      blue: '#89b4fa',
      magenta: '#f5c2e7',
      cyan: '#94e2d5',
      white: '#bac2de',
      brightBlack: '#585b70',
      brightRed: '#f38ba8',
      brightGreen: '#a6e3a1',
      brightYellow: '#f9e2af',
      brightBlue: '#89b4fa',
      brightMagenta: '#f5c2e7',
      brightCyan: '#94e2d5',
      brightWhite: '#a6adc8',
    },
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

  scheduleFit(10)

  fallbackTimer = setTimeout(() => doFit(), 300)

  resizeObserver = new ResizeObserver(() => {
    requestAnimationFrame(() => doFit())
  })
  resizeObserver.observe(terminalRef.value)

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
      )
    : props.telnetInfo
      ? await telnetConnect(props.telnetInfo.host, props.telnetInfo.port)
      : await createSession()

  if (id) {
    store.updateSessionId(store.activeTabId ?? '', id)
    const unsub = await onOutput((data: string) => {
      terminal?.write(data)
    })
    if (unsub) unlisten = unsub
  }

  terminal.onResize(({ rows, cols }) => {
    resize(rows, cols)
  })
}

onMounted(() => {
  initTerminal()
})

onUnmounted(() => {
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
  aiConv.writeAI(`分析以下终端输出：\n${sel}`)
  aiConv.startConversation(`分析以下终端输出，解释其含义：\n${sel}`)
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
        placeholder="Search..."
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

    <div v-if="aiConv.aiActive.value" class="ai-bar">
      <span class="ai-dot">●</span> AI 模式 — 输入 <code>/exit</code> 退出
    </div>

    <!-- Context menu -->
    <teleport to="body">
      <div v-if="ctxVisible" class="ctx-backdrop" @click="closeContextMenu" @contextmenu.prevent="closeContextMenu" />
      <div v-if="ctxVisible" class="ctx-menu" :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }">
        <div class="ctx-item" @click="doCopy">复制</div>
        <div class="ctx-item" @click="doPaste">粘贴</div>
        <div class="ctx-item" @click="doSelectAll">全选</div>
        <div class="ctx-sep" />
        <div class="ctx-item" @click="doToggleSearch">搜索</div>
        <div class="ctx-item" @click="doClear">清除终端</div>
        <div class="ctx-sep" />
        <div class="ctx-item" @click="doNewTab">新建标签</div>
        <div class="ctx-item" @click="doNewSsh">新建 SSH 连接...</div>
        <div class="ctx-sep" />
        <div v-if="aiStore.enabled" class="ctx-item" @click="doAskAi">🤖 AI 解释选中内容</div>
        <div v-if="aiConv.aiActive.value" class="ctx-item" @click="aiConv.resetConversation()">🔄 重置 AI 对话</div>
        <div class="ctx-sep" />
        <div class="ctx-item ctx-danger" @click="doCloseTab">关闭标签</div>
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
  background: #1e1e2e;
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
  background: #181825;
  border-bottom: 1px solid #313244;
}

.search-input {
  flex: 1;
  max-width: 200px;
  padding: 4px 8px;
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  font-size: 12px;
  outline: none;
}

.search-input:focus {
  border-color: #89b4fa;
}

.search-btn {
  padding: 4px 8px;
  background: #313244;
  border: 1px solid #45475a;
  color: #cdd6f4;
  cursor: pointer;
  font-size: 12px;
}

.search-btn:hover {
  background: #45475a;
}

.ctx-backdrop {
  position: fixed;
  inset: 0;
  z-index: 999;
}

.ctx-menu {
  position: fixed;
  z-index: 1000;
  background: #181825;
  border: 1px solid #313244;
  border-radius: 6px;
  padding: 4px 0;
  min-width: 160px;
  box-shadow: 0 4px 16px rgba(0,0,0,.4);
}

.ctx-item {
  padding: 6px 16px;
  font-size: 12px;
  color: #cdd6f4;
  cursor: pointer;
  white-space: nowrap;
}

.ctx-item:hover {
  background: #313244;
}

.ctx-danger:hover {
  color: #f38ba8;
}

.ctx-sep {
  height: 1px;
  margin: 4px 8px;
  background: #313244;
}

.ai-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: #1a1a2e;
  border-top: 1px solid #4547a0;
  font-size: 12px;
  color: #89b4fa;
  flex-shrink: 0;
}

.ai-bar .ai-dot {
  color: #a6e3a1;
  font-size: 10px;
  animation: ai-pulse 1.5s infinite;
}

.ai-bar code {
  background: #313244;
  padding: 1px 5px;
  border-radius: 3px;
  font-size: 11px;
  color: #cdd6f4;
}

@keyframes ai-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
</style>
