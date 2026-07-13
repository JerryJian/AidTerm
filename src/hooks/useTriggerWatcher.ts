import { onMounted, onUnmounted } from 'vue'
import { invoke, listen } from '@/api'
import { useTriggerStore } from '../stores/triggerStore'
import { useTerminalStore } from '../stores/terminal'
import type { TerminalOutputPayload } from '../types'

export function useTriggerWatcher() {
  let unlisten: (() => void) | null = null

  onMounted(async () => {
    const triggerStore = useTriggerStore()
    const termStore = useTerminalStore()

    unlisten = await listen<TerminalOutputPayload>('terminal-output', async (event) => {
      const activeSessionId = termStore.activeTab?.session?.id
      if (!activeSessionId || event.payload.session_id !== activeSessionId) return

      const matched = triggerStore.findMatch(event.payload.data)
      if (matched) {
        await invoke('write_terminal', {
          sessionId: activeSessionId,
          data: matched.response.endsWith('\n') ? matched.response : matched.response + '\n',
        })
      }
    })
  })

  onUnmounted(() => {
    unlisten?.()
  })
}
