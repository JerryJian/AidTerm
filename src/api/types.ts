export type UnlistenFn = () => void

export interface ListenEvent<T = any> {
  payload: T
}

export interface WindowHandle {
  isFullscreen(): Promise<boolean>
  setFullscreen(fullscreen: boolean): Promise<void>
  isMaximized(): Promise<boolean>
  minimize(): Promise<void>
  toggleMaximize(): Promise<void>
  startDragging(): Promise<void>
  show(): Promise<void>
  hide(): Promise<void>
  close(): Promise<void>
}
