/**
 * Platform abstraction layer.
 * Auto-detects Tauri vs Electron and re-exports a unified API.
 */

export type { UnlistenFn } from './types'

import type { UpdateInfo } from '@/types'

export const isElectron = !!(window as any).electronAPI

type ApiModule = typeof import('./tauri') | typeof import('./electron')

let modPromise: Promise<ApiModule>

if (isElectron) {
  modPromise = import('./electron')
} else {
  modPromise = import('./tauri')
}

async function getMod(): Promise<ApiModule> {
  return modPromise
}

export async function invoke<T>(cmd: string, args?: Record<string, any>): Promise<T> {
  const mod = await getMod()
  return mod.invoke<T>(cmd, args)
}

export async function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  const mod = await getMod()
  return mod.listen<T>(event, handler)
}

export async function openDialog(opts?: any): Promise<any> {
  const mod = await getMod()
  return mod.openDialog(opts)
}

export async function saveDialog(opts?: any): Promise<any> {
  const mod = await getMod()
  return mod.saveDialog(opts)
}

export async function clipboardWrite(text: string): Promise<void> {
  const mod = await getMod()
  return mod.clipboardWrite(text)
}

export async function clipboardRead(): Promise<string> {
  const mod = await getMod()
  return mod.clipboardRead()
}

export async function toFileUrl(filePath: string): Promise<string> {
  const mod = await getMod()
  return mod.toFileUrl(filePath)
}

export async function getAppVersion(): Promise<string> {
  return invoke<string>('get_app_version')
}

export async function getInstallerType(): Promise<string> {
  return invoke<string>('get_installer_type')
}

export async function checkForUpdate(): Promise<UpdateInfo> {
  return invoke<UpdateInfo>('check_for_update')
}

export async function downloadUpdate(url: string): Promise<string> {
  return invoke<string>('download_update', { url })
}

export async function installUpdate(path: string): Promise<void> {
  return invoke<void>('install_update', { path })
}

export function getCurrentWindow() {
  // getCurrentWindow is synchronous in both implementations
  // We need to handle this differently - return a proxy that lazy-loads
  return {
    async isFullscreen() {
      const mod = await getMod()
      return mod.getCurrentWindow().isFullscreen()
    },
    async setFullscreen(fullscreen: boolean) {
      const mod = await getMod()
      return mod.getCurrentWindow().setFullscreen(fullscreen)
    },
    async isMaximized() {
      const mod = await getMod()
      return mod.getCurrentWindow().isMaximized()
    },
    async onResized(cb: () => void) {
      const mod = await getMod()
      return mod.getCurrentWindow().onResized(cb)
    },
    async minimize() {
      const mod = await getMod()
      return mod.getCurrentWindow().minimize()
    },
    async maximize() {
      const mod = await getMod()
      return mod.getCurrentWindow().maximize()
    },
    async unmaximize() {
      const mod = await getMod()
      return mod.getCurrentWindow().unmaximize()
    },
    async toggleMaximize() {
      const mod = await getMod()
      return mod.getCurrentWindow().toggleMaximize()
    },
    async startDragging() {
      const mod = await getMod()
      return mod.getCurrentWindow().startDragging()
    },
    async startResizeDragging(direction: import('./types').ResizeDirection) {
      const mod = await getMod()
      return mod.getCurrentWindow().startResizeDragging(direction)
    },
    async show() {
      const mod = await getMod()
      return mod.getCurrentWindow().show()
    },
    async hide() {
      const mod = await getMod()
      return mod.getCurrentWindow().hide()
    },
    async close() {
      const mod = await getMod()
      return mod.getCurrentWindow().close()
    },
  }
}
