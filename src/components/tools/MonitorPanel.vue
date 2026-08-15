<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '../../api'
import { useTerminalStore } from '../../stores/terminal'
import { useThemeStore } from '../../stores/themeStore'
import ECChart from './charts/ECChart.vue'
import type { TerminalTab, RemoteSystemMetrics } from '../../types'
import type { EChartsCoreOption } from 'echarts/core'

const props = defineProps<{ tabId: string; tab: TerminalTab; visible?: boolean }>()

const { t } = useI18n()
const terminalStore = useTerminalStore()
const themeStore = useThemeStore()

const sessionId = computed(() => terminalStore.resolveSessionTab(props.tab)?.session?.id ?? '')
const metrics = ref<RemoteSystemMetrics | null>(null)
const error = ref<string | null>(null)

// History buffers for the line charts (keep ~30 samples = 60s at 2s poll).
const MAX_POINTS = 30
const cpuHistory = ref<number[]>([])
const netHistory = ref<Record<string, { rx: number[]; tx: number[] }>>({})

let timer: ReturnType<typeof setInterval> | null = null
let busy = false
const windowActive = ref(true)

function fmtMb(mb: number): string {
  if (mb >= 1024) return (mb / 1024).toFixed(1) + ' GB'
  return Math.round(mb) + ' MB'
}

function fmtBps(bps: number): string {
  if (bps >= 1024 * 1024 * 1024) return (bps / (1024 * 1024 * 1024)).toFixed(1) + ' G/s'
  if (bps >= 1024 * 1024) return (bps / (1024 * 1024)).toFixed(1) + ' M/s'
  if (bps >= 1024) return (bps / 1024).toFixed(0) + ' K/s'
  return Math.round(bps) + ' B/s'
}

function percent(used: number, total: number): number {
  if (total <= 0) return 0
  return Math.min(100, Math.max(0, (used / total) * 100))
}

// Resolve a CSS variable used by the current theme (fallback for safe defaults).
// Reads themeStore.mode so chart options recompute when the theme switches.
function cssVar(name: string, fallback: string): string {
  void themeStore.mode
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return v || fallback
}

function chartColors() {
  return {
    accent: cssVar('--accent', '#89b4fa'),
    success: cssVar('--success', '#a6e3a1'),
    warning: cssVar('--warning', '#f9e2af'),
    danger: cssVar('--danger', '#f38ba8'),
    track: cssVar('--bg-surface0', 'rgba(128,128,128,.2)'),
    text: cssVar('--text', '#cdd6f4'),
    dim: cssVar('--text-sub0', 'rgba(205,214,244,.6)'),
    violet: '#cba6f7',
    periwinkle: '#b4befe',
    cyan: '#94e2d5',
  }
}

async function poll() {
  if (busy || !sessionId.value) return
  busy = true
  try {
    const m = await invoke<RemoteSystemMetrics>('get_remote_system_metrics', {
      sessionId: sessionId.value,
    })
    metrics.value = m
    error.value = null

    cpuHistory.value.push(m.cpu_percent)
    if (cpuHistory.value.length > MAX_POINTS) cpuHistory.value.shift()

    const next: Record<string, { rx: number[]; tx: number[] }> = {}
    for (const n of m.nets) {
      const prev = netHistory.value[n.name] ?? { rx: [], tx: [] }
      prev.rx.push(n.rx_bps)
      prev.tx.push(n.tx_bps)
      if (prev.rx.length > MAX_POINTS) prev.rx.shift()
      if (prev.tx.length > MAX_POINTS) prev.tx.shift()
      next[n.name] = prev
    }
    netHistory.value = next
  } catch (e) {
    error.value = String(e)
  } finally {
    busy = false
  }
}

const memPct = computed(() => percent(metrics.value?.mem_used_mb ?? 0, metrics.value?.mem_total_mb ?? 1))

