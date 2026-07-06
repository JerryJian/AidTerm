<script setup lang="ts">
import { computed } from 'vue'
import { useTerminalStore } from '../../stores/terminal'

const store = useTerminalStore()

import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const statusText = computed(() => {
  const tab = store.activeTab
  if (!tab) return ''
  const s = tab.session
  if (!s) return t('status.local')
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
      {{ $t('status.tabs_count', { count: store.tabs.length }) }}
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
  background: var(--bg-mantle);
  border-top: 1px solid var(--bg-surface0);
  font-size: 11px;
  color: var(--text-sub0);
  user-select: none;
  flex-shrink: 0;
}

.status-left {
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-right {
  color: var(--text-overlay0);
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot.connected {
  background: var(--success);
}

.status-dot.connecting {
  background: var(--warning);
}

.status-dot.disconnected {
  background: var(--bg-surface1);
}
</style>
