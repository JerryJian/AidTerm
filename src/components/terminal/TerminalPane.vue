<script setup lang="ts">
import { ref, watch } from 'vue'
import TerminalWrapper from './TerminalWrapper.vue'
import { useTerminalStore } from '../../stores/terminal'
import { saveDialog, invoke } from '@/api'
import type { TerminalTab } from '../../types'

defineProps<{
  tab: TerminalTab
}>()

defineEmits<{
  newSsh: []
  splitTab: [tabId: string, direction: 'horizontal' | 'vertical']
}>()

const terminalStore = useTerminalStore()
const termRef = ref<InstanceType<typeof TerminalWrapper>>()

function isPaneSelected(tabId: string): boolean {
  return terminalStore.selectedPaneId === tabId
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
  <div class="terminal-pane-root">
    <div class="split-container" :class="{ 'split-row': tab.splitDirection === 'horizontal', 'split-col': tab.splitDirection === 'vertical' }">
      <template v-if="tab.session">
        <div
          class="split-child"
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
          @split-tab="(id, dir) => $emit('splitTab', id, dir)"
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
</style>