function gaugeColor(val: number): string {
  if (val >= 85) return chartColors().danger
  if (val >= 60) return chartColors().warning
  return chartColors().success
}

function timeAxis(): string[] {
  return Array.from({ length: MAX_POINTS }, (_, i) => String(-((MAX_POINTS - 1 - i) * 2)))
}

function baseTextStyle() {
  const c = chartColors()
  return { color: c.dim, fontSize: 10, fontFamily: 'inherit' }
}

const cpuGaugeOption = computed<EChartsCoreOption>(() => {
  const c = chartColors()
  const v = metrics.value?.cpu_percent ?? 0
  return {
    series: [
      {
        type: 'gauge',
        startAngle: 180,
        endAngle: 0,
        min: 0,
        max: 100,
        radius: '100%',
        center: ['50%', '75%'],
        progress: {
          show: true,
          width: 9,
          roundCap: true,
          itemStyle: { color: gaugeColor(v) },
        },
        axisLine: {
          lineStyle: { width: 9, color: [[1, c.track]] },
        },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        pointer: { show: false },
        anchor: { show: false },
        title: { show: false },
        detail: {
          valueAnimation: true,
          offsetCenter: [0, '-10%'],
          fontSize: 16,
          fontWeight: 700,
          formatter: '{value}%',
          color: c.text,
        },
        data: [{ value: +v.toFixed(1) }],
      },
    ],
  }
})

const memDonutOption = computed<EChartsCoreOption>(() => {
  const c = chartColors()
  const used = metrics.value?.mem_used_mb ?? 0
  const total = metrics.value?.mem_total_mb ?? 1
  const pct = memPct.value
  return {
    tooltip: {
      trigger: 'item',
      formatter: (p: { name: string; value: number }) =>
        `${p.name}: ${fmtMb(p.value)}`,
      textStyle: baseTextStyle(),
      backgroundColor: cssVar('--bg-mantle', '#181825'),
      borderColor: cssVar('--border', '#313244'),
    },
    series: [
      {
        type: 'pie',
        radius: ['66%', '88%'],
        center: ['50%', '50%'],
        startAngle: 90,
        itemStyle: { borderRadius: 6, borderWidth: 0 },
        label: { show: false },
        emphasis: { scale: false },
        data: [
          { value: used, name: t('monitor_panel.used'), itemStyle: { color: c.success } },
          { value: Math.max(0, total - used), name: t('monitor_panel.free'), itemStyle: { color: c.track } },
        ],
      },
    ],
    graphic: [
      {
        type: 'text',
        left: 'center',
        top: 'center',
        style: {
          text: `${pct.toFixed(0)}%`,
          fontSize: 15,
          fontWeight: 700,
          fill: c.text,
          textAlign: 'center',
        },
      },
    ],
  }
})

const cpuLineOption = computed<EChartsCoreOption>(() => {
  const c = chartColors()
  return {
    grid: { left: 2, right: 2, top: 6, bottom: 2 },
    xAxis: {
      type: 'category',
      boundaryGap: false,
      data: timeAxis(),
      show: false,
    },
    yAxis: {
      type: 'value',
      min: 0,
      max: 100,
      show: false,
    },
    series: [
      {
        type: 'line',
        data: cpuHistory.value,
        smooth: true,
        symbol: 'none',
        lineStyle: { width: 1.5, color: c.accent },
        areaStyle: {
          color: {
            type: 'linear',
            x: 0, y: 0, x2: 0, y2: 1,
            colorStops: [
              { offset: 0, color: c.accent + '55' },
              { offset: 1, color: c.accent + '00' },
            ],
          },
        },
      },
    ],
  }
})

