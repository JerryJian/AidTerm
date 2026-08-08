<script setup lang="ts">
import { computed } from 'vue'
import { useTerminalStore } from '../../stores/terminal'
import { useI18n } from 'vue-i18n'

const store = useTerminalStore()
const { t } = useI18n()

const leafTab = computed(() => {
  const tab = store.activeTab
  if (!tab) return null
  const leafId = store.leafIdOf(tab)
  if (!leafId || leafId === tab.id) return tab
  return store.findTab(leafId) ?? tab
})

const statusText = computed(() => {
  const tab = leafTab.value
  if (!tab) return ''
  const s = tab.session
  if (!s) return t('status.local')

  const parts: string[] = [s.type.toUpperCase()]

  if (s.type === 'local' && s.command) {
    parts.push(s.command)
  }

  if (s.status === 'connected' || s.status === 'connecting') {
    const ssh = tab.sshInfo
    if (ssh) {
      parts.push(`${ssh.username}@${ssh.host}:${ssh.port}`)
      if (ssh.privateKeyPath) {
        const keyName = ssh.privateKeyPath.split(/[/\\]/).pop() || 'key'
        parts.push(`${t('status.key')}: ${keyName}`)
      } else {
        parts.push(t('status.password'))
      }
    }
    const telnet = tab.telnetInfo
    if (telnet) {
      parts.push(`${telnet.host}:${telnet.port}`)
    }

    const serial = tab.serialInfo
    if (serial) {
      parts.push(`${serial.portName} @ ${serial.baudRate} baud`)
    }

    const adb = tab.adbInfo
    if (adb) {
      parts.push(adb.model && adb.model !== adb.serial ? `${adb.serial} (${adb.model})` : adb.serial)
    }

    if (s.status === 'connected' && tab.systemInfo) {
      parts.push(`${tab.systemInfo.hostname} (${tab.systemInfo.os} ${tab.systemInfo.arch})`)
    }
  }

  parts.push(t('status.' + s.status))
  return parts.join(' │ ')
})

const statusTooltip = computed(() => {
  const tab = leafTab.value
  if (!tab) return ''
  const s = tab.session
  if (!s) return t('status.local')

  const lines: string[] = [
    `${t('status.type')}: ${s.type.toUpperCase()}`,
    `${t('status.status')}: ${t('status.' + s.status)}`,
  ]

  if (s.type === 'local' && s.command) {
    lines.push(`${t('status.shell')}: ${s.command}`)
  }

  const ssh = tab.sshInfo
  if (ssh) {
    lines.push(`${t('status.host')}: ${ssh.host}`)
    lines.push(`${t('status.port')}: ${ssh.port}`)
    lines.push(`${t('status.user')}: ${ssh.username}`)
    if (ssh.privateKeyPath) {
      lines.push(`${t('status.auth')}: ${t('status.key')} (${ssh.privateKeyPath})`)
    } else {
      lines.push(`${t('status.auth')}: ${t('status.password')}`)
    }
    if (ssh.agentForwarding) {
      lines.push(`${t('status.agent')}: ${t('status.enabled')}`)
    }
    if (ssh.x11Forwarding) {
      lines.push(`X11: ${t('status.enabled')}`)
    }
  }

  const telnet = tab.telnetInfo
  if (telnet) {
    lines.push(`${t('status.host')}: ${telnet.host}`)
    lines.push(`${t('status.port')}: ${telnet.port}`)
  }

  const serial = tab.serialInfo
  if (serial) {
    lines.push(`Port: ${serial.portName}`)
    lines.push(`Baud: ${serial.baudRate}`)
    lines.push(`Data: ${serial.dataBits}${serial.stopBits}${serial.parity[0]}${serial.flowControl[0]}`)
  }

  const adb = tab.adbInfo
  if (adb) {
    lines.push(`Serial: ${adb.serial}`)
    if (adb.model && adb.model !== adb.serial) lines.push(`Model: ${adb.model}`)
    if (adb.product) lines.push(`Product: ${adb.product}`)
  }

  if (tab.systemInfo) {
    lines.push('---')
    lines.push(`${t('status.hostname')}: ${tab.systemInfo.hostname}`)
    lines.push(`OS: ${tab.systemInfo.os} ${tab.systemInfo.arch}`)
    lines.push(`${t('status.kernel')}: ${tab.systemInfo.kernel}`)
    lines.push(`${t('status.shell')}: ${tab.systemInfo.shell}`)
  }

  return lines.join('\n')
})
</script>

<template>
  <div class="status-bar">
    <span v-if="statusText" class="status-left" :title="statusTooltip">
      <span v-if="leafTab" class="status-dot" :class="leafTab.session?.status" />
      {{ statusText }}
    </span>
    <span v-else class="status-left" />
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
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-right {
  color: var(--text-overlay0);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
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
