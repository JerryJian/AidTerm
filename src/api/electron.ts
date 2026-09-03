/**
 * Electron implementation — bridges to the preload-exposed electronAPI.
 */

import type { UnlistenFn, ListenEvent, WindowHandle, ResizeDirection, DialogOptions } from './types'

export type { UnlistenFn }

interface ElectronAPI {
  invoke(channel: string, args?: Record<string, unknown>): Promise<unknown>
  on<T>(channel: string, listener: (payload: T) => void): () => void
  getPathForFile(file: File): string | null
  openDialog(opts?: DialogOptions): Promise<null | string | string[]>
  saveDialog(opts?: DialogOptions): Promise<string | null>
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
  const api = (window as unknown as { electronAPI?: ElectronAPI }).electronAPI
  if (!api) throw new Error('electronAPI not available — are you running in Electron?')
  return api
}

/**
 * invoke — mirrors Tauri's invoke(cmd, args).
 * Handles special Tauri command names that map to different Electron channels.
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const api = getApi()

  // Map Tauri clipboard commands to Electron
  if (cmd === 'plugin:clipboard-manager|write_text') {
    await api.clipboardWrite(String(args?.text ?? ''))
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
  return api.invoke(cmd, args) as Promise<T>
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
export function openDialog(opts?: DialogOptions): Promise<null | string | string[]> {
  return getApi().openDialog(opts)
}

/**
 * saveDialog — wraps Electron's dialog.showSaveDialog.
 * Compatible with @tauri-apps/plugin-dialog's save() signature.
 */
export function saveDialog(opts?: DialogOptions): Promise<string | null> {
  return getApi().saveDialog(opts)
}

export function clipboardWrite(text: string): Promise<void> {
  return getApi().clipboardWrite(text)
}

export function clipboardRead(): Promise<string> {
  return getApi().clipboardRead()
}

export async function toFileUrl(filePath: string): Promise<string> {
  return getApi().invoke('file_to_data_url', { path: filePath }) as Promise<string>
}
