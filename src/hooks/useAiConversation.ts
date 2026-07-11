import { ref, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useAiStore, type AiMessage } from '../stores/aiStore'
import { useTerminalStore } from '../stores/terminal'
import type { SystemInfo } from '../types'

const MAX_HISTORY = 10
const LONG_OUTPUT_THRESHOLD = 5000
const LONG_OUTPUT_TRUNCATE = 8000
const PROMPT_TIMEOUT = 30000

interface CommandRecord {
  command: string
  result: string
}

export interface ConversationMessage {
  role: 'user' | 'assistant' | 'command' | 'result' | 'error' | 'thinking'
  content: string
  command?: string
  toolCallId?: string
}

function buildSystemPrompt(systemInfo: SystemInfo, history: CommandRecord[]): string {
  const lines: string[] = [
    '你是 AidTerm 终端 AI 助手。你可以通过 execute_command 工具在用户当前的终端中执行命令。',
    '请根据用户的请求，分析问题并执行适当的命令。每次执行命令前，请先解释你要做什么。',
    '',
    '注意：命令会直接在用户的终端 shell 中执行，拥有与用户相同的环境变量、PATH 和权限。',
    '',
    '=== 当前终端系统信息 ===',
    `操作系统: ${systemInfo.os}`,
    `架构: ${systemInfo.arch}`,
    `主机名: ${systemInfo.hostname}`,
    `内核: ${systemInfo.kernel}`,
    `Shell: ${systemInfo.shell}`,
  ]

  if (history.length > 0) {
    lines.push('', '=== 最近执行的命令 ===')
    for (const h of history) {
      lines.push(`$ ${h.command}`)
      const resultPreview = h.result.length > 200 ? h.result.slice(0, 200) + '...(已截断)' : h.result
      if (resultPreview) lines.push(resultPreview)
    }
  }

  lines.push(
    '',
    '注意：',
    '- 使用工具时，系统会提示用户确认，用户确认后才会执行',
    '- 命令输出会返回给你，你可以基于输出继续分析',
    '- 如果用户请求不明确，可以询问细节',
    '- 请使用中文或英文回复，根据用户输入的语言选择',
  )

  return lines.join('\n')
}

function stripAnsi(text: string): string {
  return text.replace(/\x1b\[[\d;]*[a-zA-Z]/g, '').replace(/\r/g, '')
}

function detectPromptInOutput(output: string, savedPrompt?: string): boolean {
  const plain = stripAnsi(output)
  const lines = plain.split('\n')
  const lastLine = (lines[lines.length - 1] || lines[lines.length - 2] || '').trimEnd()

  if (!lastLine) return false

  if (savedPrompt && savedPrompt.trim() !== '$ ' && lastLine.includes(savedPrompt.trimEnd())) {
    return true
  }

  const promptPatterns = [
    /[$#>]\s*$/,
    /[%→]\s*$/,
  ]

  return promptPatterns.some(p => p.test(lastLine))
}

function cleanCommandOutput(output: string, cmd: string): string {
  const plain = stripAnsi(output)
  const lines = plain.split('\n')

  let startIdx = -1
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].includes(cmd)) {
      startIdx = i
      break
    }
  }

  const resultLines = startIdx >= 0 ? lines.slice(startIdx + 1) : lines

  const trimmed = resultLines.filter(l => l.trim().length > 0)

  if (trimmed.length > 0) {
    const lastIdx = trimmed.length - 1
    const last = trimmed[lastIdx]
    if (/[$#>]\s*$/.test(last) || /[%→]\s*$/.test(last)) {
      trimmed.pop()
    }
  }

  return trimmed.join('\n').trim()
}

