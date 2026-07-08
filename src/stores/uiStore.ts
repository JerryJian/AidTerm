import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useUiStore = defineStore('ui', () => {
  const leftSidebar = ref(false)
  const quickConnect = ref(false)
  const sshDialog = ref(false)
  const settingsDialog = ref(false)
  const leftSidebarPct = ref(18)

  return {
    leftSidebar,
    quickConnect,
    sshDialog,
    settingsDialog,
    leftSidebarPct,
  }
})
