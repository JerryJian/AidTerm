<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke, isElectron } from '../../api'
import type { TerminalTab } from '../../types'

const props = defineProps<{ tabId: string; tab: TerminalTab }>()

const { t } = useI18n()

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)

const running = ref(false)
const starting = ref(false)
const error = ref<string | null>(null)
const devWidth = ref(0)
const devHeight = ref(0)
const diag = ref('')

const serial = computed(() => props.tab.adbInfo?.serial ?? '')
const supported = !isElectron && typeof window !== 'undefined' && 'VideoDecoder' in window

let decoder: VideoDecoder | null = null
let configured = false
let waitingForKey = true
let ctx: CanvasRenderingContext2D | null = null
let pollTimer: ReturnType<typeof setInterval> | null = null
let lastSeq = 0
let pollsSinceNewFrame = 0
let downX = 0
let downY = 0
let downTime = 0
let downActive = false
let renderedCount = 0

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const bytes = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  return bytes
}

function drawFrame(frame: VideoFrame) {
  const canvas = canvasRef.value
  if (!canvas) {
    frame.close()
    return
  }
  if (!ctx) ctx = canvas.getContext('2d')
  if (!ctx) {
    frame.close()
    return
  }
  if (devWidth.value !== frame.displayWidth || devHeight.value !== frame.displayHeight) {
    devWidth.value = frame.displayWidth
    devHeight.value = frame.displayHeight
    canvas.width = frame.displayWidth
    canvas.height = frame.displayHeight
  }
  ctx.drawImage(frame, 0, 0, canvas.width, canvas.height)
  frame.close()
  renderedCount++
  diag.value = `已渲染 ${renderedCount} 帧 ${devWidth.value}x${devHeight.value}`
}

function onDecoderError(e: unknown) {
  // A decode hiccup must not kill the cast: drop the decoder and let the next
  // keyframe rebuild it (see decodeChunk: every keyframe recreates).
  console.warn('[cast] decoder error', e)
  decoder = null
  configured = false
  waitingForKey = true
  diag.value = '解码出错，等待下一关键帧自愈'
}

function closeDecoder() {
  if (!decoder) return
  try {
    if (decoder.state !== 'closed') decoder.close()
  } catch {
    // already closed (a codec error auto-closes it)
  }
  decoder = null
}

function setupDecoder() {
  closeDecoder()
  newDecoder()
}

function avccCodecString(avcc: Uint8Array): string {
  const hex = (b: number) => b.toString(16).padStart(2, '0')
  return `avc1.${hex(avcc[1])}${hex(avcc[2])}${hex(avcc[3])}`
}

function newDecoder(): VideoDecoder {
  decoder = new VideoDecoder({ output: drawFrame, error: onDecoderError })
  configured = false
  waitingForKey = true
  return decoder
}

// WebCodecs strictly requires the FIRST chunk after configure() to be a key
// frame, and any decode hiccup afterwards leaves the decoder wanting another
// key. So we rebuild the decoder on every key frame: fresh VideoDecoder +
// configure(avcC) + decode(keyframe) in one go. Delta chunks are only fed to
// an already healthy decoder.
async function decodeChunk(seq: number, key: boolean, b64: string, configB64: string | null) {
  if (key) {
    if (!configB64) {
      diag.value = `关键帧无 avcC 配置 (seq ${seq})`
      return
    }
    if (!decoder || decoder.state === 'closed') newDecoder()
    const avcc = base64ToBytes(configB64)
    try {
      decoder!.configure({ codec: avccCodecString(avcc), description: avcc })
      configured = true
      waitingForKey = false
      console.log('[cast] reconfigured with', avccCodecString(avcc), 'at seq', seq)
      diag.value = '已收到关键帧，重建解码器'
    } catch (e) {
      console.warn('[cast] configure with description failed', e)
      diag.value = '配置失败: ' + String((e as Error)?.message ?? e)
      closeDecoder()
      return
    }
  }
  if (waitingForKey) {
    diag.value = `跳过非关键帧，等待关键帧 (seq ${seq})`
    return
  }
  const chunk = new EncodedVideoChunk({
    type: key ? 'key' : 'delta',
    timestamp: seq * 40_000,
    data: base64ToBytes(b64),
  })
  try {
    decoder!.decode(chunk)
  } catch (e) {
    console.warn('[cast] decode failed', e)
    diag.value = 'decode 失败: ' + String((e as Error)?.message ?? e)
    // A failed decode poisons the decoder: rebuild and wait for the next key.
    closeDecoder()
    waitingForKey = true
  }
}

