import { defineStore } from 'pinia'
import { ref } from 'vue'

export type ToolTab = 'sftp' | 'tunnel' | 'proxy' | 'snippet' | 'trigger' | 'key' | 'knownHosts'

export const useUiStore = defineStore('ui', () => {
  const leftSidebar = ref(false)
  const rightSidebar = ref(false)
  const openToolTabs = ref<ToolTab[]>([])
  const activeToolTab = ref<ToolTab>('sftp')
  const quickConnect = ref(false)
  const sshDialog = ref(false)
  const settingsDialog = ref(false)
  const leftSidebarPct = ref(18)
  const rightSidebarPct = ref(28)

  function addToolTab(tab: ToolTab) {
    if (!openToolTabs.value.includes(tab)) {
      openToolTabs.value.push(tab)
    }
    activeToolTab.value = tab
    rightSidebar.value = true
  }

  function closeToolTab(tab: ToolTab) {
    const idx = openToolTabs.value.indexOf(tab)
    if (idx === -1) return
    openToolTabs.value.splice(idx, 1)
    if (activeToolTab.value === tab) {
      if (openToolTabs.value.length > 0) {
        const nextIdx = Math.min(idx, openToolTabs.value.length - 1)
        activeToolTab.value = openToolTabs.value[nextIdx]
      } else {
        rightSidebar.value = false
      }
    }
  }

  return {
    leftSidebar,
    rightSidebar,
    openToolTabs,
    activeToolTab,
    quickConnect,
    sshDialog,
    settingsDialog,
    leftSidebarPct,
    rightSidebarPct,
    addToolTab,
    closeToolTab,
  }
})
