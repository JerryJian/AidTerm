<script setup lang="ts">
import { computed, onUnmounted } from 'vue'
import { getCurrentWindow, isElectron } from '@/api'
import type { ResizeDirection, WindowBounds } from '@/api/types'

const win = getCurrentWindow()

// Tauri windows are frameless on every platform (tauri.conf.json decorations:false)
// and resize via the compositor-native startResizeDragging, so handles are always
// needed there. Electron keeps native resize everywhere (Windows/macOS native
// frame, Linux Wayland extended resize boundaries), so it needs no handles.
const showHandles = computed(() => !isElectron)

// Tauri exposes the compositor-native `startResizeDragging` (smooth, works on
// frameless windows); Electron has no such API, so it keeps the manual
// getBounds/setBounds pointer tracking. Each is chosen per backend.
const nativeResize = !isElectron

const zones: { direction: ResizeDirection; className: string }[] = [
  { direction: 'North', className: 'rh-north' },
  { direction: 'East', className: 'rh-east' },
  { direction: 'South', className: 'rh-south' },
  { direction: 'West', className: 'rh-west' },
  { direction: 'NorthWest', className: 'rh-northwest' },
  { direction: 'NorthEast', className: 'rh-northeast' },
  { direction: 'SouthEast', className: 'rh-southeast' },
  { direction: 'SouthWest', className: 'rh-southwest' },
]

let active: { dir: ResizeDirection; sx: number; sy: number; b: WindowBounds } | null = null

onUnmounted(() => {
  cleanup()
})

function cleanup() {
  if (!active) return
  active = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
}

function onResizeStart(direction: ResizeDirection, e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  e.stopPropagation()

  if (nativeResize) {
    win.startResizeDragging(direction)
    return
  }

  win.getBounds().then((b) => {
    active = { dir: direction, sx: e.clientX, sy: e.clientY, b }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  })
}

function onMove(e: PointerEvent) {
  if (!active) return
  const { dir, sx, sy, b } = active
  const dx = e.clientX - sx
  const dy = e.clientY - sy
  let { x, y, width, height } = b

  if (dir.includes('East')) width = b.width + dx
  if (dir.includes('West')) { x = b.x + dx; width = b.width - dx }
  if (dir.includes('South')) height = b.height + dy
  if (dir.includes('North')) { y = b.y + dy; height = b.height - dy }

  if (width < 100 || height < 60) return
  win.setBounds({ x, y, width, height })
}

function onUp() {
  cleanup()
}
</script>

<template>
  <div v-if="showHandles" class="resize-handles">
    <div
      v-for="zone in zones"
      :key="zone.direction"
      class="rh-zone"
      :class="zone.className"
      @pointerdown="onResizeStart(zone.direction, $event)"
    />
  </div>
</template>

<style scoped>
.resize-handles {
  position: fixed;
  inset: 0;
  z-index: 50;
  pointer-events: none;
}

.rh-zone {
  position: absolute;
  pointer-events: auto;
}

.rh-north {
  top: 0;
  left: 10px;
  right: 10px;
  height: 6px;
  cursor: ns-resize;
}

.rh-south {
  bottom: 0;
  left: 10px;
  right: 10px;
  height: 6px;
  cursor: ns-resize;
}

.rh-west {
  left: 0;
  top: 10px;
  bottom: 10px;
  width: 6px;
  cursor: ew-resize;
}

.rh-east {
  right: 0;
  top: 10px;
  bottom: 10px;
  width: 6px;
  cursor: ew-resize;
}

.rh-northwest {
  top: 0;
  left: 0;
  width: 14px;
  height: 14px;
  cursor: nwse-resize;
}

.rh-northeast {
  top: 0;
  right: 0;
  width: 14px;
  height: 14px;
  cursor: nesw-resize;
}

.rh-southwest {
  bottom: 0;
  left: 0;
  width: 14px;
  height: 14px;
  cursor: nesw-resize;
}

.rh-southeast {
  bottom: 0;
  right: 0;
  width: 14px;
  height: 14px;
  cursor: nwse-resize;
}
</style>