function gpuGaugeOption(util: number): EChartsCoreOption {
  const c = chartColors()
  return {
    series: [
      {
        type: 'gauge',
        startAngle: 180,
        endAngle: 0,
        min: 0,
        max: 100,
        radius: '100%',
        center: ['50%', '72%'],
        progress: {
          show: true,
          width: 7,
          roundCap: true,
          itemStyle: { color: gaugeColor(util) },
        },
        axisLine: {
          lineStyle: { width: 7, color: [[1, c.track]] },
        },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        pointer: { show: false },
        anchor: { show: false },
        title: { show: false },
        detail: {
          valueAnimation: true,
          offsetCenter: [0, '-8%'],
          fontSize: 12,
          fontWeight: 700,
          formatter: '{value}%',
          color: c.text,
        },
        data: [{ value: +util.toFixed(0) }],
      },
    ],
  }
}

function netLineOption(rx: number[], tx: number[]): EChartsCoreOption {
  const c = chartColors()
  const max = Math.max(1, ...rx, ...tx)
  return {
    grid: { left: 2, right: 2, top: 4, bottom: 2 },
    xAxis: { type: 'category', boundaryGap: false, data: timeAxis(), show: false },
    yAxis: { type: 'value', min: 0, max: max, show: false },
    series: [
      {
        name: t('monitor_panel.rx'),
        type: 'line',
        data: rx,
        smooth: true,
        symbol: 'none',
        lineStyle: { width: 1.2, color: c.success },
        areaStyle: {
          color: {
            type: 'linear',
            x: 0, y: 0, x2: 0, y2: 1,
            colorStops: [
              { offset: 0, color: c.success + '44' },
              { offset: 1, color: c.success + '00' },
            ],
          },
        },
      },
      {
        name: t('monitor_panel.tx'),
        type: 'line',
        data: tx,
        smooth: true,
        symbol: 'none',
        lineStyle: { width: 1.2, color: c.accent },
        areaStyle: {
          color: {
            type: 'linear',
            x: 0, y: 0, x2: 0, y2: 1,
            colorStops: [
              { offset: 0, color: c.accent + '44' },
              { offset: 1, color: c.accent + '00' },
            ],
          },
        },
      },
    ],
  }
}

const shouldPoll = computed(() => {
  if (props.visible === false) return false
  if (!windowActive.value) return false
  if (document.hidden) return false
  return !!sessionId.value
})

function start() {
  if (timer) return
  poll()
  timer = setInterval(poll, 2000)
}

