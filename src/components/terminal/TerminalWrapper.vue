<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SearchAddon } from '@xterm/addon-search'
import { useTerminal } from '../../hooks/useTerminal'
import type { SshConnectionInfo, TelnetConnectionInfo } from '../../types'

const props = defineProps<{
  sshInfo?: SshConnectionInfo
  telnetInfo?: TelnetConnectionInfo
}>()

const emit = defineEmits<{
  titleChange: [title: string]
}>()

const terminalRef = ref<HTMLDivElement>()
const searchAddon = ref<SearchAddon>()
const searchVisible = ref(false)
const searchQuery = ref('')

let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let unlisten: (() => void) | null = null
let resizeObserver: ResizeObserver | null = null
let fallbackTimer: ReturnType<typeof setTimeout> | null = null
let lastSize = { w: 0, h: 0 }

const { createSession, sshConnect, telnetConnect, writeInput, resize, onOutput } = useTerminal()

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
      )
    : props.telnetInfo
      ? await telnetConnect(props.telnetInfo.host, props.telnetInfo.port)
      : await createSession()

  if (id) {
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

defineExpose({ focusSearch, doFit })
</script>

<template>
  <div class="terminal-container">
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
</style>
