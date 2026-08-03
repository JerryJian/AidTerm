<script setup lang="ts">
import { ref, watch } from 'vue'
import TerminalWrapper from './TerminalWrapper.vue'
import ToolPanel from '../tools/ToolPanel.vue'
import { useTerminalStore } from '../../stores/terminal'
import { saveDialog, invoke } from '@/api'
import type { TerminalTab } from '../../types'

defineProps<{
  tab: TerminalTab
}>()

defineEmits<{
  newSsh: []
  editFile: [remotePath: string, connId: string]
  splitTab: [tabId: string, direction: 'horizontal' | 'vertical']
}>()

const terminalStore = useTerminalStore()
const termRef = ref<InstanceType<typeof TerminalWrapper>>()
const toolWidth = ref(400)
const draggingTool = ref(false)

function isPaneSelected(tabId: string): boolean {
  return terminalStore.selectedPaneId === tabId
}

function onToolDividerDown(e: MouseEvent) {
  draggingTool.value = true
  const startX = e.clientX
  const startW = toolWidth.value
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    toolWidth.value = Math.min(Math.max(startW + delta, 260), 600)
  }
  function onUp() {
    draggingTool.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

async function doExport() {
  if (!termRef.value) return
  const content = termRef.value.getTerminalContent()
  if (!content) return
  try {
    const path = await saveDialog({ title: 'Export Text', filters: [{ name: 'Text Files', extensions: ['txt', 'log'] }] })
    if (path) {
      await invoke('write_text_file', { path, content })
    }
  } catch {
    // ignore
  }
}

watch(() => terminalStore.exportRequest, (req) => {
  if (req) {
    doExport()
    terminalStore.clearExportRequest()
  }
})
</script>

<template>
  <div class="terminal-pane-root" :class="{ dragging: draggingTool }">
    <div class="split-container" :class="{ 'split-row': tab.splitDirection === 'horizontal', 'split-col': tab.splitDirection === 'vertical' }">
      <template v-if="tab.session">
        <div
          class="split-child"
          :class="{ 'active-split': isPaneSelected(tab.id) }"
          @mousedown.stop="terminalStore.setSelectedPane(tab.id)"
        >
          <TerminalWrapper
            ref="termRef"
            :tab="tab"
            @newSsh="$emit('newSsh')"
            @split-tab="(id, dir) => $emit('splitTab', id, dir)"
          />
        </div>
      </template>
      <div
        v-for="child in tab.children"
        :key="child.id"
        class="split-child"
        :class="{ 'active-split': isPaneSelected(child.id) }"
        @mousedown.stop="terminalStore.setSelectedPane(child.id)"
      >
        <TerminalPane
          :tab="child"
          @newSsh="$emit('newSsh')"
          @edit-file="(p, c) => $emit('editFile', p, c)"
          @split-tab="(id, dir) => $emit('splitTab', id, dir)"
        />
      </div>
    </div>
    <div v-if="tab.toolSidebarOpen" class="tool-pane" :style="{ width: toolWidth + 'px' }">
      <div class="tool-divider" @mousedown="onToolDividerDown" />
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

.split-container {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
}

.split-container.split-row {
  flex-direction: row;
}

.split-container.split-col {
  flex-direction: column;
}

.split-child {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  position: relative;
}

.split-child.active-split::after {
  content: '';
  position: absolute;
  inset: 0;
  border: 2px solid var(--accent);
  pointer-events: none;
  z-index: 10;
}

.split-container.split-row > .split-child + .split-child {
  border-left: 1px solid var(--border-color, #444);
}

.split-container.split-col > .split-child + .split-child {
  border-top: 1px solid var(--border-color, #444);
}

.tool-pane {
  display: flex;
  flex-direction: row;
  overflow: hidden;
  position: relative;
}

.tool-divider {
  width: 4px;
  cursor: col-resize;
  background: transparent;
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.tool-divider:hover {
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
