<script setup lang="ts">
import { ref, nextTick, watch } from 'vue'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  sshConnect: [host: string, port: number, username: string]
  telnetConnect: [host: string, port: number]
  close: []
}>()

const input = ref('')
const inputRef = ref<HTMLInputElement>()

watch(() => props.visible, (v) => {
  if (v) {
    nextTick(() => inputRef.value?.focus())
  }
})

function parseTarget(input: string, defaultPort: number): { host: string; port: number; username?: string } | null {
  let s = input.trim()
  let username: string | undefined
  const at = s.lastIndexOf('@')
  if (at > 0) {
    username = s.slice(0, at)
    s = s.slice(at + 1)
  }

  let host: string
  let port: number
  if (s.startsWith('[')) {
    const close = s.indexOf(']')
    if (close === -1) return null
    host = s.slice(1, close)
    const rest = s.slice(close + 1)
    if (rest === '') {
      port = defaultPort
    } else {
      if (!rest.startsWith(':')) return null
      port = parseInt(rest.slice(1), 10)
      if (Number.isNaN(port)) return null
    }
  } else {
    const colonIdx = s.lastIndexOf(':')
    if (colonIdx > 0) {
      host = s.slice(0, colonIdx)
      const p = parseInt(s.slice(colonIdx + 1), 10)
      if (Number.isNaN(p)) return null
      port = p
    } else {
      host = s
      port = defaultPort
    }
  }

  if (!host || host.includes(':')) return null
  return { host, port, username }
}

function parseAndSubmit() {
  const raw = input.value.trim()
  if (!raw) return

  let str = raw
  if (str.startsWith('ssh ')) {
    str = str.slice(4).trim()
  }

  if (str.startsWith('telnet ')) {
    str = str.slice(7).trim()
    const parts = str.split(/\s+/)
    const parsed = parseTarget(parts[0], 23)
    input.value = ''
    emit('close')
    if (!parsed) return
    emit('telnetConnect', parsed.host, parsed.port)
    return
  }

  const parsed = parseTarget(str, 22)
  input.value = ''
  emit('close')
  if (!parsed) return

  emit('sshConnect', parsed.host, parsed.port, parsed.username || '')
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    input.value = ''
    emit('close')
  }
}
</script>

<template>
  <div v-if="visible" class="quick-connect">
    <span class="qc-prompt">→</span>
    <input
      ref="inputRef"
      v-model="input"
      class="qc-input"
      :placeholder="$t('quick_connect.placeholder')"
      @keydown.enter="parseAndSubmit"
      @keydown="onKeydown"
      @blur="emit('close')"
    />
  </div>
</template>

<style scoped>
.quick-connect {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.qc-prompt {
  color: var(--accent);
  font-size: 14px;
  font-weight: bold;
}

.qc-input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text);
  font-size: 13px;
  font-family: Consolas, 'Courier New', monospace;
}

.qc-input::placeholder {
  color: var(--text-overlay0);
}
</style>
