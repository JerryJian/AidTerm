/**
 * Tauri implementation — wraps @tauri-apps APIs 1:1.
 */

import { invoke as tauriInvoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen as tauriListen } from '@tauri-apps/api/event'
import { getCurrentWindow as tauriGetCurrentWindow } from '@tauri-apps/api/window'
import { PhysicalPosition, PhysicalSize } from '@tauri-apps/api/dpi'
import { save as tauriSave, open as tauriOpen } from '@tauri-apps/plugin-dialog'

import type { UnlistenFn, ListenEvent, WindowHandle, WindowBounds } from './types'

export { type UnlistenFn }

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args)
}

export function listen<T>(event: string, handler: (event: ListenEvent<T>) => void): Promise<UnlistenFn> {
  return tauriListen<T>(event, handler)
}

export function getCurrentWindow(): WindowHandle {
  const win = tauriGetCurrentWindow()
  return {
    isFullscreen: () => win.isFullscreen(),
    setFullscreen: (f) => win.setFullscreen(f),
    isMaximized: () => win.isMaximized(),
    onResized: (cb) => win.onResized(() => cb()),
    minimize: () => win.minimize(),
    maximize: () => win.maximize(),
    unmaximize: () => win.unmaximize(),
    toggleMaximize: () => win.toggleMaximize(),
    startDragging: () => win.startDragging(),
    startResizeDragging: (direction) => win.startResizeDragging(direction),
    getBounds: async () => {
      const pos = await win.outerPosition()
      const size = await win.outerSize()
      return { x: pos.x, y: pos.y, width: size.width, height: size.height }
    },
    setBounds: async (b: WindowBounds) => {
      await win.setPosition(new PhysicalPosition(b.x, b.y))
      await win.setSize(new PhysicalSize(b.width, b.height))
    },
    show: () => win.show(),
    hide: () => win.hide(),
    close: () => win.close(),
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function openDialog(opts?: any): Promise<any> {
  return tauriOpen(opts)
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function saveDialog(opts?: any): Promise<any> {
  return tauriSave(opts)
}

export function clipboardWrite(text: string): Promise<void> {
  return tauriInvoke('plugin:clipboard-manager|write_text', { text })
}

export function clipboardRead(): Promise<string> {
  return tauriInvoke<string>('plugin:clipboard-manager|read_text')
}

export async function toFileUrl(filePath: string): Promise<string> {
  return convertFileSrc(filePath)
}
