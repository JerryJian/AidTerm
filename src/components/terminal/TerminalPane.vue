<script setup lang="ts">
import { ref, watch } from 'vue'
import TerminalWrapper from './TerminalWrapper.vue'
import TerminalPaneChild from './TerminalPane.vue'
import ToolPanel from '../tools/ToolPanel.vue'
import AiSidebar from '../ai/AiSidebar.vue'
import { useTerminalStore } from '../../stores/terminal'
import { saveDialog, invoke } from '@/api'
import type { TerminalTab } from '../../types'

const props = defineProps<{
  tab: TerminalTab
}>()

defineEmits<{
  newSsh: []
  editFile: [remotePath: string, connId: string]
}>()

const terminalStore = useTerminalStore()
const termRef = ref<InstanceType<typeof TerminalWrapper>>()
const toolWidth = ref(400)
const aiWidth = ref(380)
const dragging = ref(false)
const draggingAi = ref(false)

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

function onAiDividerDown(e: MouseEvent) {
  draggingAi.value = true
  const startX = e.clientX
  const startW = aiWidth.value
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    const newW = Math.min(Math.max(startW + delta, 280), 600)
    aiWidth.value = newW
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
  if (req && req.tabId === props.tab.id) {
    doExport()
    terminalStore.clearExportRequest()
  }
})
</script>

<template>
  <div class="terminal-pane-root" :class="{ dragging: dragging || draggingAi }" :style="{ flexDirection: tab.splitDirection === 'horizontal' ? 'row' : 'column' }">
    <div class="terminal-pane" :style="{ flex: tab.children?.length ? '1 1 0' : '1' }">
      <TerminalWrapper
        ref="termRef"
        :ssh-info="tab.sshInfo"
        :telnet-info="tab.telnetInfo"
        :serial-info="tab.serialInfo"
        :ai-session-id="tab.aiSessionId"
        @newSsh="$emit('newSsh')"
      />
    </div>
    <template v-if="tab.children?.length">
      <div v-for="child in tab.children" :key="child.id" class="terminal-pane" style="flex: 1 1 0">
        <TerminalPaneChild :tab="child" @newSsh="$emit('newSsh')" @edit-file="(p, c) => $emit('editFile', p, c)" />
      </div>
    </template>
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
