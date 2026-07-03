import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { TunnelCreateRequest, TunnelInfo } from '../types'

const tunnels = ref<TunnelInfo[]>([])
const loading = ref(false)

export function useTunnelStore() {
  async function create(req: TunnelCreateRequest): Promise<TunnelInfo> {
    loading.value = true
    try {
      const info = await invoke<TunnelInfo>('tunnel_create', { req })
      tunnels.value.push(info)
      return info
    } finally {
      loading.value = false
    }
  }

  async function refresh(): Promise<void> {
    loading.value = true
    try {
      tunnels.value = await invoke<TunnelInfo[]>('tunnel_list')
    } finally {
      loading.value = false
    }
  }

  async function remove(id: string): Promise<void> {
    loading.value = true
    try {
      await invoke('tunnel_remove', { id })
      tunnels.value = tunnels.value.filter((t: TunnelInfo) => t.id !== id)
    } finally {
      loading.value = false
    }
  }

function tunnelStatusText(status: 'Starting' | 'Running' | 'Stopped' | { Error: string }): string {
  if (status === 'Starting') return '启动中'
  if (status === 'Running') return '运行中'
  if (status === 'Stopped') return '已停止'
  if (typeof status === 'object' && 'Error' in status) return `错误: ${status.Error}`
  return '未知'
}

  return { tunnels, loading, create, refresh, remove, tunnelStatusText }
}
