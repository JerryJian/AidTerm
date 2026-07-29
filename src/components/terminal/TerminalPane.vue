<script setup lang="ts">
import { ref, watch } from 'vue'
import TerminalWrapper from './TerminalWrapper.vue'
import ToolPanel from '../tools/ToolPanel.vue'
import AiSidebar from '../ai/AiSidebar.vue'
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
const aiWidth = ref(380)
const draggingTool = ref(false)
const draggingAi = ref(false)

const activeSplitChildId = ref<string | null>(null)

function setActiveSplitChild(id: string) {
  activeSplitChildId.value = id
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

function onAiDividerDown(e: MouseEvent) {
  draggingAi.value = true
  const startX = e.clientX
  const startW = aiWidth.value
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    aiWidth.value = Math.min(Math.max(startW + delta, 280), 600)
  }
  function onUp() {
    draggingAi.value = false
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
  <div class="terminal-pane-root" :class="{ dragging: draggingTool || draggingAi }">
    <div class="split-container" :class="tab.splitDirection === 'horizontal' ? 'split-row' : 'split-col'">
      <div
        class="split-child"
        :class="{ 'active-split': activeSplitChildId === tab.id }"
        @mousedown="setActiveSplitChild(tab.id)"
      >
        <TerminalWrapper
          ref="termRef"
          :tab="tab"
          @newSsh="$emit('newSsh')"
          @split-tab="(id, dir) => $emit('splitTab', id, dir)"
        />
      </div>
      <div
        v-for="child in tab.children"
        :key="child.id"
        class="split-child"
        :class="{ 'active-split': activeSplitChildId === child.id }"
        @mousedown="setActiveSplitChild(child.id)"
      >
        <TerminalPane
          :tab="child"
          @newSsh="$emit('newSsh')"
          @edit-file="(p, c) => $emit('editFile', p, c)"
          @split-tab="(id, dir) => $emit('splitTab', id, dir)"
        />
      </div>
    </div>
    <div v-if="tab.aiSidebarOpen && termRef?.aiConv" class="ai-pane" :style="{ width: aiWidth + 'px' }">
      <div class="ai-divider" @mousedown="onAiDividerDown" />
      <div class="ai-pane-body">
        <AiSidebar
          :ai-conv="termRef.aiConv"
          @close="terminalStore.toggleAiSidebar(tab.id)"
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
  overflow: hidden;
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
  overflow: hidden;
  position: relative;
}

.split-child.active-split {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
  z-index: 1;
}

.split-container.split-row > .split-child + .split-child {
  border-left: 1px solid var(--border-color, #444);
}

.split-container.split-col > .split-child + .split-child {
  border-top: 1px solid var(--border-color, #444);
}

.ai-pane {
  display: flex;
  flex-direction: row;
  overflow: hidden;
  position: relative;
}

.ai-divider {
  width: 4px;
  cursor: col-resize;
  background: transparent;
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.ai-divider:hover {
  background: var(--accent-glass);
}

.ai-pane-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
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