function stop() {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

function onVisibility() {
  if (shouldPoll.value) {
    start()
  } else {
    stop()
  }
}

function onWindowFocus() {
  windowActive.value = true
  onVisibility()
}

function onWindowBlur() {
  windowActive.value = false
  stop()
}

watch(() => sessionId.value, (id, old) => {
  if (id !== old) {
    metrics.value = null
    error.value = null
    cpuHistory.value = []
    netHistory.value = {}
  }
  onVisibility()
})

watch(() => props.visible, onVisibility)

onMounted(() => {
  document.addEventListener('visibilitychange', onVisibility)
  window.addEventListener('focus', onWindowFocus)
  window.addEventListener('blur', onWindowBlur)
  onVisibility()
})

onBeforeUnmount(() => {
  stop()
  document.removeEventListener('visibilitychange', onVisibility)
  window.removeEventListener('focus', onWindowFocus)
  window.removeEventListener('blur', onWindowBlur)
})
</script>

<template>
  <div class="monitor-panel">
    <div class="toolbar">
      <span class="toolbar-title">{{ t('monitor_panel.title') }}</span>
    </div>

    <div v-if="!sessionId" class="empty-hint">
      {{ t('monitor_panel.no_session') }}
    </div>
    <div v-else-if="error" class="empty-hint error-hint">
      {{ t('monitor_panel.unavailable') }}
      <div class="error-detail">{{ error }}</div>
    </div>
    <template v-else-if="metrics">
      <!-- CPU gauge + Memory donut -->
      <div class="charts-row">
        <div class="chart-card">
          <div class="card-title">{{ t('monitor_panel.cpu') }}</div>
          <ECChart :option="cpuGaugeOption" :height="86" />
          <div class="card-sub">
            {{ metrics.cpu_cores }} {{ t('monitor_panel.core') }}
          </div>
          <div class="card-sub">
            {{ t('monitor_panel.load') }} {{ metrics.load_1.toFixed(2) }} / {{ metrics.load_5.toFixed(2) }} / {{ metrics.load_15.toFixed(2) }}
          </div>
        </div>
        <div class="chart-card">
          <div class="card-title">{{ t('monitor_panel.memory') }}</div>
          <ECChart :option="memDonutOption" :height="86" />
          <div class="card-sub">{{ fmtMb(metrics.mem_used_mb) }} / {{ fmtMb(metrics.mem_total_mb) }}</div>
          <div v-if="metrics.swap_total_mb > 0" class="card-sub">
            {{ t('monitor_panel.swap') }} {{ fmtMb(metrics.swap_used_mb) }} / {{ fmtMb(metrics.swap_total_mb) }}
          </div>
        </div>
      </div>

      <!-- CPU trend line -->
      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.cpu_history') }}</span>
        </div>
        <ECChart :option="cpuLineOption" :height="56" />
      </div>

      <!-- GPU: utilization gauge + VRAM bar (dual chart per GPU) -->
      <div v-if="metrics.gpus && metrics.gpus.length > 0" class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.gpu') }}</span>
        </div>
        <div v-for="g in metrics.gpus" :key="g.name" class="gpu-item">
          <div class="gpu-line">
            <span class="gpu-name">{{ g.name }}</span>
            <span v-if="g.temperature > 0" class="gpu-temp">{{ g.temperature.toFixed(0) }}°C</span>
          </div>
          <div class="gpu-dual">
            <div class="gpu-gauge-wrap">
              <ECChart :option="gpuGaugeOption(g.utilization)" :height="72" />
              <div class="gpu-gauge-label">{{ t('monitor_panel.usage') }}</div>
            </div>
            <div v-if="g.mem_total_mb > 0" class="vram-block">
              <div class="gpu-chart-label">{{ t('monitor_panel.vram') }}</div>
              <div class="gpu-chart-body">
                <div class="bar">
                  <div class="bar-fill vram" :style="{ width: percent(g.mem_used_mb, g.mem_total_mb) + '%' }" />
                </div>
              </div>
              <div class="gpu-chart-val">{{ fmtMb(g.mem_used_mb) }} / {{ fmtMb(g.mem_total_mb) }}</div>
            </div>
          </div>
        </div>
      </div>

      <!-- Disk bars (div bars, like GPU VRAM) -->
      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.disk') }}</span>
        </div>
        <div v-if="metrics.disks.length > 0">
          <div v-for="d in metrics.disks" :key="d.mount" class="disk-item">
            <div class="disk-line">
              <span class="disk-mount">{{ d.mount }}</span>
              <span class="disk-val">{{ fmtMb(d.used_mb) }} / {{ fmtMb(d.total_mb) }}</span>
            </div>
            <div class="bar">
              <div class="bar-fill disk" :style="{ width: percent(d.used_mb, d.total_mb) + '%' }" />
            </div>
          </div>
        </div>
        <div v-else class="metric-sub">
          {{ t('monitor_panel.unavailable') }}
        </div>
      </div>

      <!-- Network: line chart per interface -->
      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.network') }}</span>
        </div>
        <div v-for="n in metrics.nets" :key="n.name" class="net-item">
          <div class="net-row">
            <span class="net-name">{{ n.name }}</span>
            <span class="net-rate down">↓ {{ fmtBps(n.rx_bps) }}</span>
            <span class="net-rate up">↑ {{ fmtBps(n.tx_bps) }}</span>
          </div>
          <ECChart :option="netLineOption(netHistory[n.name]?.rx ?? [], netHistory[n.name]?.tx ?? [])" :height="52" />
        </div>
        <div v-if="metrics.nets.length === 0" class="metric-sub">
          {{ t('monitor_panel.unavailable') }}
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.monitor-panel {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--panel-bg, var(--bg-base));
  color: var(--fg, var(--text));
  font-size: 13px;
  overflow-y: auto;
}
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
  background: var(--bg-mantle);
  position: sticky;
  top: 0;
  z-index: 1;
}
.toolbar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.metric-block {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
}
.metric-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 6px;
}
.metric-value {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-sub0, var(--text));
  font-variant-numeric: tabular-nums;
}
.metric-sub {
  margin-top: 5px;
  font-size: 11px;
  color: var(--fg-dim, var(--text-overlay1));
}

