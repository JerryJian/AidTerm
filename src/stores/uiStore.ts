import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useUiStore = defineStore('ui', () => {
  const leftSidebar = ref(false)
  const quickConnect = ref(false)
  const sshDialog = ref(false)
  const serialDialog = ref(false)
  const adbDialog = ref(false)
  const wslDialog = ref(false)
  const settingsDialog = ref(false)
  const settingsTab = ref<'general' | 'ai' | 'proxy'>('general')
  const aboutDialog = ref(false)
  const leftSidebarWidth = ref(280)
  const rightSidebarWidth = ref(380)

  return {
    leftSidebar,
    quickConnect,
    sshDialog,
    serialDialog,
    adbDialog,
    wslDialog,
    settingsDialog,
    settingsTab,
    aboutDialog,
    leftSidebarWidth,
    rightSidebarWidth,
  }
})
