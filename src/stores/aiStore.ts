import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface AiConfig {
  provider: string
  api_key: string
  model: string
  base_url: string
  provider_id?: string
  mode?: string
  prefix?: string
  auto_execute?: boolean
}

export interface ProviderOption {
  id: string
  label: string
  provider: string
  model: string
  baseUrl: string
}

export interface AiMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string
  tool_call_id?: string
  tool_calls?: ToolCall[]
}

export interface ToolCall {
  id: string
  function: {
    name: string
    arguments: string
  }
}

export interface AiResponse {
  text: string | null
  tool_calls: ToolCall[]
}

export const useAiStore = defineStore('ai', () => {
  const config = ref<AiConfig>(loadConfig())
  const enabled = ref(!!config.value.api_key)
  const activeSessionId = ref<string | null>(null)
  const pendingToolCall = ref<ToolCall | null>(null)
  const thinking = ref(false)
  const autoMode = ref(false)
  const modelList = ref<string[]>([])
  const loadingModels = ref(false)

  function loadConfig(): AiConfig {
    try {
      const raw = JSON.parse(localStorage.getItem('aidterm_ai_config') || '{}')
      if (raw.provider) return raw
      if (raw.api_type) {
        raw.provider = raw.api_type
        delete raw.api_type
        return raw
      }
      const providerNameMap: Record<string, string> = { openai: 'openai-compatible', deepseek: 'openai-compatible', dashscope: 'openai-compatible', ollama: 'ollama', anthropic: 'anthropic' }
      if (raw.provider && providerNameMap[raw.provider]) {
        raw.provider = providerNameMap[raw.provider]
        return raw
      }
      return {
        provider: 'openai-compatible',
        api_key: '',
        model: 'gpt-4o',
        base_url: 'https://api.openai.com/v1',
      }
    } catch {
      return {
        provider: 'openai-compatible',
        api_key: '',
        model: 'gpt-4o',
        base_url: 'https://api.openai.com/v1',
      }
    }
  }

  function saveConfig() {
    localStorage.setItem('aidterm_ai_config', JSON.stringify(config.value))
    enabled.value = !!config.value.api_key
  }

  function updateConfig(cfg: Partial<AiConfig>) {
    Object.assign(config.value, cfg)
    saveConfig()
  }

  const providerList = ref<ProviderOption[]>([
    { id: 'openai', label: 'OpenAI', provider: 'openai-compatible', model: 'gpt-4o', baseUrl: 'https://api.openai.com/v1' },
    { id: 'deepseek', label: 'DeepSeek', provider: 'openai-compatible', model: 'deepseek-chat', baseUrl: 'https://api.deepseek.com/v1' },
    { id: 'dashscope', label: 'DashScope', provider: 'openai-compatible', model: 'qwen-plus', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1' },
    { id: 'openai-compatible', label: 'OpenAI Compatible', provider: 'openai-compatible', model: 'gpt-4o', baseUrl: 'https://api.openai.com/v1' },
    { id: 'ollama', label: 'Ollama', provider: 'ollama', model: 'llama3', baseUrl: 'http://localhost:11434' },
    { id: 'anthropic', label: 'Anthropic', provider: 'anthropic', model: 'claude-sonnet-4-20250514', baseUrl: 'https://api.anthropic.com' },
  ])

  const currentProviderId = computed(() => {
    const c = config.value
    if (c.provider_id) return c.provider_id
    for (const p of providerList.value) {
      if (p.provider === c.provider && p.model === c.model && p.baseUrl === c.base_url) {
        return p.id
      }
    }
    return 'openai'
  })

  function setProvider(id: string) {
    const p = providerList.value.find(x => x.id === id)
    if (p) {
      config.value.provider = p.provider
      config.value.model = p.model
      config.value.base_url = p.baseUrl
      config.value.provider_id = id
      saveConfig()
    }
    modelList.value = []
  }

  async function fetchModels() {
    loadingModels.value = true
    modelList.value = []
    try {
      modelList.value = await invoke<string[]>('fetch_ai_models', {
        provider: config.value.provider,
        baseUrl: config.value.base_url,
        apiKey: config.value.api_key,
      })
    } catch (e: any) {
      console.error('Failed to fetch models:', e)
    } finally {
      loadingModels.value = false
    }
  }

  function getPrefixes(cfg: AiConfig): string[] {
    const raw = cfg.prefix || ':'
    return raw.split('')
  }

  function isNaturalLanguage(input: string): boolean {
    const cfg = config.value

    // Keybinding mode: Enter never triggers AI
    if (cfg.mode === 'keybinding') {
      return false
    }

    // Prefix mode: only trigger on prefix
    if (cfg.mode === 'prefix') {
      const trimmed = input.trim()
      const prefixes = getPrefixes(cfg)
      return prefixes.some(p => trimmed.startsWith(p))
    }

    // Auto mode: existing heuristic
    const trimmed = input.trim().toLowerCase()

    const commandPrefixes = [
      'cd ', 'ls ', 'cat ', 'echo ', 'rm ', 'cp ', 'mv ', 'mkdir ',
      'grep ', 'find ', 'chmod ', 'chown ', 'ps ', 'kill ', 'top ',
      'df ', 'du ', 'free ', 'uname ', 'whoami ', 'id ', 'pwd ',
      'git ', 'npm ', 'yarn ', 'pnpm ', 'cargo ', 'pip ', 'pip3 ',
      'sudo ', 'apt ', 'yum ', 'dnf ', 'brew ', 'docker ',
      './', '/', 'make ', 'curl ', 'wget ', 'ssh ', 'telnet ',
      'ping ', 'traceroute ', 'netstat ', 'ss ', 'ip ',
      'export ', 'alias ', 'source ', '. ',
    ]

    if (commandPrefixes.some(p => trimmed.startsWith(p))) {
      return false
    }

    const commonCommands = new Set([
      'ls', 'cat', 'echo', 'pwd', 'cd', 'clear', 'exit', 'help',
      'date', 'whoami', 'id', 'uname', 'uptime', 'env', 'which',
      'ps', 'top', 'htop', 'df', 'du', 'free', 'neofetch',
      'git', 'npm', 'yarn', 'node', 'python', 'python3', 'go',
      'cargo', 'rustc', 'gcc', 'g++', 'clang', 'make', 'cmake',
      'docker', 'kubectl', 'aws', 'gcloud', 'az',
      'ssh', 'scp', 'sftp', 'rsync', 'curl', 'wget',
      'ping', 'traceroute', 'nslookup', 'dig', 'nmap',
      'vim', 'nano', 'emacs', 'code', 'code-insiders',
    ])

    if (commonCommands.has(trimmed) || commonCommands.has(trimmed.split(/\s+/)[0])) {
      return false
    }

    if (/[\u4e00-\u9fff]/.test(trimmed)) {
      return true
    }

    const nlPatterns = [
      /^(what|how|why|when|where|who|which|can|could|would|should|do|does|is|are|show|tell|list|find|explain|help|check|fix|install|create|setup|configure|describe|summarize|analyze|compare|generate|write|make|run|start|stop|restart|update|upgrade|remove|delete|add|search|grep|count|sort|filter|convert|download|upload|backup|restore|monitor|watch|follow|tail|head|less|more)/i,
      /[?？]$/,
      /^(please|pls|can you|could you|would you|i want|i need|i'd like|i'm trying|how do|how to|what is|what are|show me|tell me)/i,
    ]

    if (nlPatterns.some(p => p.test(trimmed))) {
      return true
    }

    return false
  }

  async function chat(messages: AiMessage[], sessionId?: string): Promise<AiResponse> {
    thinking.value = true
    try {
      const response = await invoke<AiResponse>('ai_chat', {
        sessionId: sessionId || activeSessionId.value || 'default',
        messages,
        config: config.value,
      })
      if (response.tool_calls && response.tool_calls.length > 0) {
        pendingToolCall.value = response.tool_calls[0]
      }
      return response
    } finally {
      thinking.value = false
    }
  }

  async function executeCommand(command: string): Promise<string> {
    return await invoke<string>('ai_execute', { command })
  }

  async function continueWithResult(toolCallId: string, result: string, sessionId?: string): Promise<AiResponse> {
    thinking.value = true
    pendingToolCall.value = null
    try {
      const response = await invoke<AiResponse>('ai_continue', {
        sessionId: sessionId || activeSessionId.value || 'default',
        toolCallId,
        toolResult: result,
        config: config.value,
      })
      if (response.tool_calls && response.tool_calls.length > 0) {
        pendingToolCall.value = response.tool_calls[0]
      }
      return response
    } finally {
      thinking.value = false
    }
  }

  function clearHistory(sessionId?: string) {
    invoke('ai_clear_history', { sessionId: sessionId || activeSessionId.value || 'default' })
    pendingToolCall.value = null
  }

  return {
    config,
    enabled,
    activeSessionId,
    pendingToolCall,
    thinking,
    autoMode,
    modelList,
    loadingModels,
    providerList,
    currentProviderId,
    updateConfig,
    setProvider,
    saveConfig,
    isNaturalLanguage,
    fetchModels,
    chat,
    executeCommand,
    continueWithResult,
    clearHistory,
  }
})
