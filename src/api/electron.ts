/**
 * Electron implementation — bridges to the preload-exposed electronAPI.
 */

import type { UnlistenFn, ListenEvent, WindowHandle, ResizeDirection } from './types'

export type { UnlistenFn }

interface ElectronAPI {
  invoke(channel: string, args?: Record<string, any>): Promise<any>
  on(channel: string, listener: (payload: any) => void): () => void
  openDialog(opts: any): Promise<any>
  saveDialog(opts: any): Promise<any>
  clipboardWrite(text: string): Promise<void>
  clipboardRead(): Promise<string>
  window: {
    isFullscreen(): Promise<boolean>
    setFullscreen(fullscreen: boolean): Promise<void>
    isMaximized(): Promise<boolean>
    minimize(): Promise<void>
    maximize(): Promise<void>
    unmaximize(): Promise<void>
    toggleMaximize(): Promise<void>
    startDragging(): Promise<void>
    startResizeDragging(direction: ResizeDirection): Promise<void>
    show(): Promise<void>
    hide(): Promise<void>
    close(): Promise<void>
    openDevtools(): Promise<void>
    getPlatform(): Promise<string>
  }
}

function getApi(): ElectronAPI {
  const api = (window as any).electronAPI as ElectronAPI | undefined
  if (!api) throw new Error('electronAPI not available — are you running in Electron?')
  return api
}

/**
 * invoke — mirrors Tauri's invoke(cmd, args).
 * Handles special Tauri command names that map to different Electron channels.
 */
export async function invoke<T>(cmd: string, args?: Record<string, any>): Promise<T> {
  const api = getApi()

  // Map Tauri clipboard commands to Electron
  if (cmd === 'plugin:clipboard-manager|write_text') {
    await api.clipboardWrite(args?.text ?? '')
    return undefined as T
  }
  if (cmd === 'plugin:clipboard-manager|read_text') {
    return api.clipboardRead() as T
  }

  // Map Tauri window commands to Electron
  if (cmd === 'open_devtools') {
    await api.window.openDevtools()
    return undefined as T
  }

  // Default: forward to Electron main process
  return api.invoke(cmd, args)
}

export function listen<T>(event: string, handler: (event: ListenEvent<T>) => void): Promise<UnlistenFn> {
  return Promise.resolve(getApi().on(event, (payload: T) => {
    handler({ payload })
  }))
}

export function getCurrentWindow(): WindowHandle {
  const api = getApi()
  return {
    isFullscreen: () => api.window.isFullscreen(),
    setFullscreen: (f) => api.window.setFullscreen(f),
    isMaximized: () => api.window.isMaximized(),
    onResized: (cb) => Promise.resolve(api.on('window:resized', () => cb())),
    minimize: () => api.window.minimize(),
    maximize: () => api.window.maximize(),
    unmaximize: () => api.window.unmaximize(),
    toggleMaximize: () => api.window.toggleMaximize(),
    startDragging: () => api.window.startDragging(),
    startResizeDragging: () => Promise.resolve(),
    show: () => api.window.show(),
    hide: () => api.window.hide(),
    close: () => api.window.close(),
  }
}

/**
 * openDialog — wraps Electron's dialog.showOpenDialog.
 * Compatible with @tauri-apps/plugin-dialog's open() signature.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function openDialog(opts?: any): Promise<any> {
  return getApi().openDialog(opts)
}

/**
 * saveDialog — wraps Electron's dialog.showSaveDialog.
 * Compatible with @tauri-apps/plugin-dialog's save() signature.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function saveDialog(opts?: any): Promise<any> {
  return getApi().saveDialog(opts)
}

export function clipboardWrite(text: string): Promise<void> {
  return getApi().clipboardWrite(text)
}

export function clipboardRead(): Promise<string> {
  return getApi().clipboardRead()
}

export async function toFileUrl(filePath: string): Promise<string> {
  return getApi().invoke('file_to_data_url', { path: filePath })
}
