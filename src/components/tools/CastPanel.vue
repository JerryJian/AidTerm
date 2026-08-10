<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke, isElectron } from '../../api'
import type { TerminalTab } from '../../types'

const props = defineProps<{ tabId: string; tab: TerminalTab }>()

const { t } = useI18n()

const containerRef = ref<HTMLDivElement | null>(null)
const canvasRef = ref<HTMLCanvasElement | null>(null)
const bezelRef = ref<HTMLDivElement | null>(null)
const navRef = ref<HTMLDivElement | null>(null)

const running = ref(false)
const starting = ref(false)
const reconnecting = ref(false)
const error = ref<string | null>(null)
const devWidth = ref(0)
const devHeight = ref(0)
const diag = ref('')

const MAX_RETRIES = 5
let retryCount = 0
let manualStop = false

const serial = computed(() => props.tab.adbInfo?.serial ?? '')
const supported = !isElectron && typeof window !== 'undefined' && 'VideoDecoder' in window

let decoder: VideoDecoder | null = null
let waitingForKey = true
let currentAvccB64: string | null = null
let pollTimer: ReturnType<typeof setInterval> | null = null
let lastSeq = 0
let pollsSinceNewFrame = 0
let downX = 0
let downY = 0
let downTime = 0
let downActive = false
// Real device display resolution (native orientation, usually portrait) from
// `adb shell wm size`. 0 = unknown (wm size failed) — canvasPos then falls
// back to the streamed video size, which is only correct when max_size does
// not downscale the video.
let realWidth = 0
let realHeight = 0

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
  // Always get the context from the current canvas element. Vue may recreate
  // the canvas element when `running` toggles (v-if/v-else), so the cached
  // ctx from a previous canvas would silently draw to a detached element.
  const ctx = canvas.getContext('2d')
  if (!ctx) {
    frame.close()
    return
  }
  // Defence-in-depth: apply the correct pixel size whenever the canvas
  // element was freshly created (browser-default 300x150) OR the device
  // actually changed resolution. A stale devWidth/Height from a previous
  // session is reset in stopCasting(), but we check the canvas pixel size
  // directly too so even a missed reset cannot cause blurry output.
  const needResize =
    devWidth.value !== frame.displayWidth ||
    devHeight.value !== frame.displayHeight ||
    canvas.width !== frame.displayWidth ||
    canvas.height !== frame.displayHeight
  if (needResize) {
    devWidth.value = frame.displayWidth
    devHeight.value = frame.displayHeight
    canvas.width = frame.displayWidth
    canvas.height = frame.displayHeight
    fitCanvas()
  }
  ctx.drawImage(frame, 0, 0, canvas.width, canvas.height)
  frame.close()
}

// Scale the canvas to fit inside the phone screen area. The canvas keeps the
// device aspect ratio; available space = stage box minus stage padding, bezel
// padding and the nav bar. Measured from the stage box (not the bezel, whose
// size depends on the canvas itself), so this stays stable as the canvas
// resizes.
function fitCanvas() {
  const stage = containerRef.value
  const canvas = canvasRef.value
  const bezel = bezelRef.value
  const nav = navRef.value
  if (!stage || !canvas || !bezel || !nav) return
  const dw = devWidth.value || 720
  const dh = devHeight.value || 1280
  if (!dw || !dh) return
  const ss = getComputedStyle(stage)
  const bs = getComputedStyle(bezel)
  const stagePadX = parseFloat(ss.paddingLeft) + parseFloat(ss.paddingRight)
  const stagePadY = parseFloat(ss.paddingTop) + parseFloat(ss.paddingBottom)
  const bezelPadX = parseFloat(bs.paddingLeft) + parseFloat(bs.paddingRight)
  const bezelPadY = parseFloat(bs.paddingTop) + parseFloat(bs.paddingBottom)
  const navH = nav.getBoundingClientRect().height
  const availW = stage.clientWidth - stagePadX - bezelPadX
  const availH = stage.clientHeight - stagePadY - bezelPadY - navH
  if (availW <= 0 || availH <= 0) return
  const scale = Math.min(availW / dw, availH / dh) * 0.995
  canvas.style.width = `${Math.floor(dw * scale)}px`
  canvas.style.height = `${Math.floor(dh * scale)}px`
}

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  if (containerRef.value) {
    resizeObserver = new ResizeObserver(fitCanvas)
    resizeObserver.observe(containerRef.value)
  }
})

