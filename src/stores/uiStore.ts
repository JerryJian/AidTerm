import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useUiStore = defineStore('ui', () => {
  const leftSidebar = ref(false)
  const quickConnect = ref(false)
  const sshDialog = ref(false)
  const serialDialog = ref(false)
  const settingsDialog = ref(false)
  const aboutDialog = ref(false)
  const leftSidebarWidth = ref(280)
  const aiSidebarOpen = ref(false)
  const aiSidebarWidth = ref(380)

  return {
    leftSidebar,
    quickConnect,
    sshDialog,
    serialDialog,
    settingsDialog,
    aboutDialog,
    leftSidebarWidth,
    aiSidebarOpen,
    aiSidebarWidth,
  }
})
