<script setup lang="ts">
import { ref } from 'vue'
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

const toolWidth = ref(400)
const dragging = ref(false)

function onDividerDown(e: MouseEvent) {
  dragging.value = true
  const startX = e.clientX
  const startW = toolWidth.value
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    const newW = Math.min(Math.max(startW + delta, 260), 600)
    toolWidth.value = newW
  }
  function onUp() {
    dragging.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}
</script>

<template>
  <div class="terminal-pane-root" :class="{ dragging }">
    <div class="terminal-pane">
      <TerminalWrapper :ssh-info="tab.sshInfo" :telnet-info="tab.telnetInfo" :serial-info="tab.serialInfo" @newSsh="$emit('newSsh')" />
    </div>
    <div v-if="tab.toolSidebarOpen" class="tool-pane" :style="{ width: toolWidth + 'px' }">
      <div class="divider" @mousedown="onDividerDown" />
      <div class="tool-pane-body">
        <ToolPanel
          :tab-id="tab.id"
          :tab="tab"
          @edit-file="(p, c) => $emit('editFile', p, c)"
        />
      </div>
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

.terminal-pane-root.dragging {
  user-select: none;
}

.terminal-pane {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}

.tool-pane {
  display: flex;
  flex-direction: row;
  overflow: hidden;
  position: relative;
}

.divider {
  width: 4px;
  cursor: col-resize;
  background: transparent;
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.divider:hover {
  background: var(--accent-glass);
}

.tool-pane-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