/* CPU + Memory cards */
.charts-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  border-bottom: 1px solid var(--border, var(--bg-surface0));
}
.chart-card {
  padding: 8px 12px 10px;
  text-align: center;
  min-width: 0;
}
.chart-card + .chart-card {
  border-left: 1px solid var(--border, var(--bg-surface0));
}
.card-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-sub0, var(--text));
  margin-bottom: 2px;
}
.card-sub {
  margin-top: 3px;
  font-size: 10px;
  color: var(--fg-dim, var(--text-overlay1));
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* GPU */
.gpu-item { margin-top: 8px; }
.gpu-item + .gpu-item {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px dashed var(--border, var(--bg-surface0));
}
.gpu-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  margin-bottom: 4px;
}
.gpu-name {
  font-weight: 500;
  color: var(--text-sub0, var(--text));
  max-width: 70%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.gpu-temp { color: var(--danger, #f38ba8); font-size: 10px; }
.gpu-dual {
  display: flex;
  align-items: center;
  gap: 12px;
}
.gpu-gauge-wrap {
  flex: 0 0 88px;
  text-align: center;
}
.gpu-gauge-label {
  font-size: 10px;
  color: var(--fg-dim, var(--text-overlay1));
  margin-top: -2px;
}
.vram-block { flex: 1; min-width: 0; }
.gpu-chart-label {
  font-size: 10px;
  color: var(--fg-dim, var(--text-overlay1));
  margin-bottom: 3px;
}
.gpu-chart-body {
  display: flex;
  align-items: center;
  gap: 6px;
}
.gpu-chart-body .bar { flex: 1; }
.gpu-chart-val {
  margin-top: 4px;
  font-size: 10px;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

/* Bars (VRAM) */
.bar {
  height: 8px;
  border-radius: 4px;
  background: var(--bg-surface0);
  overflow: hidden;
}
.bar-fill {
  height: 100%;
  border-radius: 4px;
  transition: width 0.4s ease;
}
.bar-fill.vram { background: linear-gradient(90deg, #89b4fa, #94e2d5); }
.bar-fill.disk { background: #b4befe; }

/* Disk */
.disk-item { margin-top: 7px; }
.disk-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 3px;
  font-size: 11px;
}
.disk-mount {
  color: var(--text-sub0, var(--text));
  max-width: 60%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.disk-val {
  font-variant-numeric: tabular-nums;
  color: var(--text);
  white-space: nowrap;
}

/* Network */
.net-item { margin-top: 6px; }
.net-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 2px;
  font-size: 11px;
}
.net-name {
  font-weight: 500;
  color: var(--text-sub0, var(--text));
  margin-right: auto;
}
.net-rate {
  font-variant-numeric: tabular-nums;
  color: var(--text);
}
.net-rate.down { color: var(--success, #a6e3a1); }
.net-rate.up { color: var(--accent, #89b4fa); }

.empty-hint {
  text-align: center;
  padding: 32px;
  color: var(--fg-dim, var(--text-overlay1));
  font-size: 12px;
}
.error-hint .error-detail {
  margin-top: 8px;
  font-size: 11px;
  color: var(--danger, #f38ba8);
  word-break: break-all;
}
</style>
