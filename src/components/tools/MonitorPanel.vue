<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '../../api'
import { useTerminalStore } from '../../stores/terminal'
import type { TerminalTab, RemoteSystemMetrics } from '../../types'

const props = defineProps<{ tabId: string; tab: TerminalTab }>()

const { t } = useI18n()
const terminalStore = useTerminalStore()

const sessionId = computed(() => terminalStore.resolveSessionTab(props.tab)?.session?.id ?? '')
const metrics = ref<RemoteSystemMetrics | null>(null)
const error = ref<string | null>(null)

let timer: ReturnType<typeof setInterval> | null = null
let busy = false

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

async function poll() {
  if (busy || !sessionId.value) return
  busy = true
  try {
    metrics.value = await invoke<RemoteSystemMetrics>('get_remote_system_metrics', {
      sessionId: sessionId.value,
    })
    error.value = null
  } catch (e) {
    error.value = String(e)
  } finally {
    busy = false
  }
}

onMounted(() => {
  poll()
  timer = setInterval(poll, 2000)
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
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
      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.cpu') }}</span>
          <span class="metric-value">{{ metrics.cpu_percent.toFixed(1) }}%</span>
        </div>
        <div class="bar">
          <div class="bar-fill cpu" :style="{ width: metrics.cpu_percent + '%' }" />
        </div>
        <div class="metric-sub">
          {{ metrics.cpu_cores }} {{ t('monitor_panel.core') }} ·
          {{ t('monitor_panel.load') }} {{ metrics.load_1.toFixed(2) }} / {{ metrics.load_5.toFixed(2) }} / {{ metrics.load_15.toFixed(2) }}
        </div>
      </div>

      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.memory') }}</span>
          <span class="metric-value">
            {{ fmtMb(metrics.mem_used_mb) }} / {{ fmtMb(metrics.mem_total_mb) }}
          </span>
        </div>
        <div class="bar">
          <div class="bar-fill mem" :style="{ width: percent(metrics.mem_used_mb, metrics.mem_total_mb) + '%' }" />
        </div>
      </div>

      <div v-if="metrics.swap_total_mb > 0" class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.swap') }}</span>
          <span class="metric-value">
            {{ fmtMb(metrics.swap_used_mb) }} / {{ fmtMb(metrics.swap_total_mb) }}
          </span>
        </div>
        <div class="bar">
          <div class="bar-fill swap" :style="{ width: percent(metrics.swap_used_mb, metrics.swap_total_mb) + '%' }" />
        </div>
      </div>

      <div v-if="metrics.gpus && metrics.gpus.length > 0" class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.gpu') }}</span>
        </div>
        <div v-for="g in metrics.gpus" :key="g.name" class="gpu-item">
          <div class="gpu-line">
            <span class="gpu-name">{{ g.name }}</span>
            <span class="metric-value">{{ g.utilization.toFixed(1) }}%</span>
          </div>
          <div class="bar">
            <div class="bar-fill gpu" :style="{ width: g.utilization + '%' }" />
          </div>
          <div v-if="g.mem_total_mb > 0" class="gpu-sub">
            <span>{{ t('monitor_panel.memory') }} {{ fmtMb(g.mem_used_mb) }} / {{ fmtMb(g.mem_total_mb) }}</span>
            <span v-if="g.temperature > 0" class="gpu-temp">{{ g.temperature.toFixed(0) }}°C</span>
          </div>
        </div>
      </div>

      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.disk') }}</span>
        </div>
        <div v-for="d in metrics.disks" :key="d.mount" class="disk-item">
          <div class="disk-line">
            <span class="disk-mount">{{ d.mount }}</span>
            <span class="metric-value">{{ fmtMb(d.used_mb) }} / {{ fmtMb(d.total_mb) }}</span>
          </div>
          <div class="bar bar-sm">
            <div class="bar-fill disk" :style="{ width: percent(d.used_mb, d.total_mb) + '%' }" />
          </div>
        </div>
        <div v-if="metrics.disks.length === 0" class="metric-sub">
          {{ t('monitor_panel.unavailable') }}
        </div>
      </div>

      <div class="metric-block">
        <div class="metric-head">
          <span>{{ t('monitor_panel.network') }}</span>
        </div>
        <div v-for="n in metrics.nets" :key="n.name" class="net-item">
          <div class="net-name">{{ n.name }}</div>
          <div class="net-row">
            <span class="net-arrow down">↓</span>
            <span class="net-rate">{{ fmtBps(n.rx_bps) }}</span>
            <span class="net-arrow up">↑</span>
            <span class="net-rate">{{ fmtBps(n.tx_bps) }}</span>
          </div>
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
.bar {
  height: 8px;
  border-radius: 4px;
  background: var(--bg-surface0);
  overflow: hidden;
}
.bar-sm { height: 5px; margin-top: 3px; }
.bar-fill {
  height: 100%;
  border-radius: 4px;
  transition: width 0.4s ease;
}
.bar-fill.cpu { background: var(--accent, #89b4fa); }
.bar-fill.mem { background: var(--success, #a6e3a1); }
.bar-fill.swap { background: var(--warning, #f9e2af); }
.bar-fill.disk { background: #b4befe; }
.bar-fill.gpu { background: #f5c2e7; }
.gpu-item { margin-top: 6px; }
.gpu-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
}
.gpu-name {
  font-weight: 500;
  color: var(--text-sub0, var(--text));
  max-width: 70%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.gpu-sub {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 3px;
  font-size: 10px;
  color: var(--fg-dim, var(--text-overlay1));
}
.gpu-temp { color: var(--danger, #f38ba8); }
.disk-item, .net-item { margin-top: 6px; }
.disk-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
}
.disk-mount {
  color: var(--text-sub0, var(--text));
  max-width: 60%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.net-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
}
.net-name {
  font-weight: 500;
  color: var(--text-sub0, var(--text));
}
.net-row {
  display: flex;
  align-items: center;
  gap: 4px;
}
.net-arrow.down { color: var(--success, #a6e3a1); }
.net-arrow.up { color: var(--accent, #89b4fa); }
.net-rate {
  font-variant-numeric: tabular-nums;
  color: var(--text);
}
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
