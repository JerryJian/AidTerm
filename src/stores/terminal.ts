import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useFileStore } from './fileStore'
import { useSettingsStore } from './settingsStore'
import { invoke } from '@/api'
import type { TerminalTab, TerminalSession, SshConnectionInfo, TelnetConnectionInfo, SerialConnectionInfo, AdbConnectionInfo, WslConnectionInfo, SystemInfo, ToolTab, ConnectionType, ConnectionCapability } from '../types'

let nextId = 1
function generateId(): string {
  return `tab-${nextId++}`
}

let adbServerStarted = false

const shellKeyMap: Record<string, string> = {
  'cmd.exe': 'cmd',
  'powershell.exe': 'powershell',
  'pwsh.exe': 'pwsh',
  'wsl.exe': 'wsl',
  'bash.exe': 'bash',
  'bash': 'bash',
  'zsh': 'zsh',
  'sh': 'sh',
  'fish': 'fish',
  '/bin/bash': 'bash',
  '/usr/bin/bash': 'bash',
  '/bin/zsh': 'zsh',
  '/usr/bin/zsh': 'zsh',
  '/bin/sh': 'sh',
  '/usr/bin/sh': 'sh',
  '/usr/bin/fish': 'fish',
  '/usr/bin/pwsh': 'pwsh',
}

/** Default capabilities per connection type (used before the backend reports them). */
const defaultCapabilities: Record<ConnectionType, ConnectionCapability[]> = {
  local: ['file'],
  wsl: ['file'],
  ssh: ['file', 'tunnel', 'exec', 'zmodem'],
  telnet: [],
  serial: [],
  adb: ['file'],
}

