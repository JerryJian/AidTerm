export type UnlistenFn = () => void

export type ResizeDirection = 'East' | 'North' | 'NorthEast' | 'NorthWest' | 'South' | 'SouthEast' | 'SouthWest' | 'West'

export interface ListenEvent<T = any> {
  payload: T
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
