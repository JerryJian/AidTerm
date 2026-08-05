<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke, getCurrentWindow } from '@/api'
import type { ResizeDirection } from '@/api/types'

const win = getCurrentWindow()
const isLinux = ref(false)

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

onMounted(async () => {
  try {
    const platform = await invoke<string>('get_platform')
    isLinux.value = platform === 'linux'
  } catch { /* ignore */ }
})

function onResizeStart(direction: ResizeDirection, e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  e.stopPropagation()
  win.startResizeDragging(direction)
}
</script>

<template>
  <div v-if="isLinux" class="resize-handles">
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
  height: 5px;
  cursor: ns-resize;
}

.rh-south {
  bottom: 0;
  left: 10px;
  right: 10px;
  height: 5px;
  cursor: ns-resize;
}

.rh-west {
  left: 0;
  top: 10px;
  bottom: 10px;
  width: 5px;
  cursor: ew-resize;
}

.rh-east {
  right: 0;
  top: 10px;
  bottom: 10px;
  width: 5px;
  cursor: ew-resize;
}

.rh-northwest {
  top: 0;
  left: 0;
  width: 12px;
  height: 12px;
  cursor: nwse-resize;
}

.rh-northeast {
  top: 0;
  right: 0;
  width: 12px;
  height: 12px;
  cursor: nesw-resize;
}

.rh-southwest {
  bottom: 0;
  left: 0;
  width: 12px;
  height: 12px;
  cursor: nesw-resize;
}

.rh-southeast {
  bottom: 0;
  right: 0;
  width: 12px;
  height: 12px;
  cursor: nwse-resize;
}
</style>
