<script setup lang="ts">
import { ref, nextTick, watch } from 'vue'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  sshConnect: [host: string, port: number, username: string]
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
      placeholder="ssh user@host[:port]"
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
  background: #181825;
  border-bottom: 1px solid #313244;
}

.qc-prompt {
  color: #89b4fa;
  font-size: 14px;
  font-weight: bold;
}

.qc-input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: #cdd6f4;
  font-size: 13px;
  font-family: Consolas, 'Courier New', monospace;
}

.qc-input::placeholder {
  color: #585b70;
}
</style>
