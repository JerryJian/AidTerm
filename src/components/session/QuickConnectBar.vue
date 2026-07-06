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
    const addr = parts[0]
    const colonIdx = addr.lastIndexOf(':')
    let host: string
    let port: number
    if (colonIdx > 0) {
      host = addr.slice(0, colonIdx)
      port = parseInt(addr.slice(colonIdx + 1)) || 23
    } else {
      host = addr
      port = 23
    }
    input.value = ''
    emit('close')
    emit('telnetConnect', host, port)
    return
  }

  const regex = /^(?:(\w[\w.-]*)@)?([\w.-]+)(?::(\d+))?$/
  const m = str.match(regex)
  if (!m) {
    input.value = ''
    emit('close')
    return
  }

  const username = m[1] || ''
  const host = m[2]
  const port = m[3] ? parseInt(m[3]) : 22

  input.value = ''
  emit('close')
  emit('sshConnect', host, port, username)
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