async function pollFrame() {
  if (!running.value) return
  let res: [number, boolean, string, string | null] | null = null
  try {
    res = await invoke<[number, boolean, string, string | null]>('cast_frame', {
      serial: serial.value,
      needKey: waitingForKey,
    })
  } catch {
    res = null
  }
  if (!res) {
    stopCasting()
    error.value = t('cast_panel.disconnected')
    return
  }
  const [seq, key, b64, config] = res
  if (seq === lastSeq) {
    pollsSinceNewFrame++
    if (pollsSinceNewFrame % 50 === 0) {
      console.log('[cast] no new frame for', pollsSinceNewFrame * 40, 'ms (seq still', lastSeq + ')')
    }
    return
  }
  lastSeq = seq
  pollsSinceNewFrame = 0
  console.log('[cast] frame', seq, 'key', key, 'bytes', b64.length, 'config', !!config)
  decodeChunk(seq, key, b64, config)
}

async function startCasting() {
  if (!serial.value) {
    error.value = t('cast_panel.no_device')
    return
  }
  if (!supported) {
    error.value = t('cast_panel.not_supported')
    return
  }
  starting.value = true
  error.value = null
  try {
    const port = await invoke<number>('cast_start', { serial: serial.value, maxSize: 1280 })
    console.log('[cast] cast_start returned port', port)
    setupDecoder()
    lastSeq = 0
    pollsSinceNewFrame = 0
    running.value = true
    pollTimer = setInterval(pollFrame, 40)
  } catch (e) {
    error.value = t('cast_panel.failed', { msg: String(e) })
    console.warn('[cast] cast_start failed', e)
  } finally {
    starting.value = false
  }
}

function stopCasting() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
  running.value = false
  if (serial.value) {
    invoke('cast_stop', { serial: serial.value }).catch(() => {})
  }
  closeDecoder()
  configured = false
  waitingForKey = true
  renderedCount = 0
  diag.value = ''
}

function canvasPos(e: PointerEvent | WheelEvent): { x: number; y: number } | null {
  const canvas = canvasRef.value
  if (!canvas || !devWidth.value || !devHeight.value) return null
  const rect = canvas.getBoundingClientRect()
  if (!rect.width || !rect.height) return null
  return {
    x: Math.round(((e.clientX - rect.left) * devWidth.value) / rect.width),
    y: Math.round(((e.clientY - rect.top) * devHeight.value) / rect.height),
  }
}

function sendInput(args: string) {
  if (!running.value) return
  invoke('cast_input', { serial: serial.value, cmd: args }).catch(() => {})
}

function onPointerDown(e: PointerEvent) {
  const p = canvasPos(e)
  if (!p) return
  downX = p.x
  downY = p.y
  downTime = Date.now()
  downActive = true
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}

function onPointerUp(e: PointerEvent) {
  if (!downActive) return
  downActive = false
  const p = canvasPos(e)
  if (!p) return
  const dx = p.x - downX
  const dy = p.y - downY
  if (Math.abs(dx) < 8 && Math.abs(dy) < 8) {
    sendInput(`tap ${p.x} ${p.y}`)
  } else {
    const duration = Math.min(Math.max(Date.now() - downTime, 100), 800)
    sendInput(`swipe ${downX} ${downY} ${p.x} ${p.y} ${duration}`)
  }
}

function onWheel(e: WheelEvent) {
  const p = canvasPos(e)
  if (!p) return
  e.preventDefault()
  const steps = Math.round(e.deltaY / 50)
  const dir = steps > 0 ? -1 : 1
  const dist = Math.abs(steps) * 120
  const y = Math.max(0, Math.min(devHeight.value, p.y + dir * dist))
  sendInput(`swipe ${p.x} ${p.y} ${p.x} ${y} 100`)
}