function onDecoderError(e: unknown) {
  // A decode hiccup must not kill the cast: drop the decoder and let the next
  // keyframe rebuild it (see decodeChunk: every keyframe recreates).
  console.warn('[cast] decoder error', e)
  decoder = null
  currentAvccB64 = null
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
  currentAvccB64 = null
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
  waitingForKey = true
  currentAvccB64 = null
  return decoder
}

// WebCodecs requires the FIRST chunk after configure() to be a key frame, and
// the reference chain must be complete: a delta can only be decoded if every
// earlier frame of its GOP was fed too. The backend buffers each GOP and hands
// frames out FIFO, so we feed EVERY frame in order and never skip — otherwise
// a later delta references a frame we dropped and the decoder breaks.
// Returns true when the frame was handed to the decoder (so the caller should
// advance lastSeq); false when it was deliberately skipped (caller must NOT
// advance, or the backend would think the chain is complete).
async function decodeChunk(seq: number, key: boolean, b64: string, configB64: string | null): Promise<boolean> {
  const needDecoder = !decoder || decoder.state === 'closed'
  const effectiveConfig = configB64 ?? currentAvccB64
  const avccChanged = effectiveConfig !== null && effectiveConfig !== currentAvccB64
  const needReconfig = needDecoder || avccChanged
  if (needReconfig) {
    // Config packet (key=false) with new avcC: configure the decoder now so
    // the upcoming keyframe can be decoded. WebCodecs requires configure()
    // before any decode(), and configure() does not need a keyframe.
    if (!key) {
      if (!effectiveConfig) {
        diag.value = `跳过非关键帧，等待关键帧 (seq ${seq})`
        return false
      }
      if (needDecoder) newDecoder()
      const avcc = base64ToBytes(effectiveConfig)
      try {
        decoder!.configure({ codec: avccCodecString(avcc), description: avcc })
        currentAvccB64 = effectiveConfig
        console.log('[cast] pre-configured with', avccCodecString(avcc), 'at seq', seq, '(config packet, waiting for keyframe)')
      } catch (e) {
        console.warn('[cast] configure with description failed', e)
        diag.value = '配置失败: ' + String((e as Error)?.message ?? e)
        closeDecoder()
        return false
      }
      // Do NOT clear waitingForKey — we still need a keyframe to decode
      return false
    }
    if (!effectiveConfig) {
      diag.value = `关键帧无 avcC 配置 (seq ${seq})`
      return false
    }
    if (needDecoder) newDecoder()
    const avcc = base64ToBytes(effectiveConfig)
    try {
      decoder!.configure({ codec: avccCodecString(avcc), description: avcc })
      currentAvccB64 = effectiveConfig
      waitingForKey = false
      console.log('[cast] reconfigured with', avccCodecString(avcc), 'at seq', seq, needDecoder ? '(new decoder)' : '(avcC change)')
    } catch (e) {
      console.warn('[cast] configure with description failed', e)
      diag.value = '配置失败: ' + String((e as Error)?.message ?? e)
      closeDecoder()
      return false
    }
  }
  if (waitingForKey) {
    if (key) {
      // Keyframe arrived — clear waitingForKey. The decoder chain is now valid.
      waitingForKey = false
      console.log('[cast] keyframe seq', seq, 'cleared waitingForKey (decoder already valid)')
    } else {
      diag.value = `等待关键帧 (seq ${seq})`
      return false
    }
  }
  const chunk = new EncodedVideoChunk({
    type: key ? 'key' : 'delta',
    timestamp: seq * 40_000,
    data: base64ToBytes(b64),
  })
  try {
    decoder!.decode(chunk)
    return true
  } catch (e) {
    console.warn('[cast] decode failed', e)
    diag.value = 'decode 失败: ' + String((e as Error)?.message ?? e)
    // A failed decode poisons the decoder: rebuild and wait for the next key.
    closeDecoder()
    waitingForKey = true
    return false
  }
}

