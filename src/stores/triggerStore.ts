import { ref, watch } from 'vue'
import type { Trigger } from '../types'

const STORAGE_KEY = 'tndterm_triggers'

const triggers = ref<Trigger[]>([])

function load() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) triggers.value = JSON.parse(raw) as Trigger[]
  } catch { /* ignore */ }
}

function persist() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(triggers.value))
}

load()
watch(triggers, persist, { deep: true })

function genId(): string {
  return crypto.randomUUID()
}

export function useTriggerStore() {
  function add(name: string, pattern: string, response: string, cooldownMs = 3000): Trigger {
    const t: Trigger = {
      id: genId(),
      name,
      pattern,
      response,
      enabled: true,
      cooldown_ms: cooldownMs,
      last_fired: 0,
    }
    triggers.value.push(t)
    return t
  }

  function update(id: string, data: Partial<Trigger>) {
    const t = triggers.value.find(t => t.id === id)
    if (t) Object.assign(t, data)
  }

  function remove(id: string) {
    triggers.value = triggers.value.filter(t => t.id !== id)
  }

  function findMatch(text: string): Trigger | null {
    const now = Date.now()
    for (const t of triggers.value) {
      if (!t.enabled) continue
      if (now - t.last_fired < t.cooldown_ms) continue
      try {
        const re = new RegExp(t.pattern)
        if (re.test(text)) {
          t.last_fired = now
          return t
        }
      } catch { /* invalid regex, skip */ }
    }
    return null
  }

  return { triggers, add, update, remove, findMatch }
}
