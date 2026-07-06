<script setup lang="ts">
import { computed } from 'vue'
import { useTerminalStore } from '../../stores/terminal'

const store = useTerminalStore()

const statusText = computed(() => {
  const tab = store.activeTab
  if (!tab) return ''
  const s = tab.session
  if (!s) return 'local'
  const parts: string[] = [s.type.toUpperCase()]
  if (s.status === 'connected') {
    const ssh = tab.sshInfo
    if (ssh) parts.push(`${ssh.username}@${ssh.host}:${ssh.port}`)
    const telnet = tab.telnetInfo
    if (telnet) parts.push(`${telnet.host}:${telnet.port}`)
  }
  parts.push(s.status)
  return parts.join(' │ ')
})
</script>

<template>
  <div class="status-bar">
    <span class="status-left">
      <span v-if="store.activeTab" class="status-dot" :class="store.activeTab.session?.status" />
      {{ statusText }}
    </span>
    <span class="status-right">
      {{ store.tabs.length }} tab{{ store.tabs.length !== 1 ? 's' : '' }}
    </span>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 24px;
  padding: 0 12px;
  background: #181825;
  border-top: 1px solid #313244;
  font-size: 11px;
  color: #a6adc8;
  user-select: none;
  flex-shrink: 0;
}

.status-left {
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-right {
  color: #585b70;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot.connected {
  background: #a6e3a1;
}

.status-dot.connecting {
  background: #f9e2af;
}

.status-dot.disconnected {
  background: #45475a;
}
</style>