async function pollFrame() {
  if (!running.value && !reconnecting.value) return
  let res: [number, boolean, string, string | null] | null = null
  try {
    res = await invoke<[number, boolean, string, string | null]>('cast_frame', {
      serial: serial.value,
      needKey: waitingForKey,
      seenSeq: lastSeq,
    })
  } catch {
    res = null
  }
  if (!res) {
    if (!manualStop && retryCount < MAX_RETRIES) {
      running.value = false
      reconnecting.value = true
      retryCount++
      diag.value = `连接断开，正在重连 (${retryCount}/${MAX_RETRIES})...`
      console.warn('[cast] stream disconnected, reconnecting attempt', retryCount)
      // Stop the old poll timer before reconnecting
      if (pollTimer) {
        clearInterval(pollTimer)
        pollTimer = null
      }
      closeDecoder()
      waitingForKey = true
      // Attempt reconnection with backoff
      const delay = Math.min(1000 * retryCount, 5000)
      setTimeout(async () => {
        if (!reconnecting.value || manualStop) return
        try {
          await invoke('cast_stop', { serial: serial.value }).catch(() => {})
          const info = await invoke<{ port: number; width: number | null; height: number | null }>(
            'cast_start',
            { serial: serial.value, maxSize: 1280 },
          )
          realWidth = info.width ?? 0
          realHeight = info.height ?? 0
          console.log('[cast] reconnected, port', info.port)
          reconnecting.value = false
          setupDecoder()
          lastSeq = 0
          pollsSinceNewFrame = 0
          running.value = true
          pollTimer = setInterval(pollFrame, 40)
          void nextTick(fitCanvas)
          diag.value = `已重连 (${retryCount} 次)`
          retryCount = 0
        } catch (e) {
          console.warn('[cast] reconnect failed', e)
          if (retryCount >= MAX_RETRIES) {
            reconnecting.value = false
            stopCasting()
            error.value = t('cast_panel.disconnected')
          } else {
            // Schedule another attempt
            pollTimer = setInterval(pollFrame, 40)
          }
        }
      }, delay)
      return
    }
    // Max retries exhausted or manual stop
    reconnecting.value = false
    stopCasting()
    error.value = t('cast_panel.disconnected')
    return
  }
  const [seq, key, b64, config] = res
  // seq=0 means "no new frame, keep polling" (backend buffer empty, stream
  // still alive). This replaces the old `seq === lastSeq` check and avoids
  // mistaking a stale seq for a disconnect.
  if (seq === 0) {
    pollsSinceNewFrame++
    if (pollsSinceNewFrame % 50 === 0) {
      console.log('[cast] no new frame for', pollsSinceNewFrame * 40, 'ms')
    }
    return
  }
  pollsSinceNewFrame = 0
  console.log('[cast] frame', seq, 'key', key, 'bytes', b64.length, 'config', !!config)
  const fed = await decodeChunk(seq, key, b64, config)
  if (fed) {
    lastSeq = seq
  } else {
    // decode failed: the frame is already popped from the backend (pop
    // semantics), so we just set waitingForKey and let the next poll skip
    // deltas until the next keyframe arrives.
    console.log('[cast] decode failed seq', seq, 'waitingForKey', waitingForKey)
  }
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
  manualStop = false
  reconnecting.value = false
  retryCount = 0
  try {
    // Ensure any previous session is fully stopped before starting a new one
    await invoke('cast_stop', { serial: serial.value }).catch(() => {})
    const info = await invoke<{ port: number; width: number | null; height: number | null }>(
      'cast_start',
      { serial: serial.value, maxSize: 1280 },
    )
    realWidth = info.width ?? 0
    realHeight = info.height ?? 0
    console.log('[cast] cast_start returned port', info.port, 'real', realWidth, 'x', realHeight)
    setupDecoder()
    lastSeq = 0
    pollsSinceNewFrame = 0
    running.value = true
    pollTimer = setInterval(pollFrame, 40)
    void nextTick(fitCanvas)
  } catch (e) {
    error.value = t('cast_panel.failed', { msg: String(e) })
    console.warn('[cast] cast_start failed', e)
  } finally {
    starting.value = false
  }
}

function stopCasting() {
  manualStop = true
  reconnecting.value = false
  retryCount = 0
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
  running.value = false
  if (serial.value) {
    invoke('cast_stop', { serial: serial.value }).catch(() => {})
  }
  closeDecoder()
  waitingForKey = true
  currentAvccB64 = null
  diag.value = ''
  // Reset device dimensions so the next fresh start always re-applies the
  // correct canvas.width/canvas.height from the first incoming frame. Without
  // this a restart at the *same* resolution skips the size-mismatch branch
  // (because devWidth/Height still equal the frame values) and leaves the
  // newly-created canvas at its browser-default 300x150 — the frame then
  // gets downscaled into a tiny canvas, which looks very blurry once CSS
  // stretches it back to phone size.
  devWidth.value = 0
  devHeight.value = 0
  realWidth = 0
  realHeight = 0
  lastSeq = 0
  pollsSinceNewFrame = 0
  downX = 0
  downY = 0
  downTime = 0
  downActive = false
  // Canvas element is about to be destroyed by Vue's v-if toggle (running
  // changed to false), so clearRect is unnecessary; we just drop the ref.
  canvasRef.value = null
}

