<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { getCurrentWindow, invoke, isElectron } from '@/api'
import type { ResizeDirection } from '@/api/types'

const win = getCurrentWindow()

// Tauri windows are frameless on every platform (tauri.conf.json decorations:false).
// Windows/macOS still get native edge resize even without decorations, so custom
// handles are only needed on Linux (Wayland/X11 lack reliable frameless resize).
// Electron keeps native resize everywhere, so it needs no handles.
const isMaximized = ref(false)
const isLinux = ref(false)

onMounted(async () => {
  try {
    isLinux.value = (await invoke<string>('get_platform')) === 'linux'
  } catch { /* ignore */ }
  try {
    isMaximized.value = await win.isMaximized()
    await win.onResized(async () => {
      isMaximized.value = await win.isMaximized()
    })
  } catch { /* ignore */ }
})

const showHandles = computed(() => !isElectron && isLinux.value && !isMaximized.value)

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

function onResizeStart(direction: ResizeDirection, e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  e.stopPropagation()
  win.startResizeDragging(direction)
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
