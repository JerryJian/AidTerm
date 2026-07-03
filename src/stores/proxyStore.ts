import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ProxyConfig } from '../types'

const proxies = ref<ProxyConfig[]>([])
const loading = ref(false)

function genId(): string {
  return crypto.randomUUID()
}

export function useProxyStore() {
  async function refresh() {
    loading.value = true
    try {
      proxies.value = await invoke<ProxyConfig[]>('proxy_list')
    } finally {
      loading.value = false
    }
  }

  async function save(config: ProxyConfig) {
    loading.value = true
    try {
      await invoke('proxy_save', { config })
      const idx = proxies.value.findIndex(p => p.id === config.id)
      if (idx >= 0) {
        proxies.value[idx] = config
      } else {
        proxies.value.push(config)
      }
    } finally {
      loading.value = false
    }
  }

  async function remove(id: string) {
    loading.value = true
    try {
      await invoke('proxy_delete', { id })
      proxies.value = proxies.value.filter(p => p.id !== id)
    } finally {
      loading.value = false
    }
  }

  function getById(id: string): ProxyConfig | undefined {
    return proxies.value.find(p => p.id === id)
  }

  return { proxies, loading, refresh, save, remove, genId, getById }
}