function canvasPos(e: PointerEvent | WheelEvent): { x: number; y: number } | null {
  const canvas = canvasRef.value
  if (!canvas || !devWidth.value || !devHeight.value) return null
  const rect = canvas.getBoundingClientRect()
  if (!rect.width || !rect.height) return null
  // `adb shell input tap/swipe` operates in the device's REAL screen coordinate
  // space, but the streamed video is downscaled by scrcpy's max_size. So the
  // click ratio across the canvas must be mapped to the real resolution, not
  // the video resolution. Without this, taps land in the top-left quadrant.
  //
  // realWidth/Height come from `wm size` in the device's NATIVE orientation
  // (usually portrait). scrcpy follows device rotation, so the video swaps
  // orientation when the device rotates — orient the real dimensions to match
  // the current video orientation before mapping.
  let rw = realWidth
  let rh = realHeight
  if (rw > 0 && rh > 0) {
    const videoLandscape = devWidth.value > devHeight.value
    const realLandscape = rw > rh
    if (videoLandscape !== realLandscape) {
      ;[rw, rh] = [rh, rw]
    }
  } else {
    // Fallback: real size unknown (wm size failed) — use video dimensions.
    rw = devWidth.value
    rh = devHeight.value
  }
  return {
    x: Math.round(((e.clientX - rect.left) * rw) / rect.width),
    y: Math.round(((e.clientY - rect.top) * rh) / rect.height),
  }
}

function sendInput(args: string) {
  if (!running.value) return
  invoke('cast_input', { serial: serial.value, cmd: args }).catch(() => {})
}

function sendKeyEvent(code: number) {
  sendInput(`keyevent ${code}`)
}

// Android key codes: 26 power, 24/25 volume, 4 back, 3 home, 187 recents.
const PHONE_KEYS = {
  power: 26,
  volumeUp: 24,
  volumeDown: 25,
  back: 4,
  home: 3,
  recent: 187,
} as const

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
  manualStop = true
  stopCasting()
  resizeObserver?.disconnect()
  resizeObserver = null
  window.removeEventListener('keydown', onWindowKeyDown)
})

window.addEventListener('keydown', onWindowKeyDown)
</script>

<template>
  <div class="cast-panel">
    <div class="cast-toolbar">
      <button class="cb-btn" :disabled="running || starting || reconnecting" @click="startCasting">
        {{ starting ? t('cast_panel.starting') : t('cast_panel.start') }}
      </button>
      <button class="cb-btn danger" :disabled="!running && !reconnecting" @click="stopCasting">
        {{ t('cast_panel.stop') }}
      </button>
    </div>
    <div class="cast-stage" ref="containerRef">
      <div v-if="error" class="cast-error">{{ error }}</div>
      <div v-else-if="!running && !reconnecting" class="cast-wait">
        <div v-if="!supported" class="cast-msg">{{ t('cast_panel.not_supported') }}</div>
        <div v-else-if="!serial" class="cast-msg">{{ t('cast_panel.no_device') }}</div>
        <div v-else class="cast-msg">{{ t('cast_panel.waiting') }}</div>
      </div>
      <div v-else class="phone-frame">
        <div class="phone-bezel" ref="bezelRef">
          <div class="phone-side-buttons">
            <button
              class="phone-btn side-btn vol-up"
              :title="t('cast_panel.volume_up')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.volumeUp)"
            >
              <span class="vol-icon">+</span>
            </button>
            <button
              class="phone-btn side-btn vol-down"
              :title="t('cast_panel.volume_down')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.volumeDown)"
            >
              <span class="vol-icon">−</span>
            </button>
            <button
              class="phone-btn side-btn power"
              :title="t('cast_panel.power')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.power)"
            >
              <span class="power-icon" />
            </button>
          </div>
          <div class="phone-screen">
            <canvas
              ref="canvasRef"
              class="cast-canvas"
              @pointerdown="onPointerDown"
              @pointerup="onPointerUp"
              @wheel.prevent="onWheel"
            />
          </div>
          <div class="phone-nav" ref="navRef">
            <button
              class="nav-btn"
              :title="t('cast_panel.nav_back')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.back)"
            >
              <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6" /></svg>
            </button>
            <button
              class="nav-btn home-btn"
              :title="t('cast_panel.nav_home')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.home)"
            >
              <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 10.5 12 3l9 7.5" /><path d="M5 9.5V20a1 1 0 0 0 1 1h4v-6h4v6h4a1 1 0 0 0 1-1V9.5" /></svg>
            </button>
            <button
              class="nav-btn"
              :title="t('cast_panel.nav_recent')"
              @pointerdown.stop
              @click="sendKeyEvent(PHONE_KEYS.recent)"
            >
              <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="5" width="14" height="14" rx="2" /></svg>
            </button>
          </div>
        </div>
      </div>
      <div v-if="running" class="cast-hint">{{ t('cast_panel.hint_tap') }}</div>
      <div v-if="(running || reconnecting) && diag" class="cast-diag">{{ diag }}</div>
      <div v-if="reconnecting && !running" class="cast-reconnecting">
        <span class="reconnect-spinner" />
        {{ t('cast_panel.reconnecting') || '正在重连...' }}
      </div>
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
  padding: 12px;
}

