import { shallowReactive } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { useAiConversation, type AiTerminalBinding } from './useAiConversation'

type AiConversation = ReturnType<typeof useAiConversation>

// One AI conversation per tab (keyed by the tab's shared aiSessionId).
// All panes of a split tab share the same conversation, so switching panes
// does not lose the conversation content.
const conversations = shallowReactive(new Map<string, AiConversation>())

// Per-pane terminal bindings so the shared conversation can target the pane
// that was selected when a question was asked.
const leafBindings = shallowReactive(new Map<string, AiTerminalBinding>())

export function getOrCreateAiConversation(aiSessionId: string, getTerminal: () => Terminal | null = () => null): AiConversation {
  let conv = conversations.get(aiSessionId)
  if (!conv) {
    conv = useAiConversation(getTerminal, undefined, undefined, aiSessionId)
    conversations.set(aiSessionId, conv)
  }
  return conv
}

export function getAiConversation(aiSessionId: string): AiConversation | undefined {
  return conversations.get(aiSessionId)
}

export function unregisterAiConversation(aiSessionId: string) {
  conversations.delete(aiSessionId)
}

export function pruneAiConversations(activeSessionIds: string[]) {
  const active = new Set(activeSessionIds)
  for (const key of [...conversations.keys()]) {
    if (!active.has(key)) conversations.delete(key)
  }
}

export function registerLeafBinding(leafId: string, binding: AiTerminalBinding) {
  leafBindings.set(leafId, binding)
}

export function unregisterLeafBinding(leafId: string) {
  leafBindings.delete(leafId)
}

export function getLeafBinding(leafId: string | null | undefined): AiTerminalBinding | undefined {
  return leafId ? leafBindings.get(leafId) : undefined
}
