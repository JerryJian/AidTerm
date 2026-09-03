import { contextBridge, ipcRenderer, webUtils } from 'electron'

contextBridge.exposeInMainWorld('electronAPI', {
  /**
   * Resolve the real filesystem path of a File dropped onto the webview.
   * Electron hides raw paths in the renderer; this uses webUtils.getPathForFile.
   * Returns null when the given object is not a File with a resolvable path.
   */
  getPathForFile(file: unknown): string | null {
    try {
      if (!(file instanceof File)) return null
      const p = webUtils.getPathForFile(file as File)
      return typeof p === 'string' && p ? p : (file as File & { path?: string }).path ?? null
    } catch {
      return null
    }
  },

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

  /**
   * Open a MessageChannel push channel for a cast frame stream. The main
   * process creates a MessageChannelMain, registers the main-side port as the
   * cast sink, and transfers port2 here. Every demuxed frame arrives on the
   * port as a `{ type: 'frame', seq, key, data: ArrayBuffer, config }` (or a
   * `{ type: 'disconnect' }` on stream end) and is forwarded to `onMessage`.
   * Returns an unsubscribe function that closes the channel.
   */
  castOpenPush(serial: string, onMessage: (msg: any) => void) {
    const channel = `cast-stream-port:${serial}`
    let port: MessagePort | null = null
    const handler = (event: any) => {
      const [p] = (event.ports as MessagePort[]) ?? []
      if (!p) return
      ipcRenderer.removeListener(channel, handler)
      port = p
      p.onmessage = (e: MessageEvent) => onMessage(e.data)
      p.start()
    }
    ipcRenderer.on(channel, handler)
    ipcRenderer.send('cast_stream_port', { serial })
    return () => {
      ipcRenderer.removeListener(channel, handler)
      if (port) {
        port.onmessage = null
        port.close()
      }
      ipcRenderer.send('cast_stream_close', { serial })
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
