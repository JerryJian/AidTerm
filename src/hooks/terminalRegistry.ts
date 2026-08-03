import type { Terminal } from '@xterm/xterm'
import type { FitAddon } from '@xterm/addon-fit'
import type { SearchAddon } from '@xterm/addon-search'

export interface RelocatedTerminal {
  terminal: Terminal
  fitAddon: FitAddon | null
  searchAddon: SearchAddon | null
  sessionId: string | null
  unlisten: (() => void) | null
  unlisteners: Array<() => void>
}

const registry = new Map<string, RelocatedTerminal>()

export function stashTerminal(tabId: string, bundle: RelocatedTerminal) {
  registry.set(tabId, bundle)
}

export function takeTerminal(tabId: string): RelocatedTerminal | undefined {
  const bundle = registry.get(tabId)
  if (bundle) registry.delete(tabId)
  return bundle
}
