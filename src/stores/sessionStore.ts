import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SavedSession, SavedSessionGroup, SessionStoreData } from '../types'

function genId(): string {
  return crypto.randomUUID()
}

export const useSessionStore = defineStore('sessions', () => {
  const groups = ref<SavedSessionGroup[]>([])
  const sessions = ref<SavedSession[]>([])
  const loaded = ref(false)

  async function load() {
    try {
      const data = await invoke<SessionStoreData>('load_session_store')
      groups.value = data.groups
      sessions.value = data.sessions
      loaded.value = true
    } catch (e) {
      console.error('Failed to load sessions:', e)
    }
  }

  async function save() {
    try {
      await invoke('save_session_store', {
        data: { groups: groups.value, sessions: sessions.value },
      })
    } catch (e) {
      console.error('Failed to save sessions:', e)
    }
  }

  function addGroup(name: string) {
    const group: SavedSessionGroup = { id: genId(), name, expanded: true }
    groups.value.push(group)
    save()
    return group
  }

  function removeGroup(id: string) {
    groups.value = groups.value.filter(g => g.id !== id)
    sessions.value.forEach(s => {
      if (s.group_id === id) s.group_id = null
    })
    save()
  }

  function renameGroup(id: string, name: string) {
    const g = groups.value.find(g => g.id === id)
    if (g) {
      g.name = name
      save()
    }
  }

  function addSession(
    name: string,
    type: SavedSession['session_type'],
    config: { host?: string; port?: number; username?: string; password?: string; privateKeyPath?: string; data_bits?: number; stop_bits?: number; parity?: string; flow_control?: string },
    groupId?: string | null,
  ) {
    const session: SavedSession = {
      id: genId(),
      name,
      session_type: type,
      group_id: groupId ?? null,
      host: config.host ?? null,
      port: config.port ?? null,
      username: config.username ?? null,
      password: config.password ?? null,
      private_key_path: config.privateKeyPath ?? null,
      proxy_id: null,
      last_connected: null,
      created_at: new Date().toISOString(),
      data_bits: config.data_bits ?? null,
      stop_bits: config.stop_bits ?? null,
      parity: config.parity ?? null,
      flow_control: config.flow_control ?? null,
    }
    sessions.value.push(session)
    save()
    return session
  }

  function removeSession(id: string) {
    sessions.value = sessions.value.filter(s => s.id !== id)
    save()
  }

  function updateSession(id: string, updates: Partial<SavedSession>) {
    const s = sessions.value.find(s => s.id === id)
    if (s) {
      Object.assign(s, updates)
      save()
    }
  }

  function updateLastConnected(id: string) {
    updateSession(id, { last_connected: new Date().toISOString() })
  }

  function ensureGroup(name: string): string | null {
    if (!name) return null
    const existing = groups.value.find(g => g.name === name)
    if (existing) return existing.id
    const g = addGroup(name)
    return g.id
  }

  function getSessionsByGroup(groupId: string | null): SavedSession[] {
    return sessions.value.filter(s => s.group_id === groupId)
  }

  function getUngroupedSessions(): SavedSession[] {
    return getSessionsByGroup(null)
  }

  return {
    groups,
    sessions,
    loaded,
    load,
    save,
    addGroup,
    removeGroup,
    renameGroup,
    addSession,
    removeSession,
    updateSession,
    updateLastConnected,
    getSessionsByGroup,
    getUngroupedSessions,
    ensureGroup,
  }
})
