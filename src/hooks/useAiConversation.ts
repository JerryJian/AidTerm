import { ref } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { invoke } from '@tauri-apps/api/core'
import { useAiStore, type AiMessage } from '../stores/aiStore'

const MAX_HISTORY = 10

interface SystemInfo {
  os: string
  arch: string
  hostname: string
  kernel: string
  shell: string
}

interface CommandRecord {
  command: string
  result: string
}

/** Strip control chars that would garble terminal display */
function sanitizeForTerminal(text: string): string {
  return text.replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '')
}

/** Display width of a character in a terminal (1 = narrow, 2 = CJK wide) */
function charWidth(ch: string): number {
  const code = ch.codePointAt(0)
  if (code === undefined) return 1
  if (code < 0x1100) return 1
  if (code <= 0x115F) return 2
  if (code >= 0x2329 && code <= 0x232A) return 2
  if (code >= 0x2E80 && code <= 0xA4CF) return 2
  if (code >= 0xAC00 && code <= 0xD7AF) return 2
  if (code >= 0xF900 && code <= 0xFAFF) return 2
  if (code >= 0xFE10 && code <= 0xFE19) return 2
  if (code >= 0xFE30 && code <= 0xFE6F) return 2
  if (code >= 0xFF01 && code <= 0xFF60) return 2
  if (code >= 0xFFE0 && code <= 0xFFE6) return 2
  if (code >= 0x1F300 && code <= 0x1F64F) return 2
  if (code >= 0x1F900 && code <= 0x1F9FF) return 2
  if (code >= 0x20000 && code <= 0x2FFFD) return 2
  if (code >= 0x30000 && code <= 0x3FFFD) return 2
  return 1
}

/** Erase the last character of inputBuffer from the terminal display */
function eraseLastChar(inputBuffer: string, t: Terminal): string {
  if (!inputBuffer) return inputBuffer
  const last = inputBuffer.slice(-1)
  const w = charWidth(last)
  t.write('\b \b'.repeat(w))
  return inputBuffer.slice(0, -1)
}

/** Erase the entire inputBuffer from the terminal display */
function eraseLine(inputBuffer: string, t: Terminal): void {
  for (let i = inputBuffer.length - 1; i >= 0; i--) {
    const w = charWidth(inputBuffer[i])
    t.write('\b \b'.repeat(w))
  }
}

