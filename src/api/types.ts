export type UnlistenFn = () => void

export type ResizeDirection = 'East' | 'North' | 'NorthEast' | 'NorthWest' | 'South' | 'SouthEast' | 'SouthWest' | 'West'

export interface ListenEvent<T = unknown> {
  payload: T
}

export interface DialogFilter {
  name: string
  extensions: string[]
}

export interface DialogOptions {
  title?: string
  defaultPath?: string
  multiple?: boolean
  directory?: boolean
  filters?: DialogFilter[]
}

export interface WindowHandle {
  isFullscreen(): Promise<boolean>
  setFullscreen(fullscreen: boolean): Promise<void>
  isMaximized(): Promise<boolean>
  onResized(cb: () => void): Promise<UnlistenFn>
  minimize(): Promise<void>
  maximize(): Promise<void>
  unmaximize(): Promise<void>
  toggleMaximize(): Promise<void>
  startDragging(): Promise<void>
  startResizeDragging(direction: ResizeDirection): Promise<void>
  show(): Promise<void>
  hide(): Promise<void>
  close(): Promise<void>
}