const keyEventMap: Record<string, number> = {
  Enter: 66,
  Backspace: 67,
  Tab: 61,
  Escape: 111,
  Home: 3,
  End: 123,
  PageUp: 92,
  PageDown: 93,
  ArrowUp: 19,
  ArrowDown: 20,
  ArrowLeft: 21,
  ArrowRight: 22,
}

function onWindowKeyDown(e: KeyboardEvent) {
  if (!running.value) return
  if (props.tab.activeToolTab !== 'cast' || !props.tab.toolSidebarOpen) return
  const target = e.target as HTMLElement
  if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
  const code = keyEventMap[e.key]
  if (code) {
    e.preventDefault()
    sendInput(`keyevent ${code}`)
  } else if (e.key === ' ') {
    e.preventDefault()
    sendInput('keyevent 62')
  } else if (e.key.length === 1 && /^[A-Za-z0-9]$/.test(e.key)) {
    e.preventDefault()
    sendInput(`text ${e.key}`)
  }
}

onBeforeUnmount(() => {
  stopCasting()
  window.removeEventListener('keydown', onWindowKeyDown)
})

window.addEventListener('keydown', onWindowKeyDown)
</script>

<template>
  <div class="cast-panel">
    <div class="cast-toolbar">
      <button class="cb-btn" :disabled="running || starting" @click="startCasting">
        {{ starting ? t('cast_panel.starting') : t('cast_panel.start') }}
      </button>
      <button class="cb-btn danger" :disabled="!running" @click="stopCasting">
        {{ t('cast_panel.stop') }}
      </button>
    </div>
    <div class="cast-stage" ref="containerRef">
      <div v-if="error" class="cast-error">{{ error }}</div>
      <div v-else-if="!running" class="cast-wait">
        <div v-if="!supported" class="cast-msg">{{ t('cast_panel.not_supported') }}</div>
        <div v-else-if="!serial" class="cast-msg">{{ t('cast_panel.no_device') }}</div>
        <div v-else class="cast-msg">{{ t('cast_panel.waiting') }}</div>
      </div>
      <canvas
        v-show="running"
        ref="canvasRef"
        class="cast-canvas"
        @pointerdown="onPointerDown"
        @pointerup="onPointerUp"
        @wheel.prevent="onWheel"
      />
      <div v-if="running" class="cast-hint">{{ t('cast_panel.hint_tap') }}</div>
      <div v-if="running && diag" class="cast-diag">{{ diag }}</div>
    </div>
  </div>
</template>

<style scoped>
.cast-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.cast-toolbar {
  display: flex;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--bg-surface0);
  flex-shrink: 0;
}

.cb-btn {
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 5px;
  padding: 6px 14px;
  font-size: 13px;
  cursor: pointer;
}

.cb-btn:hover:not(:disabled) {
  background: var(--bg-surface1);
}

.cb-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.cb-btn.danger:hover:not(:disabled) {
  background: rgba(220, 60, 60, 0.15);
  color: #e0656a;
}

.cast-stage {
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  position: relative;
  background: #101014;
}

.cast-canvas {
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
  touch-action: none;
  cursor: crosshair;
  image-rendering: auto;
}

.cast-wait,
.cast-error {
  padding: 20px;
  color: var(--text-sub0);
  font-size: 13px;
  text-align: center;
}

.cast-error {
  color: #e0656a;
}

.cast-hint {
  position: absolute;
  bottom: 10px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 12px;
  color: rgba(255, 255, 255, 0.55);
  background: rgba(0, 0, 0, 0.45);
  border-radius: 4px;
  padding: 4px 10px;
  pointer-events: none;
}

.cast-diag {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 12px;
  color: #7fd6a0;
  background: rgba(0, 0, 0, 0.6);
  border-radius: 4px;
  padding: 4px 10px;
  pointer-events: none;
  white-space: nowrap;
}
</style>
