import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { TerminalTab, TerminalSession, SshConnectionInfo, TelnetConnectionInfo, SerialConnectionInfo, SystemInfo, ToolTab } from '../types'

let nextId = 1
function generateId(): string {
  return `tab-${nextId++}`
}

export const useTerminalStore = defineStore('terminal', () => {
  const tabs = ref<TerminalTab[]>([])
  const activeTabId = ref<string | null>(null)
  const batchMode = ref(false)
  const batchTabIds = ref<Set<string>>(new Set())

  const activeTab = computed(() => {
    if (!activeTabId.value) return null
    return tabs.value.find(t => t.id === activeTabId.value) ?? null
  })

  function addTab(type: TerminalSession['type'] = 'local', sshInfo?: SshConnectionInfo, telnetInfo?: TelnetConnectionInfo, subshell?: string, serialInfo?: SerialConnectionInfo) {
    const id = generateId()
    const title = subshell ? subshell.replace(/\.exe$/, '') : (type === 'local' ? 'Local' : type.toUpperCase())
    const tab: TerminalTab = {
      id,
      title,
      session: {
        id: `session-${id}`,
        title,
        type,
        status: 'connecting',
        subshell,
      },
      sshInfo,
      telnetInfo,
      serialInfo,
    }
    tabs.value.push(tab)
    activeTabId.value = id
    return tab
  }

  function ensureTabTools(tab: TerminalTab) {
    if (!tab.openToolTabs) {
      tab.openToolTabs = []
    }
    if (!tab.activeToolTab) {
      tab.activeToolTab = 'sftp'
    }
  }

  function addToolTab(tabId: string, tool: ToolTab) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (!tab) return
    ensureTabTools(tab)
    if (!tab.openToolTabs!.includes(tool)) {
      tab.openToolTabs!.push(tool)
    }
    tab.activeToolTab = tool
    tab.toolSidebarOpen = true
  }

  function closeToolTab(tabId: string, tool: ToolTab) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (!tab || !tab.openToolTabs) return
    const idx = tab.openToolTabs.indexOf(tool)
    if (idx === -1) return
    tab.openToolTabs.splice(idx, 1)
    if (tab.activeToolTab === tool) {
      if (tab.openToolTabs.length > 0) {
        const nextIdx = Math.min(idx, tab.openToolTabs.length - 1)
        tab.activeToolTab = tab.openToolTabs[nextIdx]
      } else {
        tab.toolSidebarOpen = false
      }
    }
  }

  function setActiveToolTab(tabId: string, tool: ToolTab) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (!tab) return
    ensureTabTools(tab)
    tab.activeToolTab = tool
    tab.toolSidebarOpen = true
  }

  function toggleToolSidebar(tabId: string) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (!tab) return
    ensureTabTools(tab)
    tab.toolSidebarOpen = !tab.toolSidebarOpen
  }

  function isToolOpen(tabId: string, tool: ToolTab): boolean {
    const tab = tabs.value.find(t => t.id === tabId)
    return !!tab?.openToolTabs?.includes(tool)
  }

  function toggleAiSidebar(tabId: string) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (tab) {
      tab.aiSidebarOpen = !tab.aiSidebarOpen
    }
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

  function updateSessionId(tabId: string, sessionId: string) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (tab?.session) {
      tab.session.id = sessionId
    }
  }

  function updateSystemInfo(tabId: string, info: SystemInfo) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (tab) {
      tab.systemInfo = info
    }
  }

  function toggleBatch() {
    batchMode.value = !batchMode.value
    if (!batchMode.value) {
      batchTabIds.value = new Set()
    }
  }

  function setBatchTabId(tabId: string, selected: boolean) {
    const s = new Set(batchTabIds.value)
    if (selected) s.add(tabId)
    else s.delete(tabId)
    batchTabIds.value = s
  }

  function getBatchSessionIds(): string[] {
    return tabs.value
      .filter(t => batchTabIds.value.has(t.id) && t.session?.id)
      .map(t => t.session!.id)
  }

  return {
    tabs,
    activeTabId,
    activeTab,
    batchMode,
    batchTabIds,
    addTab,
    closeTab,
    setActiveTab,
    updateTabTitle,
    updateSessionStatus,
    updateSessionId,
    updateSystemInfo,
    toggleBatch,
    setBatchTabId,
    getBatchSessionIds,
    addToolTab,
    closeToolTab,
    setActiveToolTab,
    toggleToolSidebar,
    isToolOpen,
    toggleAiSidebar,
  }
})