.phone-frame {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
}

.phone-bezel {
  position: relative;
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  max-width: 100%;
  max-height: 100%;
  padding: 18px 16px 8px;
  background: linear-gradient(150deg, #2a2b31, #141518);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 32px;
  box-shadow:
    0 14px 44px rgba(0, 0, 0, 0.55),
    inset 0 0 0 1px rgba(0, 0, 0, 0.9),
    inset 0 1px 0 rgba(255, 255, 255, 0.08);
}

.phone-screen {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  max-width: 100%;
  max-height: 100%;
  border-radius: 20px;
  overflow: hidden;
  background: #000;
  box-shadow: inset 0 0 0 2px #000, 0 0 0 1px rgba(255, 255, 255, 0.07);
  flex-shrink: 1;
}

.phone-screen::before {
  content: '';
  position: absolute;
  top: 6px;
  left: 50%;
  transform: translateX(-50%);
  width: 84px;
  height: 5px;
  border-radius: 3px;
  background: rgba(0, 0, 0, 0.85);
  z-index: 2;
  pointer-events: none;
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.06);
}

.cast-canvas {
  flex-shrink: 0;
  touch-action: none;
  cursor: crosshair;
  image-rendering: auto;
  display: block;
}

.phone-side-buttons {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.phone-btn {
  pointer-events: auto;
  position: absolute;
  border: none;
  cursor: pointer;
  background: #3a3b42;
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.12),
    inset 0 -1px 0 rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
}

.phone-btn:hover:not(:disabled) {
  background: #4c4d56;
}

.phone-btn:active:not(:disabled) {
  background: #2c2d33;
  box-shadow: inset 0 0 6px rgba(0, 0, 0, 0.6);
}

.side-btn.vol-up,
.side-btn.vol-down {
  left: 0;
  width: 10px;
  height: 46px;
  border-radius: 6px 0 0 6px;
}

.side-btn.vol-up {
  top: 18%;
}

.side-btn.vol-down {
  top: calc(18% + 52px);
}

.side-btn.power {
  right: 0;
  width: 10px;
  height: 56px;
  border-radius: 0 6px 6px 0;
  top: 24%;
}

.vol-icon {
  position: absolute;
  left: 2px;
  font-size: 11px;
  line-height: 1;
  color: rgba(255, 255, 255, 0.55);
  font-weight: 600;
}

.power-icon {
  position: absolute;
  right: 2px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  border: 1.5px solid rgba(255, 255, 255, 0.55);
}

.power-icon::after {
  content: '';
  position: absolute;
  top: -5px;
  left: 50%;
  transform: translateX(-50%);
  width: 1.5px;
  height: 5px;
  background: rgba(255, 255, 255, 0.55);
}

.phone-nav {
  display: flex;
  gap: 34px;
  align-items: center;
  justify-content: center;
  padding: 10px 12px 4px;
  width: 100%;
  flex-shrink: 0;
}

.nav-btn {
  width: 42px;
  height: 38px;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.72);
  border-radius: 10px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background 0.15s,
    color 0.15s;
}

.nav-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}

.nav-btn:active:not(:disabled) {
  background: rgba(255, 255, 255, 0.18);
}

.home-btn {
  width: 74px;
  border-radius: 24px;
  background: rgba(255, 255, 255, 0.08);
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

.cast-reconnecting {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #e8b84a;
  background: rgba(0, 0, 0, 0.75);
  border-radius: 8px;
  padding: 10px 18px;
  pointer-events: none;
}

.reconnect-spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid rgba(232, 184, 74, 0.3);
  border-top-color: #e8b84a;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
