<script setup lang="ts">
import { useUiStore, type ToolTab } from '../../stores/uiStore'
import SftpPanel from '../sftp/SftpPanel.vue'
import TunnelPanel from '../tunnel/TunnelPanel.vue'
import ProxyPanel from '../proxy/ProxyPanel.vue'
import SnippetPanel from '../snippet/SnippetPanel.vue'
import TriggerPanel from '../trigger/TriggerPanel.vue'
import KeyManagerPanel from '../keychain/KeyManagerPanel.vue'
import KnownHostsPanel from '../keychain/KnownHostsPanel.vue'

const ui = useUiStore()

const emit = defineEmits<{
  editFile: [remotePath: string, connId: string]
}>()

const tabs: { id: ToolTab; label: string; icon: string }[] = [
  { id: 'sftp', label: 'SFTP', icon: '\uD83D\uDCC2' },
  { id: 'tunnel', label: 'Tunnel', icon: '\uD83D\uDD0C' },
  { id: 'proxy', label: 'Proxy', icon: '\uD83C\uDF10' },
  { id: 'snippet', label: 'Snippets', icon: '\u26A1' },
  { id: 'trigger', label: 'Triggers', icon: '\uD83D\uDD2B' },
  { id: 'key', label: 'Keys', icon: '\uD83D\uDD11' },
  { id: 'knownHosts', label: 'Hosts', icon: '\uD83D\uDDC2' },
]
</script>

<template>
  <div class="tool-panel">
    <div class="panel-header">
      <span class="panel-title">{{ '\uD83D\uDD27' }} Tools</span>
      <button class="panel-close" @click="ui.rightSidebar = false">{{ '\u2715' }}</button>
    </div>
    <div class="tool-tabs">
      <button
        v-for="t in tabs"
        :key="t.id"
        class="tool-tab"
        :class="{ active: ui.activeToolTab === t.id }"
        @click="ui.activeToolTab = t.id"
        :title="t.label"
      >
        <span class="tool-tab-icon">{{ t.icon }}</span>
        <span class="tool-tab-label">{{ t.label }}</span>
      </button>
    </div>
    <div class="tool-body">
      <SftpPanel
        v-if="ui.activeToolTab === 'sftp'"
        @edit-file="(p, c) => emit('editFile', p, c)"
      />
      <TunnelPanel v-if="ui.activeToolTab === 'tunnel'" />
      <ProxyPanel v-if="ui.activeToolTab === 'proxy'" />
      <SnippetPanel v-if="ui.activeToolTab === 'snippet'" />
      <TriggerPanel v-if="ui.activeToolTab === 'trigger'" />
      <KeyManagerPanel v-if="ui.activeToolTab === 'key'" />
      <KnownHostsPanel v-if="ui.activeToolTab === 'knownHosts'" />
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

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.panel-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-sub0);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.panel-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
}
.panel-close:hover {
  background: var(--bg-surface1);
  color: var(--text);
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
  transition: none;
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

.tool-tab-icon {
  font-size: 13px;
}

.tool-tab-label {
  font-size: 11px;
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
