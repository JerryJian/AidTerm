import { ref, watch } from 'vue'
import type { Snippet } from '../types'

const STORAGE_KEY = 'aidterm_snippets'

const snippets = ref<Snippet[]>([])

function load() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) snippets.value = JSON.parse(raw) as Snippet[]
  } catch { /* ignore */ }
}

function persist() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snippets.value))
}

load()

watch(snippets, persist, { deep: true })

function genId(): string {
  return crypto.randomUUID()
}

export function useSnippetStore() {
  function add(name: string, command: string): Snippet {
    const s: Snippet = {
      id: genId(),
      name,
      command,
      sort_order: snippets.value.length,
    }
    snippets.value.push(s)
    return s
  }

  function update(id: string, data: Partial<Pick<Snippet, 'name' | 'command'>>) {
    const s = snippets.value.find(s => s.id === id)
    if (s) Object.assign(s, data)
  }

  function remove(id: string) {
    snippets.value = snippets.value.filter(s => s.id !== id)
  }

  function reorder(ids: string[]) {
    const map = new Map(snippets.value.map(s => [s.id, s]))
    snippets.value = ids.map((id, i) => {
      const s = map.get(id)
      if (s) s.sort_order = i
      return s ?? { id: '', name: '', command: '', sort_order: i }
    }).filter(s => s.id)
  }

  return { snippets, add, update, remove, reorder }
}
