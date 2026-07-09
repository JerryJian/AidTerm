import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface AiConfig {
  api_type: string
  api_key: string
  model: string
  base_url: string
}

export interface ApiTypePreset {
  label: string
  defaultBaseUrl: string
  defaultModel: string
  presets?: Record<string, { model: string; base_url: string }>
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
      if (raw.api_type) return raw
      const oldMap: Record<string, string> = { openai: 'openai-compatible', deepseek: 'openai-compatible', dashscope: 'openai-compatible', ollama: 'ollama', anthropic: 'anthropic' }
      if (raw.provider && oldMap[raw.provider]) {
        raw.api_type = oldMap[raw.provider]
        delete raw.provider
        return raw
      }
      return {
        api_type: 'openai-compatible',
        api_key: '',
        model: 'gpt-4o',
        base_url: 'https://api.openai.com/v1',
      }
    } catch {
      return {
        api_type: 'openai-compatible',
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

  const apiTypes: Record<string, ApiTypePreset> = {
    'openai-compatible': {
      label: 'OpenAI Compatible',
      defaultBaseUrl: 'https://api.openai.com/v1',
      defaultModel: 'gpt-4o',
      presets: {
        openai: { model: 'gpt-4o', base_url: 'https://api.openai.com/v1' },
        deepseek: { model: 'deepseek-chat', base_url: 'https://api.deepseek.com/v1' },
        dashscope: { model: 'qwen-plus', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1' },
      },
    },
    ollama: {
      label: 'Ollama',
      defaultBaseUrl: 'http://localhost:11434',
      defaultModel: 'llama3',
    },
    anthropic: {
      label: 'Anthropic',
      defaultBaseUrl: 'https://api.anthropic.com',
      defaultModel: 'claude-sonnet-4-20250514',
    },
  }

  function setApiType(name: string) {
    const t = apiTypes[name]
    if (t) {
      config.value.api_type = name
      config.value.model = t.defaultModel
      config.value.base_url = t.defaultBaseUrl
      saveConfig()
    }
    modelList.value = []
  }

  function applyPreset(name: string) {
    const t = apiTypes[config.value.api_type]
    if (t?.presets) {
      const p = t.presets[name]
      if (p) {
        config.value.model = p.model
        config.value.base_url = p.base_url
        saveConfig()
      }
    }
    modelList.value = []
  }

  async function fetchModels() {
    loadingModels.value = true
    modelList.value = []
    try {
      modelList.value = await invoke<string[]>('fetch_ai_models', {
        apiType: config.value.api_type,
        baseUrl: config.value.base_url,
        apiKey: config.value.api_key,
      })
    } catch (e: any) {
      console.error('Failed to fetch models:', e)
    } finally {
      loadingModels.value = false
    }
  }

  function isNaturalLanguage(input: string): boolean {
    const trimmed = input.trim().toLowerCase()

    // Common command prefixes - treat as command
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

    // Single word commands
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

    // Contains Chinese characters → natural language
    if (/[\u4e00-\u9fff]/.test(trimmed)) {
      return true
    }

    // Questions or requests in English → natural language
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

  async function chat(messages: AiMessage[]): Promise<AiResponse> {
    thinking.value = true
    try {
      const response = await invoke<AiResponse>('ai_chat', {
        sessionId: activeSessionId.value || 'default',
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

  async function continueWithResult(toolCallId: string, result: string): Promise<AiResponse> {
    thinking.value = true
    pendingToolCall.value = null
    try {
      const response = await invoke<AiResponse>('ai_continue', {
        sessionId: activeSessionId.value || 'default',
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

  function clearHistory() {
    invoke('ai_clear_history', { sessionId: activeSessionId.value || 'default' })
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
    apiTypes,
    updateConfig,
    setApiType,
    applyPreset,
    saveConfig,
    isNaturalLanguage,
    fetchModels,
    chat,
    executeCommand,
    continueWithResult,
    clearHistory,
  }
})