export const useTerminalStore = defineStore('terminal', () => {
  const tabs = ref<TerminalTab[]>([])
  const activeTabId = ref<string | null>(null)
  const selectedPaneId = ref<string | null>(null)
  const selectedPaneByTab = ref<Record<string, string>>({})
  const batchMode = ref(false)
  const batchTabIds = ref<Set<string>>(new Set())
  const { t, te } = useI18n()

  const activeTab = computed(() => {
    if (!activeTabId.value) return null
    return tabs.value.find(t => t.id === activeTabId.value) ?? null
  })

  function isDescendant(tab: TerminalTab, id: string): boolean {
    if (tab.id === id) return true
    if (tab.children?.length) return tab.children.some(c => isDescendant(c, id))
    return false
  }

  function topLevelTabIdOf(id: string): string | null {
    for (const root of tabs.value) {
      if (isDescendant(root, id)) return root.id
    }
    return null
  }

  function resolveSessionTab(tab: TerminalTab | null | undefined): TerminalTab | null {
    if (!tab) return null
    if (tab.session) return tab
    if (tab.children?.length) {
      for (const c of tab.children) {
        const found = resolveSessionTab(c)
        if (found) return found
      }
    }
    return null
  }

  function tabSessionType(tab: TerminalTab | null | undefined): TerminalSession['type'] | null {
    return resolveSessionTab(tab)?.session?.type ?? null
  }

  /** Capabilities of the tab's session (backend-reported, falling back to per-type defaults). */
  function tabCapabilities(tab: TerminalTab | null | undefined): ConnectionCapability[] {
    const s = resolveSessionTab(tab)?.session
    if (!s) return []
    return s.capabilities?.length ? s.capabilities : defaultCapabilities[s.type] ?? []
  }

  function hasCapability(tab: TerminalTab | null | undefined, cap: ConnectionCapability): boolean {
    return tabCapabilities(tab).includes(cap)
  }

  function leafIdOf(tab: TerminalTab): string | null {
    if (tab.children?.length) {
      const rootId = topLevelTabIdOf(tab.id)
      const remembered = rootId ? selectedPaneByTab.value[rootId] : null
      const sel = remembered && isDescendant(tab, remembered) ? remembered : null
      const next = (sel
        ? tab.children.find(c => isDescendant(c, sel))
        : null)
        ?? tab.children.find(c => c.session)
        ?? tab.children[0]
      return leafIdOf(next)
    }
    return tab.id
  }

  const activeLeafId = computed<string | null>(() => {
    if (!activeTab.value) return null
    return leafIdOf(activeTab.value)
  })

  function syncSelectedPane() {
    const tab = activeTab.value
    selectedPaneId.value = tab ? leafIdOf(tab) : null
  }

  function addTab(type: TerminalSession['type'] = 'local', sshInfo?: SshConnectionInfo, telnetInfo?: TelnetConnectionInfo, localCommand?: string, serialInfo?: SerialConnectionInfo, workingDir?: string, titleOverride?: string, adbInfo?: AdbConnectionInfo, wslInfo?: WslConnectionInfo) {
    const id = generateId()
    let title: string
    if (titleOverride) {
      title = titleOverride
    } else if (type === 'ssh') {
      title = sshInfo ? `${sshInfo.username || 'ssh'}@${sshInfo.host}` : 'SSH'
    } else if (type === 'telnet') {
      title = telnetInfo ? `Telnet ${telnetInfo.host}` : 'Telnet'
    } else if (type === 'serial') {
      title = serialInfo ? `Serial ${serialInfo.portName}` : 'Serial'
    } else if (type === 'adb') {
      title = adbInfo ? `ADB ${adbInfo.serial}` : 'ADB'
    } else if (type === 'wsl') {
      title = 'WSL'
    } else if (localCommand) {
      const base = localCommand.replace(/\\/g, '/').split('/').pop() || localCommand
      const key = shellKeyMap[localCommand] || base.replace(/\.exe$/, '')
      title = te(`shell.${key}`) ? t(`shell.${key}`) : base
    } else {
      title = t('menu.local_shell')
    }
    if (type === 'adb') {
      adbServerStarted = true
    }
    const tab: TerminalTab = {
      id,
      title,
      session: {
        id: `session-${id}`,
        title,
        type,
        status: 'connecting',
        command: localCommand,
        workingDir,
        capabilities: defaultCapabilities[type],
      },
      sshInfo,
      telnetInfo,
      serialInfo,
      adbInfo,
      wslInfo,
      aiSessionId: `ai-${id}`,
    }
    tabs.value.push(tab)
    activeTabId.value = id
    syncSelectedPane()
    return tab
  }

  function ensureTabTools(tab: TerminalTab) {
    if (!tab.openToolTabs) {
      tab.openToolTabs = []
    }
    if (!tab.activeToolTab) {
      tab.activeToolTab = 'ai'
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

  function removeToolTab(tabId: string, tool: ToolTab) {
    const tab = tabs.value.find(t => t.id === tabId)
    if (!tab?.openToolTabs) return
    tab.openToolTabs = tab.openToolTabs.filter(t => t !== tool)
    if (tab.activeToolTab === tool) {
      tab.activeToolTab = tab.openToolTabs[0] || undefined
      if (!tab.activeToolTab) tab.toolSidebarOpen = false
    }
  }

  const exportRequest = ref<{ tabId: string } | null>(null)
  function requestExport(tabId: string) {
    exportRequest.value = { tabId }
  }
  function clearExportRequest() {
    exportRequest.value = null
  }

  function findTab(id: string, list: TerminalTab[] = tabs.value): TerminalTab | null {
    for (const tab of list) {
      if (tab.id === id) return tab
      if (tab.children?.length) {
        const found = findTab(id, tab.children)
        if (found) return found
      }
    }
    return null
  }

  function findParent(id: string, list: TerminalTab[] = tabs.value): { parent: TerminalTab[]; index: number } | null {
    for (let i = 0; i < list.length; i++) {
      if (list[i].id === id) return { parent: list, index: i }
      if (list[i].children?.length) {
        const found = findParent(id, list[i].children)
        if (found) return found
      }
    }
    return null
  }

  function removeChildFromParent(id: string, list: TerminalTab[]): boolean {
    const idx = list.findIndex(t => t.id === id)
    if (idx !== -1) {
      list.splice(idx, 1)
      return true
    }
    for (const tab of list) {
      if (tab.children?.length && removeChildFromParent(id, tab.children)) {
        return true
      }
    }
    return false
  }

  function hasAnyAdbTab(): boolean {
    const scan = (list: TerminalTab[]): boolean => {
      for (const tab of list) {
        if (tab.adbInfo) return true
        if (tab.children?.length && scan(tab.children)) return true
      }
      return false
    }
    return scan(tabs.value)
  }

  function disposeTabResources(tabId: string) {
    const file = useFileStore()
    if (file.connId(tabId)) {
      file.disconnect(tabId).catch(() => {})
    }
    // When the last adb tab closes, tear down the isolated 5038 server
    // (unless the user disabled auto-cleanup in settings).
    const settings = useSettingsStore()
    if (adbServerStarted && settings.adbAutoKill && !hasAnyAdbTab()) {
      invoke('adb_kill_server').catch(() => {})
    }
  }

  function closeTab(id: string) {
    const tab = findTab(id)
    if (!tab) return

    const isTopLevel = tabs.value.some(t => t.id === id)

    if (tab.children?.length) {
      for (const child of [...tab.children]) {
        closeTab(child.id)
      }
    }

    removeChildFromParent(id, tabs.value)

    if (isTopLevel) {
      disposeTabResources(id)
      delete selectedPaneByTab.value[id]
    }

    if (activeTabId.value === id) {
      if (tabs.value.length > 0) {
        activeTabId.value = tabs.value[0].id
      } else {
        activeTabId.value = null
      }
    }
    syncSelectedPane()
  }

  function closeOtherTabs(id: string) {
    const tab = findTab(id)
    if (!tab) return
    const closed = tabs.value.filter(t => t.id !== id)
    tabs.value = tabs.value.filter(t => t.id === id)
    closed.forEach(t => disposeTabResources(t.id))
    closed.forEach(t => delete selectedPaneByTab.value[t.id])
    activeTabId.value = id
    syncSelectedPane()
  }

  function closeTabsToRight(id: string) {
    const idx = tabs.value.findIndex(t => t.id === id)
    if (idx === -1) return
    const closed = tabs.value.slice(idx + 1)
    tabs.value = tabs.value.slice(0, idx + 1)
    closed.forEach(t => disposeTabResources(t.id))
    closed.forEach(t => delete selectedPaneByTab.value[t.id])
    activeTabId.value = id
    syncSelectedPane()
  }

  function setActiveTab(id: string) {
    activeTabId.value = id
    syncSelectedPane()
  }

  function setSelectedPane(id: string | null) {
    selectedPaneId.value = id
    if (id) {
      const rootId = topLevelTabIdOf(id)
      if (rootId) selectedPaneByTab.value[rootId] = id
    }
  }

  function updateTabTitle(id: string, title: string) {
    const tab = findTab(id)
    if (tab) {
      tab.title = title
      if (tab.session) {
        tab.session.title = title
      }
    }
  }

  function updateSessionStatus(id: string, status: TerminalSession['status']) {
    const tab = findTab(id)
    if (tab?.session) {
      tab.session.status = status
    }
  }

  function updateSessionId(tabId: string, sessionId: string) {
    const tab = findTab(tabId)
    if (tab?.session) {
      tab.session.id = sessionId
    }
  }

  function updateSessionCapabilities(tabId: string, capabilities: ConnectionCapability[]) {
    const tab = findTab(tabId)
    if (tab?.session) {
      tab.session.capabilities = capabilities
    }
  }

  function updateSystemInfo(tabId: string, info: SystemInfo) {
    const tab = findTab(tabId)
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
    exportRequest,
    addTab,
    closeTab,
    closeOtherTabs,
    closeTabsToRight,
    findTab,
    findParent,
    topLevelTabIdOf,
    setActiveTab,
    updateTabTitle,
    updateSessionStatus,
    updateSessionId,
    updateSessionCapabilities,
    updateSystemInfo,
    toggleBatch,
    setBatchTabId,
    getBatchSessionIds,
    addToolTab,
    closeToolTab,
    removeToolTab,
    setActiveToolTab,
    toggleToolSidebar,
    isToolOpen,
    resolveSessionTab,
    tabSessionType,
    tabCapabilities,
    hasCapability,
    requestExport,
    clearExportRequest,
    selectedPaneId,
    setSelectedPane,
    activeLeafId,
    leafIdOf,
  }
})