function buildSystemPrompt(systemInfo: SystemInfo, history: CommandRecord[]): string {
  const lines: string[] = [
    '你是 TndTerm 终端 AI 助手。你可以通过 execute_command 工具在用户当前的终端中执行命令。',
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

export function useAiConversation(
  getTerminal: () => Terminal | null,
  writeToBackend?: (data: string) => void,
  executeInTerminal?: (cmd: string) => Promise<string>,
) {
  const ai = useAiStore()
  const showConfirm = ref(false)
  const pendingCommand = ref('')
  const pendingToolId = ref('')
  const pendingAiMsg = ref('')
  const busy = ref(false)
  const inputBuffer = ref('')
  const messages = ref<AiMessage[]>([])
  const cancelled = ref(false)
  const pendingConfirm = ref<((value: boolean) => void) | null>(null)
  const commandHistory = ref<CommandRecord[]>([])

  let passthrough = false

  function writeAI(text: string, prefix = '[AI] ') {
    const lines = sanitizeForTerminal(text).replace(/\n/g, '\r\n')
    getTerminal()?.write(`\r\n\x1b[36m${prefix}\x1b[0m${lines}`)
  }

  function writeAITitle(text: string) {
    getTerminal()?.write(`\r\n\x1b[35m━━━ ${text} ━━━\x1b[0m`)
  }

  function writeCommand(cmd: string) {
    getTerminal()?.write(`\r\n\x1b[33m$ ${sanitizeForTerminal(cmd)}\x1b[0m`)
  }

  function writeOutput(out: string) {
    for (const line of out.split('\n')) {
      getTerminal()?.write(`\r\n\x1b[90m${sanitizeForTerminal(line)}\x1b[0m`)
    }
  }

  const LONG_OUTPUT_THRESHOLD = 5000
  const LONG_OUTPUT_TRUNCATE = 8000

  function waitForUserConfirm(message: string): Promise<boolean> {
    return new Promise(resolve => {
      pendingConfirm.value = resolve
      getTerminal()?.write(`\r\n\x1b[33m${message}\x1b[0m`)
    })
  }

  async function startConversation(userInput: string) {
    cancelled.value = false
    busy.value = true
    const systemInfo = await invoke<SystemInfo>('get_system_info')
    const systemPrompt = buildSystemPrompt(systemInfo, commandHistory.value)
    messages.value = [
      { role: 'system', content: systemPrompt },
      { role: 'user', content: userInput },
    ]
    getTerminal()?.write('\r\n')
    continueConversation()
  }

  async function continueConversation() {
    writeAITitle('AI 思考中...')

    try {
      const response = await ai.chat([...messages.value])
      if (cancelled.value) return

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

        messages.value.push({
          role: 'assistant',
          content: response.text || '',
        })

        showConfirm.value = true
      } else {
        endConversation()
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

    const t = getTerminal()

    try {
      let result: string

      if (executeInTerminal) {
        t?.write(`\r\n`)
        result = await executeInTerminal(cmd)
        if (cancelled.value) return

        // Long output: ask user before sending to AI
        if (result.length > LONG_OUTPUT_THRESHOLD) {
          const shouldContinue = await waitForUserConfirm(
            `[输出较长 (${result.length} 字符)，按 Enter 发送给 AI 分析，按 Ctrl+C 取消]`
          )
          if (!shouldContinue || cancelled.value) {
            if (!cancelled.value) writeAI('已取消分析')
            endConversation()
            return
          }
          result = result.slice(0, LONG_OUTPUT_TRUNCATE) +
            `\n\n...(输出过长，仅显示前 ${LONG_OUTPUT_TRUNCATE} 字符，共 ${result.length} 字符)`
        }
      } else {
        writeAITitle('执行命令')
        writeCommand(cmd)
        result = await ai.executeCommand(cmd)
        writeOutput(result || '(无输出)')
      }

      commandHistory.value.unshift({ command: cmd, result: result || '(无输出)' })
      if (commandHistory.value.length > MAX_HISTORY) {
        commandHistory.value = commandHistory.value.slice(0, MAX_HISTORY)
      }

      writeAITitle('AI 继续分析')
      messages.value.push({
        role: 'tool',
        content: result || '(无输出)',
        tool_call_id: toolId,
      })

      const response = await ai.continueWithResult(toolId, result || '(无输出)')
      if (cancelled.value) return

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
      } else {
        endConversation()
      }
    } catch (e: any) {
      writeAI(`执行错误: ${e}`)
      endConversation()
    }
  }

  function onCancelCommand() {
    showConfirm.value = false
    cancelled.value = true
    pendingCommand.value = ''
    pendingToolId.value = ''

    writeAI('已取消命令执行')
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
    inputBuffer.value = ''
    ai.pendingToolCall = null
    if (pendingConfirm.value) {
      pendingConfirm.value = null
    }
  }

  /** Submit a complete line from the input bar (deprecated, kept for compat) */
  function submitLine(line: string): void {
    const trimmed = line.trim()
    if (!trimmed) return

    if (!busy.value && ai.isNaturalLanguage(trimmed)) {
      if (!ai.enabled) {
        getTerminal()?.write(`\r\n\x1b[33m⚠ 请在设置 → AI 中配置 API Key 后使用 AI 助手\x1b[0m\r\n`)
        return
      }
      getTerminal()?.write('\r\n')
      startConversation(trimmed)
      return
    }

    writeToBackend?.(trimmed + '\r\n')
  }

  function interceptInput(data: string): boolean {
    if (passthrough) {
      if (data === '\r' || data === '\n') {
        passthrough = false
      }
      return false
    }

    const t = getTerminal()
    if (!t) return false

    // === Waiting for user confirmation (long output) ===
    if (pendingConfirm.value) {
      if (data === '\r' || data === '\n') {
        const resolve = pendingConfirm.value
        pendingConfirm.value = null
        resolve(true)
        return true
      }
      if (data === '\x03') {
        const resolve = pendingConfirm.value
        pendingConfirm.value = null
        // Send Ctrl+C to shell immediately so prompt resets right away
        writeToBackend?.('\x03')
        resolve(false)
        return true
      }
      return true
    }

    // === Normal mode ===
    if (data === '\r' || data === '\n') {
      const line = inputBuffer.value
      inputBuffer.value = ''

      if (!line.trim()) {
        if (busy.value) return true
        return false
      }

      if (!busy.value && ai.isNaturalLanguage(line)) {
        if (!ai.enabled) {
          t.write(`\r\n\x1b[33m⚠ 请在设置 → AI 中配置 API Key 后使用 AI 助手\x1b[0m\r\n`)
          writeToBackend?.('\r')
          return true
        }
        t.write('\r\n')
        startConversation(line)
        return true
      }

      eraseLine(line, t)
      writeToBackend?.(line + '\r')
      return true
    }

    if (data === '\x7f') {
      if (inputBuffer.value.length > 0) {
        inputBuffer.value = eraseLastChar(inputBuffer.value, t)
      }
      return true
    }

    if (data === '\t') {
      const line = inputBuffer.value
      if (line) {
        eraseLine(line, t)
        inputBuffer.value = ''
        writeToBackend?.(line + '\t')
        passthrough = true
      } else {
        writeToBackend?.('\t')
      }
      return true
    }

    // Ctrl+C during AI conversation → cancel
    if (data === '\x03' && busy.value) {
      cancelled.value = true
      t.write(`\r\n\x1b[35m━━━ AI 对话已取消 ━━━\x1b[0m`)
      // Send Ctrl+C to shell immediately
      writeToBackend?.('\x03')
      endConversation()
      return true
    }

    // Capture printable text (IME may commit multiple chars at once)
    if (/^[\x20-\x7e\u00a0-\uffff]+$/.test(data)) {
      inputBuffer.value += data
      t.write(data)
      return true
    }

    // Single control char → clear buffer and pass through
    if (data.length === 1 && data.charCodeAt(0) < 32) {
      inputBuffer.value = ''
      return false
    }

    inputBuffer.value = ''
    return false
  }

  function clearInputBuffer() {
    inputBuffer.value = ''
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
    busy,
    cancelled,
    interceptInput,
    onConfirmCommand,
    onCancelCommand,
    onModifyCommand,
    resetConversation,
    writeAI,
    startConversation,
    commandHistory,
    clearInputBuffer,
    submitLine,
  }
}
