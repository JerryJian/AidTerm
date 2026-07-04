import { ref } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { useAiStore, type AiMessage } from '../stores/aiStore'

export function useAiConversation(getTerminal: () => Terminal | null, writeToBackend?: (data: string) => void) {
  const ai = useAiStore()
  const showConfirm = ref(false)
  const pendingCommand = ref('')
  const pendingToolId = ref('')
  const pendingAiMsg = ref('')
  const aiActive = ref(false)
  const inputBuffer = ref('')
  const messages = ref<AiMessage[]>([])

  function writeAI(text: string, prefix = '[AI] ') {
    getTerminal()?.write(`\r\n\x1b[36m${prefix}\x1b[0m${text}`)
  }

  function writeAITitle(text: string) {
    getTerminal()?.write(`\r\n\x1b[35m━━━ ${text} ━━━\x1b[0m`)
  }

  function writeCommand(cmd: string) {
    getTerminal()?.write(`\r\n\x1b[33m$ ${cmd}\x1b[0m`)
  }

  function writeOutput(out: string) {
    for (const line of out.split('\n')) {
      getTerminal()?.write(`\r\n\x1b[90m${line}\x1b[0m`)
    }
  }

  function startConversation(userInput: string) {
    aiActive.value = true
    messages.value = [
      {
        role: 'system',
        content: `你是 TndTerm 终端 AI 助手。你可以通过 execute_command 工具在用户的系统上执行命令。
请根据用户的请求，分析问题并执行适当的命令。每次执行命令前，请先解释你要做什么。
注意：
- 使用工具时，系统会提示用户确认，用户确认后才会执行
- 命令输出会返回给你，你可以基于输出继续分析
- 如果用户请求不明确，可以询问细节
- 请使用中文或英文回复，根据用户输入的语言选择`,
      },
      { role: 'user', content: userInput },
    ]
    messages.value = [...messages.value]
    continueConversation()
  }

  async function continueConversation() {
    if (!ai.enabled) {
      writeAI('请先在设置中配置 AI API Key')
      endConversation()
      return
    }

    writeAITitle('AI 思考中...')

    try {
      const response = await ai.chat([...messages.value])

      if (response.text) {
        writeAI(response.text)
        messages.value.push({ role: 'assistant', content: response.text })
      }

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        pendingCommand.value = args.command || ''
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''

        // Save assistant message with tool call info
        messages.value.push({
          role: 'assistant',
          content: response.text || '',
        })

        showConfirm.value = true
      }
    } catch (e: any) {
      writeAI(`错误: ${e}`)
      endConversation()
    }
  }

  async function onConfirmCommand() {
    showConfirm.value = false
    const cmd = pendingCommand.value
    const toolId = pendingToolId.value
    pendingCommand.value = ''
    pendingToolId.value = ''

    writeAITitle('执行命令')
    writeCommand(cmd)

    try {
      const result = await ai.executeCommand(cmd)
      writeOutput(result || '(无输出)')

      writeAITitle('AI 继续分析')
      messages.value.push({
        role: 'tool',
        content: result || '(无输出)',
        tool_call_id: toolId,
      })

      const response = await ai.continueWithResult(toolId, result || '(无输出)')

      if (response.text) {
        writeAI(response.text)
        messages.value.push({ role: 'assistant', content: response.text })
      }

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        pendingCommand.value = args.command || ''
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''
        showConfirm.value = true
      }
    } catch (e: any) {
      writeAI(`执行错误: ${e}`)
      endConversation()
    }
  }

  function onCancelCommand() {
    showConfirm.value = false
    pendingCommand.value = ''
    pendingToolId.value = ''

    writeAI('已取消命令执行')
    endConversation()
  }

  function onModifyCommand(newCommand: string) {
    showConfirm.value = false
    pendingCommand.value = newCommand
    // Re-show the confirm with modified command
    showConfirm.value = true
  }

  function endConversation() {
    aiActive.value = false
    inputBuffer.value = ''
    ai.pendingToolCall = null
    writeToBackend?.('\r')
  }

  /**
   * Intercept terminal input. Returns true if input was consumed by AI.
   */
  function interceptInput(data: string): boolean {
    if (aiActive.value) {
      // In AI mode, buffer input. Enter sends it to AI chat.
      if (data === '\r' || data === '\n') {
        const line = inputBuffer.value.trim()
        inputBuffer.value = ''
        if (line === '/exit') {
          writeAI('AI 模式已退出')
          endConversation()
          return true
        }
        if (line) {
          writeCommand(line)
          messages.value.push({ role: 'user', content: line })
          continueConversation()
        }
        return true
      } else if (data === '\x7f') {
        // Backspace
        inputBuffer.value = inputBuffer.value.slice(0, -1)
        return true
      } else if (data.charCodeAt(0) >= 32) {
        inputBuffer.value += data
        return true
      }
      return true
    }

    // Not in AI mode - buffer line for NL detection
    if (data === '\r' || data === '\n') {
      const line = inputBuffer.value.trim()
      inputBuffer.value = ''

      // Check if it's a natural language request
      if (line && ai.isNaturalLanguage(line)) {
        if (!ai.enabled) {
          getTerminal()?.write(`\r\n\x1b[33m⚠ 请在设置 → AI 中配置 API Key 后使用 AI 助手\x1b[0m\r\n`)
          writeToBackend?.('\r')
          return true
        }
        // Don't send to terminal, start AI conversation
        startConversation(line)
        return true
      }

      // Not NL, clear buffer (let normal processing handle it)
      return false
    } else if (data === '\x7f') {
      inputBuffer.value = inputBuffer.value.slice(0, -1)
      return false
    } else if (data.charCodeAt(0) >= 32) {
      inputBuffer.value += data
      return false
    }

    return false
  }

  function resetConversation() {
    ai.clearHistory()
    endConversation()
    messages.value = []
    writeAI('对话已重置')
  }

  return {
    showConfirm,
    pendingCommand,
    pendingAiMsg,
    aiActive,
    interceptInput,
    onConfirmCommand,
    onCancelCommand,
    onModifyCommand,
    resetConversation,
    writeAI,
    startConversation,
  }
}
