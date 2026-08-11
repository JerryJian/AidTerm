/**
 * Platform abstraction layer.
 * Auto-detects Tauri vs Electron and re-exports a unified API.
 */

export type { UnlistenFn } from './types'

import type { UpdateInfo } from '@/types'
import type { DialogOptions } from './types'

export const isElectron = !!(window as unknown as { electronAPI?: unknown }).electronAPI

/**
 * Frame pushed from the Electron main process over the cast MessageChannel.
 * `data`/`config` arrive as ArrayBuffers (never base64). Tauri keeps the
 * `cast_frame` poll path, so this type only applies on Electron.
 */
export interface CastPushFrame {
  type: 'frame' | 'disconnect'
  seq?: number
  key?: boolean
  data?: ArrayBuffer
  config?: ArrayBuffer | null
}

export const castPushSupported = isElectron

/**
 * Open a push channel for a cast frame stream (Electron only). `onMessage`
 * receives each demuxed frame as a `CastPushFrame`; returns an unsubscribe that
 * closes the channel, or `null` on Tauri where `cast_frame` polling is used.
 */
export function castOpenPush(serial: string, onMessage: (msg: CastPushFrame) => void): (() => void) | null {
  if (!isElectron) return null
  const el = (window as unknown as { electronAPI?: { castOpenPush?: (s: string, cb: (m: unknown) => void) => () => void } }).electronAPI
  if (!el?.castOpenPush) return null
  return el.castOpenPush(serial, (m) => onMessage(m as CastPushFrame))
}

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

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const mod = await getMod()
  return mod.invoke<T>(cmd, args)
}

export async function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  const mod = await getMod()
  return mod.listen<T>(event, handler)
}

export async function openDialog(opts?: DialogOptions): Promise<null | string | string[]> {
  const mod = await getMod()
  return mod.openDialog(opts)
}

export async function saveDialog(opts?: DialogOptions): Promise<string | null> {
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

let cachedAppVersion: string | null = null

export async function getAppVersion(): Promise<string> {
  if (cachedAppVersion) return cachedAppVersion
  cachedAppVersion = await invoke<string>('get_app_version')
  return cachedAppVersion
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
