<script setup lang="ts">
import { ref } from 'vue'
import { useAiStore } from '../../stores/aiStore'

const ai = useAiStore()
const emit = defineEmits<{ close: [] }>()
const showAiKey = ref(false)
</script>

<template>
  <div class="ai-panel">
    <div class="panel-header">
      <span class="panel-title">🤖 AI 配置</span>
      <button class="panel-close" @click="emit('close')">✕</button>
    </div>
    <div class="panel-body">
      <div class="section">
        <h3>提供商</h3>
        <div class="field">
          <select
            :value="ai.currentProviderId"
            @change="(e: any) => ai.setProvider(e.target.value)"
            class="input"
          >
            <option v-for="p in ai.providerList" :key="p.id" :value="p.id">{{ p.label }}</option>
          </select>
        </div>
      </div>

      <div class="section">
        <h3>服务地址</h3>
        <div class="field">
          <label>Base URL</label>
          <input
            :value="ai.config.base_url"
            @input="(e: any) => ai.updateConfig({ base_url: e.target.value })"
            class="input"
            placeholder="https://api.openai.com/v1"
          />
        </div>
      </div>

      <div class="section">
        <h3>API Key</h3>
        <div class="field">
          <label>API Key</label>
          <div class="input-with-toggle">
            <input
              :type="showAiKey ? 'text' : 'password'"
              :value="ai.config.api_key"
              @input="(e: any) => ai.updateConfig({ api_key: e.target.value })"
              class="input"
              placeholder="sk-..."
            />
            <button class="toggle-btn" @click="showAiKey = !showAiKey">{{ showAiKey ? '隐藏' : '显示' }}</button>
          </div>
        </div>
      </div>

      <div class="section">
        <h3>模型</h3>
        <div class="field">
          <label>Model</label>
          <div class="model-select-row">
            <select
              :value="ai.config.model"
              @change="(e: any) => ai.updateConfig({ model: e.target.value })"
              class="input model-select"
            >
              <option v-if="ai.config.model && !ai.modelList.includes(ai.config.model)" :value="ai.config.model">{{ ai.config.model }}</option>
              <option v-for="m in ai.modelList" :key="m" :value="m">{{ m }}</option>
            </select>
            <button class="refresh-btn" @click="ai.fetchModels()" :disabled="ai.loadingModels">
              {{ ai.loadingModels ? '...' : '🔄' }}
            </button>
          </div>
        </div>
      </div>

      <div class="section">
        <h3>自然语言识别</h3>
        <div class="field">
          <label>模式</label>
          <select
            :value="ai.config.mode || 'auto'"
            @change="(e: any) => ai.updateConfig({ mode: e.target.value })"
            class="input"
          >
            <option value="auto">自动识别</option>
            <option value="prefix">前缀触发</option>
            <option value="keybinding">按键发送 (Ctrl+Enter)</option>
          </select>
        </div>
        <div class="field" v-if="ai.config.mode === 'prefix'">
          <label>触发前缀</label>
          <input
            :value="ai.config.prefix || ':'"
            @input="(e: any) => ai.updateConfig({ prefix: e.target.value })"
            class="input"
            placeholder=":"
          />
          <p class="field-desc">输入任一字符开头的文本触发 AI 应答，每个字符均为独立触发前缀</p>
        </div>
      </div>

      <div class="section">
        <h3>状态</h3>
        <p class="status" :class="{ ok: ai.enabled }">
          {{ ai.enabled ? '✅ 已配置' : '❌ 未配置 (请输入 API Key)' }}
        </p>
        <button
          v-if="ai.enabled"
          class="btn btn-primary"
          @click="ai.clearHistory()"
        >
          清除对话历史
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ai-panel {
  width: 300px;
  min-width: 300px;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  height: 100%;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.panel-close {
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 12px;
}
.panel-close:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
}

.section {
  margin-bottom: 16px;
}

.section h3 {
  font-size: 12px;
  font-weight: 600;
  color: var(--accent);
  text-transform: uppercase;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--bg-surface0);
  margin-bottom: 8px;
}

.provider-list {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.provider-btn {
  padding: 6px 12px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  text-transform: capitalize;
}
.provider-btn:hover {
  background: var(--bg-surface1);
}
.provider-btn.active {
  border-color: var(--accent);
  background: var(--bg-mantle);
  color: var(--accent);
}

.preset-group {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  margin-top: 6px;
  padding-left: 8px;
  border-left: 2px solid var(--bg-surface1);
}

.preset-chip {
  padding: 4px 10px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-mantle);
  color: var(--text-sub0);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  text-transform: capitalize;
}
.preset-chip:hover {
  background: var(--bg-surface1);
  color: var(--text);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}

.field label {
  font-size: 12px;
  color: var(--text-sub0);
}

.input {
  padding: 8px 10px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 4px;
  color: var(--text);
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: var(--accent);
}

.input-with-toggle {
  display: flex;
  gap: 4px;
}
.input-with-toggle .input {
  flex: 1;
}

.toggle-btn {
  padding: 4px 8px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text-sub0);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  white-space: nowrap;
}
.toggle-btn:hover {
  background: var(--bg-surface1);
}

.model-select-row {
  display: flex;
  gap: 4px;
}
.model-select-row .model-select {
  flex: 1;
}

.refresh-btn {
  padding: 8px 10px;
  border: 1px solid var(--bg-surface1);
  background: var(--bg-surface0);
  color: var(--text);
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  white-space: nowrap;
}
.refresh-btn:hover {
  background: var(--bg-surface1);
}
.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.status {
  font-size: 13px;
  color: var(--danger);
}
.status.ok {
  color: var(--success);
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  margin-top: 8px;
}
.btn-primary {
  background: var(--accent);
  color: var(--bg-base);
  font-weight: 600;
}
.btn-primary:hover {
  background: var(--accent-hover);
}

.field-desc {
  font-size: 11px;
  color: var(--text-sub0);
  margin: 2px 0 0;
}
</style>
