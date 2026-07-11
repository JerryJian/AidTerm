import { ref, reactive, computed } from 'vue'
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
  dangerous?: boolean
  autoExecStatus?: 'executing' | 'completed'
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
    '=== 命令安全分类规则 ===',
    '当你建议执行命令时，必须在回复文本的第一行以 [SAFE] 或 [DANGER] 开头标注命令的安全等级：',
    '',
    '[SAFE] — 安全命令（仅查询/读取，不会改变系统状态）：',
    '  ls, cat, head, tail, wc, grep, find, echo, pwd, whoami, id, uname, date, uptime,',
    '  df, du, free, ps, top, htop, env, which, type, file, stat, wc, diff,',
    '  git status, git log, git diff, git show, git branch,',
    '  ping, traceroute, nslookup, dig, curl (GET), wget (download),',
    '  docker ps, docker images, docker logs, kubectl get, kubectl describe',
    '',
    '[DANGER] — 危险命令（会改变系统/文件：增、删、改）：',
    '  rm, mv, cp, chmod, chown, chgrp, mkdir, rmdir, touch, ln,',
    '  echo >, echo >>, tee, sed -i, awk (写入),',
    '  apt, apt-get, yum, dnf, brew, pip install, npm install, cargo install,',
    '  systemctl, service, kill, killall, pkill, reboot, shutdown,',
    '  docker rm, docker rmi, docker run, docker exec, docker stop,',
     '  kubectl delete, kubectl apply, kubectl exec,',
    '  git commit, git push, git merge, git reset, git checkout, git stash,',
    '  ssh (执行远程命令), scp, rsync, dd, mkfs, fdisk, mount,',
    '  sudo (任何命令), su, passwd, useradd, userdel,',
    '  export (永久修改环境变量), alias, crontab,',
    '  curl -X POST/PUT/DELETE, wget --post,',
    '  任何涉及文件内容修改、权限变更、服务管理、包安装/卸载的命令',
    '',
    '格式示例：',
    '[SAFE] 我来查看当前目录的文件列表。',
    '[DANGER] 我将删除 /tmp 下的临时文件，请确认。',
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
  return text
    .replace(/\x1b\[[\d;?]*[a-zA-Z]/g, '')
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    .replace(/\r/g, '')
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

function cleanCommandOutput(output: string, cmd: string, prompt?: string): string {
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
    let last = trimmed[lastIdx].trimEnd()

    if (prompt && prompt.trimEnd() && last.endsWith(prompt.trimEnd())) {
      last = last.slice(0, -prompt.trimEnd().length)
    } else if (/^[$#>%]$/.test(last)) {
      last = ''
    }

    if (last.trim().length === 0) {
      trimmed.pop()
    } else {
      trimmed[lastIdx] = last
    }
  }

  return trimmed.join('\n').trim()
}

function parseDangerTag(text: string): boolean | null {
  const match = text.match(/^\[SAFE\]|^\[DANGER\]/i)
  if (!match) return null
  return match[0].toUpperCase() === '[DANGER]'
}

function stripDangerTag(text: string): string {
  return text.replace(/^\[(?:SAFE|DANGER)\]\s*/i, '')
}

const DANGEROUS_CMD_PATTERNS = [
  /\brm\b/, /\bmv\b/, /\bchmod\b/, /\bchown\b/, /\bchgrp\b/,
  /\bmkdir\b/, /\brmdir\b/, /\btouch\b/, /\bln\b/,
  /\btee\b/, /\bsed\b.*-i/, /\bawk\b/,
  /\bapt(-get)?\b/, /\byum\b/, /\bdnf\b/, /\bbrew\b/,
  /\bpip3?\s+install\b/, /\bnpm\s+(i|install)\b/, /\bcargo\s+install\b/,
  /\bsystemctl\b/, /\bservice\b/, /\bkill\b/, /\bkillall\b/, /\bpkill\b/,
  /\breboot\b/, /\bshutdown\b/,
  /\bdocker\s+(rm|rmi|run|exec|stop|kill|restart)\b/,
  /\bkubectl\s+(delete|apply|exec|patch)\b/,
  /\bgit\s+(commit|push|merge|reset|checkout|stash|rebase|branch\s+-[dD])\b/,
  /\bscp\b/, /\brsync\b/, /\bdd\b/, /\bmkfs\b/, /\bfdisk\b/,
  /\bsudo\b/, /\bsu\b/, /\bpasswd\b/, /\buseradd\b/, /\buserdel\b/,
  /\bcurl\b.*-X\s*(POST|PUT|DELETE|PATCH)/i,
  /\b>\s*\S/, /\b>>\s*\S/,
  /\bmount\b/, /\bumount\b/,
]

function isDangerousByHeuristic(cmd: string): boolean {
  const trimmed = cmd.trim()
  return DANGEROUS_CMD_PATTERNS.some(p => p.test(trimmed))
}

export function useAiConversation(
  getTerminal: () => any | null,
  writeToBackend?: (data: string) => void,
  rawOnOutput?: (cb: (data: string) => void) => Promise<(() => void) | null | undefined>,
  aiSessionId?: string,
) {
  const ai = useAiStore()
  const showConfirm = ref(false)
  const pendingCommand = ref('')
  const pendingToolId = ref('')
  const pendingAiMsg = ref('')
  const busy = ref(false)
  const cancelled = ref(false)
  const autoExecute = computed(() => ai.config.auto_execute ?? false)
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
          const result = cleanCommandOutput(output, cmd, savedPrompt.value)
          resolve(result)
          return
        }

        if (Date.now() - t0 > PROMPT_TIMEOUT) {
          resolved = true
          cleanup()
          const result = cleanCommandOutput(output, cmd, savedPrompt.value)
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

  function removeLastThinking() {
    for (let i = conversationMessages.length - 1; i >= 0; i--) {
      if (conversationMessages[i].role === 'thinking') {
        conversationMessages.splice(i, 1)
        return
      }
    }
  }

  function updateLastCommandAutoExecStatus(toolId: string, status: 'executing' | 'completed') {
    for (let i = conversationMessages.length - 1; i >= 0; i--) {
      if (conversationMessages[i].role === 'command' && conversationMessages[i].toolCallId === toolId) {
        conversationMessages[i].autoExecStatus = status
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
    const historyMsgs = buildHistoryMessages(3)
    messages.value = [
      { role: 'system', content: systemPrompt },
      ...historyMsgs,
      { role: 'user', content: userInput },
    ]

    addConversationMessage({ role: 'thinking', content: '' })
    continueConversation()
  }

  function determineDanger(aiText: string, cmd: string): boolean {
    const aiTag = parseDangerTag(aiText)
    if (aiTag !== null) return aiTag
    return isDangerousByHeuristic(cmd)
  }

  function cleanAiText(text: string): string {
    return stripDangerTag(text)
  }

  function buildHistoryMessages(maxTurns: number = 3): AiMessage[] {
    const userIndices: number[] = []
    for (let i = 0; i < conversationMessages.length; i++) {
      if (conversationMessages[i].role === 'user') userIndices.push(i)
    }

    const startIdx = userIndices.length > maxTurns
      ? userIndices[userIndices.length - maxTurns]
      : userIndices.length > 0 ? userIndices[0] : conversationMessages.length

    const slice = conversationMessages.slice(startIdx)
    const result: AiMessage[] = []

    let pendingAssistant: AiMessage | null = null

    for (const msg of slice) {
      if (msg.role === 'user') {
        result.push({ role: 'user', content: msg.content })
        pendingAssistant = null
      } else if (msg.role === 'assistant') {
        if (!pendingAssistant) {
          pendingAssistant = { role: 'assistant', content: msg.content }
          result.push(pendingAssistant)
        } else {
          pendingAssistant.content = msg.content
        }
      } else if (msg.role === 'command' && msg.toolCallId) {
        if (!pendingAssistant) {
          pendingAssistant = { role: 'assistant', content: '' }
          result.push(pendingAssistant)
        }
        if (!pendingAssistant.tool_calls) pendingAssistant.tool_calls = []
        pendingAssistant.tool_calls.push({
          id: msg.toolCallId,
          function: {
            name: 'execute_command',
            arguments: JSON.stringify({ command: msg.command || '' }),
          },
        })
      } else if (msg.role === 'result' && msg.toolCallId) {
        result.push({
          role: 'tool',
          content: msg.content,
          tool_call_id: msg.toolCallId,
        })
        pendingAssistant = null
      } else if (msg.role === 'error' && msg.toolCallId) {
        result.push({
          role: 'tool',
          content: msg.content,
          tool_call_id: msg.toolCallId,
        })
        pendingAssistant = null
      }
    }

    return result
  }

  async function continueConversation() {
    try {
      const response = await ai.chat([...messages.value], aiSessionId)
      if (cancelled.value) return
      removeLastThinking()

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        const cmd = args.command || ''
        pendingCommand.value = cmd
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''

        const aiText = response.text || ''
        const dangerous = determineDanger(aiText, cmd)
        const cleanText = cleanAiText(aiText)

        messages.value.push({
          role: 'assistant',
          content: aiText,
        })

        if (cleanText) {
          addConversationMessage({ role: 'assistant', content: cleanText })
        }

        addConversationMessage({
          role: 'command',
          content: cmd,
          command: cmd,
          toolCallId: tc.id,
          dangerous,
        })

        if (!dangerous && ai.config.auto_execute) {
          updateLastCommandAutoExecStatus(tc.id, 'executing')
          await onConfirmCommand()
        } else {
          showConfirm.value = true
        }
      } else if (response.text) {
        const cleanText = cleanAiText(response.text)
        addConversationMessage({ role: 'assistant', content: cleanText || response.text })
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
      updateLastCommandAutoExecStatus(toolId, 'completed')

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

      const response = await ai.continueWithResult(toolId, resultForAI, aiSessionId)
      if (cancelled.value) return
      removeLastThinking()

      if (response.text) {
        messages.value.push({ role: 'assistant', content: response.text })
      }

      if (response.tool_calls && response.tool_calls.length > 0) {
        const tc = response.tool_calls[0]
        const args = JSON.parse(tc.function.arguments)
        const nextCmd = args.command || ''
        pendingCommand.value = nextCmd
        pendingToolId.value = tc.id
        pendingAiMsg.value = response.text || ''

        const aiText = response.text || ''
        const dangerous = determineDanger(aiText, nextCmd)
        const cleanText = cleanAiText(aiText)

        if (cleanText) {
          addConversationMessage({ role: 'assistant', content: cleanText })
        }

        addConversationMessage({
          role: 'command',
          content: nextCmd,
          command: nextCmd,
          toolCallId: tc.id,
          dangerous,
        })

        if (!dangerous && ai.config.auto_execute) {
          updateLastCommandAutoExecStatus(tc.id, 'executing')
          await onConfirmCommand()
        } else {
          showConfirm.value = true
        }
      } else if (response.text) {
        addConversationMessage({ role: 'assistant', content: response.text })
        endConversation()
      } else {
        endConversation()
      }
    } catch (e: any) {
      updateLastCommandAutoExecStatus(toolId, 'completed')
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
    autoExecute,
  }
}
