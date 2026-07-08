<script setup lang="ts">
import { useTerminalStore } from '../../stores/terminal'
import type { ToolTab, TerminalTab } from '../../types'
import SftpPanel from '../sftp/SftpPanel.vue'
import TunnelPanel from '../tunnel/TunnelPanel.vue'
import ProxyPanel from '../proxy/ProxyPanel.vue'
import SnippetPanel from '../snippet/SnippetPanel.vue'
import TriggerPanel from '../trigger/TriggerPanel.vue'
import KeyManagerPanel from '../keychain/KeyManagerPanel.vue'
import KnownHostsPanel from '../keychain/KnownHostsPanel.vue'

const props = defineProps<{
  tabId: string
  tab: TerminalTab
}>()

const emit = defineEmits<{
  editFile: [remotePath: string, connId: string]
}>()

const terminalStore = useTerminalStore()

const toolMeta: Record<ToolTab, { icon: string }> = {
  sftp: { icon: '\uD83D\uDCC2' },
  tunnel: { icon: '\uD83D\uDD0C' },
  proxy: { icon: '\uD83C\uDF10' },
  snippet: { icon: '\u26A1' },
  trigger: { icon: '\uD83D\uDD2B' },
  key: { icon: '\uD83D\uDD11' },
  knownHosts: { icon: '\uD83D\uDDC2' },
}

const openToolTabs = () => props.tab.openToolTabs ?? []
const activeToolTab = () => props.tab.activeToolTab ?? 'sftp'

function onCloseTab(e: MouseEvent, tool: ToolTab) {
  e.stopPropagation()
  terminalStore.closeToolTab(props.tabId, tool)
}
</script>

<template>
  <div class="tool-panel">
    <div class="tool-tabs">
      <button
        v-for="t in openToolTabs()"
        :key="t"
        class="tool-tab"
        :class="{ active: activeToolTab() === t }"
        @click="terminalStore.setActiveToolTab(tabId, t)"
      >
        <span class="tab-icon">{{ toolMeta[t].icon }}</span>
        <span class="tab-label">{{ $t('tool_panel.' + t) }}</span>
        <span class="tab-close" @click="(e) => onCloseTab(e, t)">{{ '\u2715' }}</span>
      </button>
    </div>
    <div class="tool-body">
      <SftpPanel
        v-if="activeToolTab() === 'sftp'"
        :tab-id="tabId"
        :tab="tab"
        @edit-file="(p, c) => emit('editFile', p, c)"
        @close="terminalStore.closeToolTab(tabId, 'sftp')"
      />
      <TunnelPanel v-if="activeToolTab() === 'tunnel'" />
      <ProxyPanel v-if="activeToolTab() === 'proxy'" />
      <SnippetPanel v-if="activeToolTab() === 'snippet'" />
      <TriggerPanel v-if="activeToolTab() === 'trigger'" />
      <KeyManagerPanel v-if="activeToolTab() === 'key'" />
      <KnownHostsPanel v-if="activeToolTab() === 'knownHosts'" />
    </div>
  </div>
</template>

<style scoped>
.tool-panel {
  min-width: 200px;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  height: 100%;
}

.tool-tabs {
  display: flex;
  overflow-x: auto;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  flex-shrink: 0;
}

.tool-tab {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 10px;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  font-size: 11px;
  white-space: nowrap;
  border-bottom: 2px solid transparent;
}
.tool-tab:hover {
  background: var(--bg-base);
  color: var(--text);
}
.tool-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  background: var(--bg-base);
}

.tab-icon {
  font-size: 13px;
}

.tab-label {
  font-size: 11px;
}

.tab-close {
  font-size: 10px;
  color: var(--text-overlay0);
  padding: 1px 2px;
  border-radius: 2px;
  line-height: 1;
  margin-left: 2px;
}
.tab-close:hover {
  color: var(--danger);
  background: var(--bg-surface0);
}

.tool-body {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.tool-body :deep(.panel-header) {
  display: none;
}
</style>