export function useAiConversation(
  getTerminal: () => any | null,
  writeToBackend?: (data: string) => void,
  rawOnOutput?: (cb: (data: string) => void) => Promise<(() => void) | null | undefined>,
) {
  const ai = useAiStore()
  const showConfirm = ref(false)
  const pendingCommand = ref('')
  const pendingToolId = ref('')
  const pendingAiMsg = ref('')
  const busy = ref(false)
  const cancelled = ref(false)
  const messages = ref<AiMessage[]>([])
  const commandHistory = ref<CommandRecord[]>([])
  const savedPrompt = ref('$ ')

  const conversationMessages = reactive<ConversationMessage[]>([])

  function detectSavedPrompt() {
    const t = getTerminal()
    if (!t) return
    try {
      const buf = t.buffer.active
      const ln = buf.getLine(buf.baseY + buf.cursorY)
      if (ln) {
        const text = ln.translateToString()
        const match = text.match(/^.*?([^\s].*?)$/)
        if (match) {
          savedPrompt.value = match[1]
        }
      }
    } catch {}
  }

  async function waitForPrompt(cmd: string): Promise<string> {
    if (!rawOnOutput || !writeToBackend) {
      return ai.executeCommand(cmd)
    }

    return new Promise<string>(resolve => {
      let output = ''
      let unsub: (() => void) | null = null
      let resolved = false

      const cleanup = () => {
        if (unsub) {
          unsub()
          unsub = null
        }
      }

      rawOnOutput!((data: string) => {
        output += data
      }).then((un: (() => void) | null | undefined) => {
        if (un) unsub = un
      })

      writeToBackend!(cmd + '\r')

      const t0 = Date.now()

      const check = () => {
        if (resolved) return

        if (detectPromptInOutput(output, savedPrompt.value)) {
          resolved = true
          cleanup()
          const result = cleanCommandOutput(output, cmd)
          resolve(result)
          return
        }

        if (Date.now() - t0 > PROMPT_TIMEOUT) {
          resolved = true
          cleanup()
          const result = cleanCommandOutput(output, cmd)
          resolve(result || output)
          return
        }

        setTimeout(check, 100)
      }

      setTimeout(check, 300)
    })
  }

  function addConversationMessage(msg: ConversationMessage) {
    conversationMessages.push(msg)
  }

  function updateLastConversationMessage(updates: Partial<ConversationMessage>) {
    if (conversationMessages.length > 0) {
      const last = conversationMessages[conversationMessages.length - 1]
      Object.assign(last, updates)
    }
  }

  function removeLastThinking() {
    for (let i = conversationMessages.length - 1; i >= 0; i--) {
      if (conversationMessages[i].role === 'thinking') {
        conversationMessages.splice(i, 1)
        return
      }
    }
  }

  async function startConversation(userInput: string) {
    cancelled.value = false
    busy.value = true

    detectSavedPrompt()

    addConversationMessage({ role: 'user', content: userInput })

    const termStore = useTerminalStore()
    const tab = termStore.activeTab

    const systemInfo = tab?.systemInfo ?? await invoke<SystemInfo>('get_system_info')

    const systemPrompt = buildSystemPrompt(systemInfo, commandHistory.value)
    messages.value = [
      { role: 'system', content: systemPrompt },
      { role: 'user', content: userInput },
    ]

    addConversationMessage({ role: 'thinking', content: '' })
    continueConversation()
  }

  async function continueConversation() {
    try {
      const response = await ai.chat([...messages.value])
      if (cancelled.value) return
      removeLastThinking()

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        pendingCommand.value = args.command || ''
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''

        messages.value.push({
          role: 'assistant',
          content: response.text || '',
        })

        if (response.text) {
          addConversationMessage({ role: 'assistant', content: response.text })
        }

        addConversationMessage({
          role: 'command',
          content: args.command || '',
          command: args.command || '',
          toolCallId: tc.id,
        })

        showConfirm.value = true
        updateLastConversationMessage({ role: 'command' })
      } else if (response.text) {
        addConversationMessage({ role: 'assistant', content: response.text })
        messages.value.push({ role: 'assistant', content: response.text })
        endConversation()
      } else {
        endConversation()
      }
    } catch (e: any) {
      removeLastThinking()
      addConversationMessage({ role: 'error', content: `错误: ${e}` })
      endConversation()
    }
  }

  async function onConfirmCommand() {
    showConfirm.value = false
    const cmd = pendingCommand.value
    const toolId = pendingToolId.value
    pendingCommand.value = ''
    pendingToolId.value = ''

    try {
      addConversationMessage({ role: 'thinking', content: '执行中...' })

      const result = await waitForPrompt(cmd)
      if (cancelled.value) return
      removeLastThinking()

      const displayResult = result || '(无输出)'

      addConversationMessage({ role: 'result', content: displayResult })

      let resultForAI = displayResult
      if (displayResult.length > LONG_OUTPUT_THRESHOLD) {
        resultForAI = displayResult.slice(0, LONG_OUTPUT_TRUNCATE) +
          `\n\n...(输出过长，仅显示前 ${LONG_OUTPUT_TRUNCATE} 字符，共 ${displayResult.length} 字符)`
        addConversationMessage({
          role: 'error',
          content: `输出较长 (${displayResult.length} 字符)，已截断发送给 AI`,
        })
      }

      commandHistory.value.unshift({ command: cmd, result: displayResult })
      if (commandHistory.value.length > MAX_HISTORY) {
        commandHistory.value = commandHistory.value.slice(0, MAX_HISTORY)
      }

      addConversationMessage({ role: 'thinking', content: 'AI 分析中...' })

      messages.value.push({
        role: 'tool',
        content: resultForAI,
        tool_call_id: toolId,
      })

      const response = await ai.continueWithResult(toolId, resultForAI)
      if (cancelled.value) return
      removeLastThinking()

      if (response.text) {
        messages.value.push({ role: 'assistant', content: response.text })
      }

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        pendingCommand.value = args.command || ''
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''

        if (response.text) {
          addConversationMessage({ role: 'assistant', content: response.text })
        }

        addConversationMessage({
          role: 'command',
          content: args.command || '',
          command: args.command || '',
          toolCallId: tc.id,
        })

        showConfirm.value = true
      } else if (response.text) {
        addConversationMessage({ role: 'assistant', content: response.text })
        endConversation()
      } else {
        endConversation()
      }
    } catch (e: any) {
      removeLastThinking()
      addConversationMessage({ role: 'error', content: `执行错误: ${e}` })
      endConversation()
    }
  }

  function onCancelCommand() {
    showConfirm.value = false
    cancelled.value = true
    pendingCommand.value = ''
    pendingToolId.value = ''

    addConversationMessage({ role: 'error', content: '已取消命令执行' })
    writeToBackend?.('\x03')
    endConversation()
  }

  function onModifyCommand(newCommand: string) {
    showConfirm.value = false
    pendingCommand.value = newCommand
    showConfirm.value = true
  }

  function endConversation() {
    busy.value = false
    ai.pendingToolCall = null
  }

  function submitInput(text: string) {
    const trimmed = text.trim()
    if (!trimmed) return

    if (!ai.enabled) {
      addConversationMessage({ role: 'error', content: '请在设置 → AI 中配置 API Key 后使用 AI 助手' })
      return
    }

    startConversation(trimmed)
  }

  function resetConversation() {
    ai.clearHistory()
    endConversation()
    messages.value = []
    conversationMessages.length = 0
    addConversationMessage({ role: 'assistant', content: '对话已重置' })
  }

  function forceAIInput() {
    const t = getTerminal()
    if (!t || busy.value) return
    if (!ai.enabled) {
      addConversationMessage({ role: 'error', content: '请在设置 → AI 中配置 API Key 后使用 AI 助手' })
      return
    }
  }

  return {
    showConfirm,
    pendingCommand,
    pendingToolId,
    pendingAiMsg,
    busy,
    cancelled,
    conversationMessages,
    onConfirmCommand,
    onCancelCommand,
    onModifyCommand,
    resetConversation,
    startConversation,
    commandHistory,
    submitInput,
    forceAIInput,
  }
}
