import { ref } from 'vue'
import { invoke } from '@/api'
import type { ProxyConfig } from '../types'

const STORAGE_KEY = 'aidterm_proxies'

function loadFromStorage(): ProxyConfig[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')
  } catch {
    return []
  }
}

function saveToStorage(list: ProxyConfig[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
}

const proxies = ref<ProxyConfig[]>(loadFromStorage())
const loading = ref(false)
let synced = false

function genId(): string {
  return crypto.randomUUID()
}

export function useProxyStore() {
  async function refresh() {
    loading.value = true
    try {
      proxies.value = loadFromStorage()
      if (!synced) {
        synced = true
        for (const p of proxies.value) {
          await invoke('proxy_save', { config: p })
        }
      }
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
      saveToStorage(proxies.value)
    } finally {
      loading.value = false
    }
  }

  async function remove(id: string) {
    loading.value = true
    try {
      await invoke('proxy_delete', { id })
      proxies.value = proxies.value.filter(p => p.id !== id)
      saveToStorage(proxies.value)
    } finally {
      loading.value = false
    }
  }

  function getById(id: string): ProxyConfig | undefined {
    return proxies.value.find(p => p.id === id)
  }

  return { proxies, loading, refresh, save, remove, genId, getById }
}
