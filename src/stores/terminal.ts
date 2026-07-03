import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { TerminalTab, TerminalSession, SshConnectionInfo } from '../types'

let nextId = 1
function generateId(): string {
  return `tab-${nextId++}`
}

export const useTerminalStore = defineStore('terminal', () => {
  const tabs = ref<TerminalTab[]>([])
  const activeTabId = ref<string | null>(null)

  const activeTab = computed(() => {
    if (!activeTabId.value) return null
    return tabs.value.find(t => t.id === activeTabId.value) ?? null
  })

  function addTab(type: TerminalSession['type'] = 'local', sshInfo?: SshConnectionInfo) {
    const id = generateId()
    const title = type === 'local' ? 'Local' : type.toUpperCase()
    const tab: TerminalTab = {
      id,
      title,
      session: {
        id: `session-${id}`,
        title,
        type,
        status: 'connecting',
      },
      sshInfo,
    }
    tabs.value.push(tab)
    activeTabId.value = id
    return tab
  }

  function closeTab(id: string) {
    const idx = tabs.value.findIndex(t => t.id === id)
    if (idx === -1) return

    tabs.value.splice(idx, 1)

    if (activeTabId.value === id) {
      if (tabs.value.length > 0) {
        activeTabId.value = tabs.value[Math.min(idx, tabs.value.length - 1)].id
      } else {
        activeTabId.value = null
      }
    }
  }

  function setActiveTab(id: string) {
    activeTabId.value = id
  }

  function updateTabTitle(id: string, title: string) {
    const tab = tabs.value.find(t => t.id === id)
    if (tab) {
      tab.title = title
      if (tab.session) {
        tab.session.title = title
      }
    }
  }

  function updateSessionStatus(id: string, status: TerminalSession['status']) {
    const tab = tabs.value.find(t => t.id === id)
    if (tab?.session) {
      tab.session.status = status
    }
  }

  return {
    tabs,
    activeTabId,
    activeTab,
    addTab,
    closeTab,
    setActiveTab,
    updateTabTitle,
    updateSessionStatus,
  }
})
