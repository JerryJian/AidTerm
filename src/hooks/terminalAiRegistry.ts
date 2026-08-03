import { shallowReactive } from 'vue'
import type { useAiConversation } from './useAiConversation'

type AiConversation = ReturnType<typeof useAiConversation>

const registry = shallowReactive(new Map<string, AiConversation>())

export function registerAiConversation(tabId: string, conv: AiConversation) {
  registry.set(tabId, conv)
}

export function unregisterAiConversation(tabId: string) {
  registry.delete(tabId)
}

export function getAiConversation(tabId: string): AiConversation | undefined {
  return registry.get(tabId)
}
