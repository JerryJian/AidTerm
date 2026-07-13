/**
 * Platform abstraction layer.
 * Auto-detects Tauri vs Electron and re-exports a unified API.
 */

export type { UnlistenFn } from './types'

const isElectron = !!(window as any).electronAPI

let mod: typeof import('./tauri') | typeof import('./electron')

if (isElectron) {
  mod = await import('./electron')
} else {
  mod = await import('./tauri')
}

export const invoke = mod.invoke
export const listen = mod.listen
export const openDialog = mod.openDialog
export const saveDialog = mod.saveDialog
export const clipboardWrite = mod.clipboardWrite
export const clipboardRead = mod.clipboardRead
export const getCurrentWindow = mod.getCurrentWindow
