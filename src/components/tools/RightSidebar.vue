<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTerminalStore } from '../../stores/terminal'
import { useUiStore } from '../../stores/uiStore'
import { getAiConversation } from '../../hooks/terminalAiRegistry'
import AiSidebar from '../ai/AiSidebar.vue'
import CastPanel from './CastPanel.vue'
import FilePanel from '../file/FilePanel.vue'
import MonitorPanel from './MonitorPanel.vue'
import TunnelPanel from '../tunnel/TunnelPanel.vue'
import CommandHistoryPanel from '../history/CommandHistoryPanel.vue'
import type { FileKind, ToolTab } from '../../types'

const emit = defineEmits<{
  editFile: [remotePath: string, connId: string, kind: FileKind]
}>()

const { t } = useI18n()
const store = useTerminalStore()
const ui = useUiStore()

const dragging = ref(false)

const activeTab = computed(() => store.activeTab)

const toolTabs = computed<{ id: ToolTab; icon: string; title: string }[]>(() => {
  const list: { id: ToolTab; icon: string; title: string }[] = []
  const tab = activeTab.value
  list.push({ id: 'ai', icon: '\u{1F916}', title: t('tool_panel.ai') })
  list.push({ id: 'history', icon: '\u{1F552}', title: t('tool_panel.history') })
  if (store.hasCapability(tab, 'file')) {
    list.push({ id: 'file', icon: '\u{1F4C2}', title: store.tabSessionType(tab) === 'adb' ? t('tool_panel.adb_files') : t('tool_panel.sftp') })
  }
  if (store.hasCapability(tab, 'tunnel')) {
    list.push({ id: 'tunnel', icon: '\u{1F50C}', title: t('tool_panel.tunnel') })
  }
  if (store.hasCapability(tab, 'cast')) {
    list.push({ id: 'cast', icon: '\u{1F4FA}', title: t('tool_panel.cast') })
  }
  if (store.hasCapability(tab, 'exec')) {
    list.push({ id: 'monitor', icon: '\u{1F4C8}', title: t('tool_panel.monitor') })
  }
  return list
})

const activeTool = computed<ToolTab>(() => {
  const cur = activeTab.value?.activeToolTab ?? 'ai'
  if (toolTabs.value.some(x => x.id === cur)) return cur
  return toolTabs.value[0]?.id ?? 'ai'
})

const aiConv = computed(() => {
  const tab = activeTab.value
  if (!tab?.aiSessionId) return undefined
  return getAiConversation(tab.aiSessionId)
})

function onDividerDown(e: MouseEvent) {
  dragging.value = true
  const startX = e.clientX
  const startW = ui.rightSidebarWidth
  function onMove(ev: MouseEvent) {
    const delta = startX - ev.clientX
    ui.rightSidebarWidth = Math.min(Math.max(startW + delta, 280), 600)
  }
  function onUp() {
    dragging.value = false
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

function closeSidebar() {
  const id = activeTab.value?.id
  if (id) store.toggleToolSidebar(id)
}
</script>

<template>
  <div class="right-pane" :class="{ dragging }" :style="{ width: ui.rightSidebarWidth + 'px' }">
    <div class="right-divider" @mousedown="onDividerDown" />
    <div class="right-sidebar">
      <div class="right-tabs">
        <div class="tab-scroll">
          <button
            v-for="tt in toolTabs"
            :key="tt.id"
            class="st-tab"
            :class="{ active: activeTool === tt.id }"
            :title="tt.title"
            @click="store.setActiveToolTab(activeTab!.id, tt.id)"
          >
            <span class="tt-icon">{{ tt.icon }}</span>
            <span v-if="activeTool === tt.id" class="tt-title">{{ tt.title }}</span>
          </button>
        </div>
        <button class="st-tab close-btn" :title="t('titlebar.close')" @click="closeSidebar">&#x2715;</button>
      </div>
      <div class="right-body">
        <AiSidebar
          v-if="activeTool === 'ai'"
          :ai-conv="aiConv"
          :tab-title="activeTab?.title ?? ''"
        />
        <CommandHistoryPanel
          v-if="activeTab"
          v-show="activeTool === 'history'"
        />
        <FilePanel
          v-if="activeTab"
          v-show="activeTool === 'file'"
          :tab-id="activeTab.id"
          :tab="activeTab"
          :visible="activeTool === 'file'"
          @edit-file="(p, c, k) => emit('editFile', p, c, k)"
        />
        <TunnelPanel
          v-if="activeTab"
          v-show="activeTool === 'tunnel'"
          :tab-id="activeTab.id"
          :tab="activeTab"
        />
        <CastPanel
          v-if="activeTab"
          v-show="activeTool === 'cast'"
          :tab-id="activeTab.id"
          :tab="activeTab"
        />
        <MonitorPanel
          v-if="activeTab"
          v-show="activeTool === 'monitor'"
          :tab-id="activeTab.id"
          :tab="activeTab"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.right-pane {
  display: flex;
  flex-direction: row;
  overflow: hidden;
  position: relative;
  flex-shrink: 0;
}

.right-pane.dragging {
  user-select: none;
}

.right-divider {
  width: 4px;
  cursor: col-resize;
  background: transparent;
  flex-shrink: 0;
  position: relative;
  z-index: 10;
}

.right-divider:hover {
  background: var(--accent-glass);
}

.right-sidebar {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
}

.right-tabs {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  border-bottom: 1px solid var(--bg-surface0);
  background: var(--bg-base);
  flex-shrink: 0;
}

.tab-scroll {
  display: flex;
  gap: 2px;
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: thin;
}

.st-tab {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 7px 10px;
  border-radius: 4px;
  font-size: 13px;
  line-height: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}

.tt-title {
  font-size: 12px;
  white-space: nowrap;
}

.st-tab:hover {
  background: var(--bg-surface0);
  color: var(--text);
}

.st-tab.active {
  background: var(--bg-surface1);
  color: var(--accent);
}

.close-btn {
  font-size: 12px;
  flex: 0 0 auto;
}

.right-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
