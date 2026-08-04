import { ref, reactive, computed } from 'vue'
import { invoke } from '@/api'
import type { Terminal } from '@xterm/xterm'
import { useAiStore, type AiMessage, type ToolCall, type AiResponse } from '../stores/aiStore'
import { useTerminalStore } from '../stores/terminal'
import { getLeafBinding } from './terminalAiRegistry'
import type { SystemInfo } from '../types'

const MAX_HISTORY = 10
const PAGE_SIZE = 10000
const MAX_PAGES_PER_OUTPUT = 200
const MAX_OUTPUTS = 10

interface CommandRecord {
  command: string
  result: string
  outputId?: string
  pageCount?: number
}

function paginate(text: string, maxChars: number): string[] {
  if (!text) return ['']
  const pages: string[] = []
  let current = ''
  for (const line of text.split('\n')) {
    if (current !== '' && current.length + 1 + line.length > maxChars) {
      pages.push(current)
      current = line
    } else {
      current = current === '' ? line : current + '\n' + line
    }
  }
  if (current !== '') pages.push(current)
  return pages
}

export interface ConversationMessage {
  role: 'user' | 'assistant' | 'command' | 'result' | 'error' | 'thinking'
  content: string
  command?: string
  toolCallId?: string
  dangerous?: boolean
  autoExecStatus?: 'executing' | 'completed'
  timestamp?: number
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
      if (h.outputId) {
        const pageInfo = h.pageCount && h.pageCount > 1
          ? ` 共 ${h.pageCount} 页，可通过 read_output_page(output_id="${h.outputId}", page) 读取其他页`
          : ''
        lines.push(`[输出 #${h.outputId}${pageInfo}]`)
      }
      const resultPreview = h.result.length > 200 ? h.result.slice(0, 200) + '...(已截断)' : h.result
      if (resultPreview) lines.push(resultPreview)
    }
  }

  lines.push(
    '',
    '=== 输出分页规则 ===',
    '执行命令后，如果输出较长，工具结果会以"共 N 页（每页约 10000 字符）"的形式返回，其中只包含第 1 页。',
    '此时你必须调用 read_output_page(output_id, page) 按需读取其他页码，直到获得回答用户问题所需的全部信息，再给出最终回答。',
    '不要只凭第 1 页就下结论。若某个输出已不可用，工具会返回错误提示，请按提示重新执行命令。',
    '命令输出只在内存中保留最近 10 份（每份最多 200 页），对话重置或应用退出后清空。',
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

export interface AiTerminalBinding {
  getTerminal: () => Terminal | null
  writeToBackend?: (data: string) => void
  rawOnOutput?: (cb: (data: string) => void) => Promise<(() => void) | null | undefined>
}

export function useAiConversation(
  getTerminal: () => Terminal | null,
  writeToBackend?: (data: string) => void,
  rawOnOutput?: (cb: (data: string) => void) => Promise<(() => void) | null | undefined>,
  aiSessionId?: string,
) {
  const ai = useAiStore()
  const binding = ref<AiTerminalBinding | null>({ getTerminal, writeToBackend, rawOnOutput })
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

  const waitingForCommand = ref(false)
  let resolveWait: ((result: string) => void) | null = null
  let waitCmd = ''
  let waitRaw = ''

  const outputPages = new Map<string, string[]>()

  function storeOutput(id: string, text: string): string[] {
    const all = paginate(text, PAGE_SIZE)
    const pages = all.length > MAX_PAGES_PER_OUTPUT
      ? [...all.slice(0, MAX_PAGES_PER_OUTPUT), `...(输出共 ${all.length} 页，仅保留前 ${MAX_PAGES_PER_OUTPUT} 页，其余内容未记录)`]
      : all
    outputPages.delete(id)
    outputPages.set(id, pages)
    while (outputPages.size > MAX_OUTPUTS) {
      const oldest = outputPages.keys().next().value as string | undefined
      if (oldest === undefined) break
      outputPages.delete(oldest)
    }
    return pages
  }

  function getPageText(id: string, page: number): { ok: true; content: string } | { ok: false; error: string } {
    const pages = outputPages.get(id)
    if (!pages) {
      const ids = [...outputPages.keys()].join(', ')
      return {
        ok: false,
        error: `[错误] 输出 #${id} 已不可用。当前可用的输出: ${ids || '无'}。如需该数据，请重新执行原命令。`,
      }
    }
    if (!Number.isInteger(page) || page < 1 || page > pages.length) {
      return { ok: false, error: `[错误] 页码 ${page} 超出范围（1-${pages.length}）。输出 #${id} 共 ${pages.length} 页。` }
    }
    return { ok: true, content: `[第 ${page}/${pages.length} 页，输出 #${id}]\n${pages[page - 1]}` }
  }

  const conversationMessages = reactive<ConversationMessage[]>([])

  function bindTerminal(b: AiTerminalBinding | null) {
    binding.value = b
  }

  function bindToSelectedPane() {
    const termStore = useTerminalStore()
    const id = termStore.selectedPaneId ?? termStore.activeLeafId
    const b = getLeafBinding(id)
    if (b) binding.value = b
  }

  function detectSavedPrompt() {
    const t = binding.value?.getTerminal()
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
    const b = binding.value
    if (!b || !b.rawOnOutput || !b.writeToBackend) {
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

      const finish = (result: string) => {
        if (resolved) return
        resolved = true
        cleanup()
        resolveWait = null
        waitCmd = ''
        waitRaw = ''
        waitingForCommand.value = false
        resolve(result)
      }

      waitCmd = cmd
      waitRaw = ''
      waitingForCommand.value = true
      resolveWait = finish

      b.rawOnOutput!((data: string) => {
        output += data
        waitRaw = output
      }).then((un: (() => void) | null | undefined) => {
        if (un) unsub = un
      })

      b.writeToBackend!(cmd + '\r')

      const check = () => {
        if (resolved) return
        if (detectPromptInOutput(output, savedPrompt.value)) {
          finish(cleanCommandOutput(output, cmd, savedPrompt.value))
          return
        }
        setTimeout(check, 100)
      }

      setTimeout(check, 300)
    })
  }

  function stopWaitingForCommand() {
    if (resolveWait) {
      binding.value?.writeToBackend?.('\x03')
      resolveWait(cleanCommandOutput(waitRaw, waitCmd, savedPrompt.value) || waitRaw)
    }
  }

  function addConversationMessage(msg: ConversationMessage) {
    conversationMessages.push({ ...msg, timestamp: Date.now() })
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

  async function resolveSystemInfo(): Promise<SystemInfo> {
    const termStore = useTerminalStore()
    const leafId = termStore.selectedPaneId ?? termStore.activeLeafId
    const leaf = leafId ? termStore.findTab(leafId) : null

    // Commands run in the selected pane's shell. For SSH sessions report the
    // remote machine info instead of the local one, falling back to local
    // info when the remote cannot be queried (not yet connected, etc.).
    if (leaf?.session?.type === 'ssh' && leaf.session.id) {
      try {
        const info = await invoke<SystemInfo>('get_remote_system_info', { sessionId: leaf.session.id })
        termStore.updateSystemInfo(leaf.id, info)
        return info
      } catch {
        // fall back to local info below
      }
    }

    const info = await invoke<SystemInfo>('get_system_info')

    // get_system_info reads the SHELL/ComSpec env vars of the app process, so
    // on Windows it always reports cmd.exe regardless of the actual session.
    // For local sessions use the shell that was actually spawned instead.
    if (leaf?.session?.type === 'local' && leaf.session.command) {
      info.shell = leaf.session.command
    }

    if (leaf) termStore.updateSystemInfo(leaf.id, info)
    return info
  }

  async function startConversation(userInput: string) {
    cancelled.value = false
    busy.value = true

    detectSavedPrompt()

    // Build history before recording the current question, otherwise the
    // conversation history (conversationMessages) already contains it and
    // the explicit user message below would send it twice.
    const historyMsgs = buildHistoryMessages(3)

    addConversationMessage({ role: 'user', content: userInput })

    const systemInfo = await resolveSystemInfo()

    const systemPrompt = buildSystemPrompt(systemInfo, commandHistory.value)
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
      await handleAiResponse(response)
    } catch (e: unknown) {
      removeLastThinking()
      addConversationMessage({ role: 'error', content: `错误: ${e}` })
      endConversation()
    }
  }

  async function continueWithToolResult(toolId: string, result: string) {
    messages.value.push({ role: 'tool', content: result, tool_call_id: toolId })
    const response = await ai.continueWithResult(toolId, result, aiSessionId)
    if (cancelled.value) return
    removeLastThinking()
    await handleAiResponse(response)
  }

  async function handleAiResponse(response: AiResponse) {
    if (response.text) {
      messages.value.push({ role: 'assistant', content: response.text })
    }

    if (response.tool_calls && response.tool_calls.length > 0) {
      const tc = response.tool_calls[0]
      try {
        if (tc.function.name === 'read_output_page') {
          await handleReadOutputPage(tc)
        } else {
          await handleExecuteCommandTool(tc, response.text || '')
        }
      } catch (e: unknown) {
        removeLastThinking()
        addConversationMessage({ role: 'error', content: `工具处理错误: ${e}` })
        endConversation()
      }
    } else if (response.text) {
      const cleanText = cleanAiText(response.text)
      addConversationMessage({ role: 'assistant', content: cleanText || response.text })
      endConversation()
    } else {
      endConversation()
    }
  }

  async function handleExecuteCommandTool(tc: ToolCall, aiText: string) {
    const args = JSON.parse(tc.function.arguments)
    const cmd = args.command || ''
    pendingCommand.value = cmd
    pendingToolId.value = tc.id
    pendingAiMsg.value = aiText

    const dangerous = determineDanger(aiText, cmd)
    const cleanText = cleanAiText(aiText)

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
  }

  async function handleReadOutputPage(tc: ToolCall) {
    let content: string
    try {
      const args = JSON.parse(tc.function.arguments)
      const res = getPageText(String(args.output_id ?? ''), Number(args.page))
      content = res.ok ? res.content : res.error
    } catch {
      content = '[错误] read_output_page 参数解析失败，需要 output_id(string) 与 page(integer)'
    }
    addConversationMessage({ role: 'thinking', content: `读取输出页...` })
    await continueWithToolResult(tc.id, content)
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

      const pages = storeOutput(toolId, displayResult)
      const resultForAI = pages.length > 1
        ? `[命令输出 #${toolId} 共 ${pages.length} 页（每页约 ${PAGE_SIZE} 字符）。以下为第 1 页：\n${pages[0]}\n\n如需查看更多，请调用 read_output_page 工具，参数 output_id="${toolId}"、page=<页码>（1-${pages.length}）。]`
        : displayResult

      commandHistory.value.unshift({ command: cmd, result: displayResult, outputId: toolId, pageCount: pages.length })
      if (commandHistory.value.length > MAX_HISTORY) {
        commandHistory.value = commandHistory.value.slice(0, MAX_HISTORY)
      }

      addConversationMessage({ role: 'thinking', content: 'AI 分析中...' })

      await continueWithToolResult(toolId, resultForAI)
    } catch (e: unknown) {
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
    binding.value?.writeToBackend?.('\x03')
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

  function cancelConversation() {
    cancelled.value = true
    showConfirm.value = false
    pendingCommand.value = ''
    pendingToolId.value = ''
    if (resolveWait) resolveWait('')
    removeLastThinking()
    endConversation()
    addConversationMessage({ role: 'error', content: '已取消本次问答' })
  }

  function submitInput(text: string) {
    const trimmed = text.trim()
    if (!trimmed) return

    if (!ai.enabled) {
      addConversationMessage({ role: 'error', content: '请在设置 → AI 中配置 API Key 后使用 AI 助手' })
      return
    }

    bindToSelectedPane()
    startConversation(trimmed)
  }

  function resetConversation() {
    ai.clearHistory(aiSessionId)
    outputPages.clear()
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
    bindTerminal,
    onConfirmCommand,
    onCancelCommand,
    onModifyCommand,
    stopWaitingForCommand,
    waitingForCommand,
    cancelConversation,
    resetConversation,
    startConversation,
    commandHistory,
    submitInput,
    forceAIInput,
    autoExecute,
  }
}
