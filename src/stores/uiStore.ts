import { defineStore } from 'pinia'
import { ref } from 'vue'

export type ToolTab = 'sftp' | 'tunnel' | 'proxy' | 'snippet' | 'trigger' | 'key' | 'knownHosts'

export const useUiStore = defineStore('ui', () => {
  const leftSidebar = ref(false)
  const rightSidebar = ref(false)
  const activeToolTab = ref<ToolTab>('sftp')
  const quickConnect = ref(false)
  const sshDialog = ref(false)
  const settingsDialog = ref(false)
  const leftSidebarPct = ref(18)
  const rightSidebarPct = ref(28)

  return {
    leftSidebar,
    rightSidebar,
    activeToolTab,
    quickConnect,
    sshDialog,
    settingsDialog,
    leftSidebarPct,
    rightSidebarPct,
  }
})
