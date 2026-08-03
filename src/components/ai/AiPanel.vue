<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTerminalStore } from '../../stores/terminal'
import { useUiStore } from '../../stores/uiStore'
import { getAiConversation } from '../../hooks/terminalAiRegistry'
import AiSidebar from './AiSidebar.vue'

const { t } = useI18n()
const emit = defineEmits<{
  close: []
}>()

const store = useTerminalStore()
const ui = useUiStore()

const draggingAi = ref(false)

const aiConv = computed(() => {
  const id = store.activeLeafId
  return id ? getAiConversation(id) : undefined
})

function onAiDividerDown(e: MouseEvent) {
  draggingAi.value = true
  const startX = e.clientX
  const startW = ui.aiSidebarWidth
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    ui.aiSidebarWidth = Math.min(Math.max(startW + delta, 280), 600)
  }
  function onUp() {
    draggingAi.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}
</script>

<template>
  <div class="ai-pane" :class="{ dragging: draggingAi }" :style="{ width: ui.aiSidebarWidth + 'px' }">
    <div class="ai-divider" @mousedown="onAiDividerDown" />
    <div class="ai-pane-body">
      <AiSidebar v-if="aiConv" :ai-conv="aiConv" @close="emit('close')" />
      <div v-else class="ai-pane-empty">{{ t('ai.no_active_terminal') }}</div>
    </div>
  </div>
</template>

<style scoped>
.ai-pane {
  display: flex;
  flex-direction: row;
  overflow: hidden;
  position: relative;
  flex-shrink: 0;
}

.ai-pane.dragging {
  user-select: none;
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

.ai-pane-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-overlay0);
  font-size: 13px;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
}
</style>
