<script setup lang="ts">
import TerminalWrapper from './TerminalWrapper.vue'
import ToolPanel from '../tools/ToolPanel.vue'
import type { TerminalTab } from '../../types'

defineProps<{
  tab: TerminalTab
}>()

defineEmits<{
  newSsh: []
  editFile: [remotePath: string, connId: string]
}>()
</script>

<template>
  <div class="terminal-pane-root">
    <div class="terminal-pane">
      <TerminalWrapper :ssh-info="tab.sshInfo" :telnet-info="tab.telnetInfo" @newSsh="$emit('newSsh')" />
    </div>
    <div v-if="tab.toolSidebarOpen" class="tool-pane">
      <ToolPanel
        :tab-id="tab.id"
        :tab="tab"
        @edit-file="(p, c) => $emit('editFile', p, c)"
      />
    </div>
  </div>
</template>

<style scoped>
.terminal-pane-root {
  flex: 1;
  display: flex;
  flex-direction: row;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.terminal-pane {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.tool-pane {
  width: 340px;
  min-width: 280px;
  max-width: 50%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-left: 1px solid var(--bg-surface0);
}
</style>
