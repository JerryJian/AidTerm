<script setup lang="ts">
import { useAiStore } from '../../stores/aiStore'

const ai = useAiStore()
const emit = defineEmits<{ close: [] }>()

function selectProvider(name: string) {
  ai.setProvider(name)
}
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
        <div class="provider-list">
          <button
            v-for="(_, name) in ai.defaultProviders"
            :key="name"
            class="provider-btn"
            :class="{ active: ai.config.provider === name }"
            @click="selectProvider(name)"
          >
            {{ name }}
          </button>
        </div>
      </div>

      <div class="section">
        <h3>API 配置</h3>
        <div class="field">
          <label>API Key</label>
          <input
            v-model="ai.config.api_key"
            type="password"
            class="input"
            placeholder="sk-..."
            @change="ai.saveConfig()"
          />
        </div>
        <div class="field">
          <label>Model</label>
          <input
            v-model="ai.config.model"
            class="input"
            @change="ai.saveConfig()"
          />
        </div>
        <div class="field">
          <label>Base URL</label>
          <input
            v-model="ai.config.base_url"
            class="input"
            @change="ai.saveConfig()"
          />
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
</style>
