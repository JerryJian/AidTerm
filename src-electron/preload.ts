import { contextBridge, ipcRenderer } from 'electron'

contextBridge.exposeInMainWorld('electronAPI', {
  /**
   * Generic IPC invoke — mirrors Tauri's invoke(cmd, args) pattern.
   * Converts { key: val } args object into positional array for Electron IPC.
   */
  invoke(channel: string, args?: Record<string, any>) {
    // Tauri sends args as a single object; Electron expects positional args.
    // We flatten common arg shapes to match the main process handlers.
    if (!args || Object.keys(args).length === 0) {
      return ipcRenderer.invoke(channel)
    }
    return ipcRenderer.invoke(channel, args)
  },

  /**
   * Event listener — mirrors Tauri's listen(event, handler).
   * Tauri events are sent via webContents.send from main process.
   */
  on(channel: string, listener: (payload: any) => void) {
    const handler = (_event: any, payload: any) => listener(payload)
    ipcRenderer.on(channel, handler)
    return () => {
      ipcRenderer.removeListener(channel, handler)
    }
  },

  // ── Dialog ──
  openDialog: (opts: any) => ipcRenderer.invoke('dialog:open', opts),
  saveDialog: (opts: any) => ipcRenderer.invoke('dialog:save', opts),

  // ── Clipboard ──
  clipboardWrite: (text: string) => ipcRenderer.invoke('clipboard:write', text),
  clipboardRead: () => ipcRenderer.invoke('clipboard:read'),

  // ── Window ──
  window: {
    isFullscreen: () => ipcRenderer.invoke('window:isFullscreen'),
    setFullscreen: (fullscreen: boolean) => ipcRenderer.invoke('window:setFullscreen', { fullscreen }),
    isMaximized: () => ipcRenderer.invoke('window:isMaximized'),
    minimize: () => ipcRenderer.invoke('window:minimize'),
    maximize: () => ipcRenderer.invoke('window:maximize'),
    unmaximize: () => ipcRenderer.invoke('window:unmaximize'),
    toggleMaximize: () => ipcRenderer.invoke('window:toggleMaximize'),
    startDragging: () => ipcRenderer.invoke('window:startDragging'),
    show: () => ipcRenderer.invoke('window:show'),
    hide: () => ipcRenderer.invoke('window:hide'),
    close: () => ipcRenderer.invoke('window:close'),
    openDevtools: () => ipcRenderer.invoke('window:openDevtools'),
    getPlatform: () => ipcRenderer.invoke('get_platform'),
  },
})
