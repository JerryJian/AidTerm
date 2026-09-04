<script setup lang="ts">
import { ref, nextTick, watch, computed, reactive, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAiConversation } from '../../hooks/useAiConversation'
import { useAiStore } from '../../stores/aiStore'
import { useUiStore } from '../../stores/uiStore'
import { marked } from 'marked'

const { t } = useI18n()
const ai = useAiStore()
const ui = useUiStore()

const props = defineProps<{
  aiConv?: ReturnType<typeof useAiConversation>
  tabTitle?: string
}>()

const inputText = ref('')
const messagesContainer = ref<HTMLDivElement>()

const conversationMessages = computed(() => props.aiConv?.conversationMessages ?? [])

const lastCommand = computed(() => {
  const msgs = conversationMessages.value
  for (let i = msgs.length - 1; i >= 0; i--) {
    if (msgs[i].role === 'command') return msgs[i]
  }
  return null
})

function openAiSettings() {
  ui.settingsTab = 'ai'
  ui.settingsDialog = true
}

function renderMarkdown(text: string): string {
  try {
    return marked.parse(text, { async: false }) as string
  } catch {
    return text
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    doSend()
  }
}

function doSend() {
  const text = inputText.value.trim()
  if (!text) return
  if (!props.aiConv) return
  inputText.value = ''
  props.aiConv?.submitInput(text)
}

function doConfirm() {
  props.aiConv?.onConfirmCommand()
}

function doCancel() {
  props.aiConv?.onCancelCommand()
}

function doReset() {
  props.aiConv?.resetConversation()
}

function getCopyText(msg: { role: string; content: string; command?: string }): string {
  if (msg.role === 'command') return msg.command || ''
  return msg.content
}

function copyMessage(msg: { role: string; content: string; command?: string }) {
  navigator.clipboard.writeText(getCopyText(msg))
}

function formatTime(ts?: number): string {
  if (!ts) return ''
  const d = new Date(ts)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function selectAllText() {
  const el = messagesContainer.value
  if (!el) return
  const range = document.createRange()
  range.selectNodeContents(el)
  const sel = window.getSelection()
  sel?.removeAllRanges()
  sel?.addRange(range)
}

function copySelection() {
  const sel = window.getSelection()
  if (sel && sel.toString()) {
    navigator.clipboard.writeText(sel.toString())
  }
}

const ctx = reactive({ show: false, x: 0, y: 0 })

function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  ctx.x = e.clientX
  ctx.y = e.clientY
  ctx.show = true
  nextTick(clampCtx)
}

function clampCtx() {
  const el = document.querySelector<HTMLElement>('.ai-ctx-menu')
  if (!el || !ctx.show || typeof el.offsetHeight !== 'number' || el.offsetHeight === 0) return
  const margin = 6
  const maxY = window.innerHeight - el.offsetHeight - margin
  const maxX = window.innerWidth - el.offsetWidth - margin
  const y = Math.max(margin, Math.min(ctx.y, maxY))
  const x = Math.max(margin, Math.min(ctx.x, maxX))
  if (x !== ctx.x || y !== ctx.y) {
    ctx.x = x
    ctx.y = y
  }
}

function hideCtx() {
  ctx.show = false
}

function ctxAction(action: 'copy' | 'selectAll') {
  hideCtx()
  if (action === 'copy') copySelection()
  else if (action === 'selectAll') selectAllText()
}

function onCtxDocClick() {
  if (ctx.show) hideCtx()
}

function onCtxKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && ctx.show) hideCtx()
}

onMounted(() => {
  document.addEventListener('click', onCtxDocClick, true)
  document.addEventListener('keydown', onCtxKeydown)
})
onUnmounted(() => {
  document.removeEventListener('click', onCtxDocClick, true)
  document.removeEventListener('keydown', onCtxKeydown)
})

watch(conversationMessages, async () => {
  await nextTick()
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}, { deep: true })
</script>

<template>
  <div class="ai-sidebar">
    <div v-if="!ai.enabled" class="ai-config-prompt" @click="openAiSettings">
      <div class="ai-empty-icon">&#x1F916;</div>
      <div class="ai-config-title">{{ t('ai.config_placeholder') }}</div>
      <div class="ai-config-sub">{{ t('ai.config_click') }}</div>
    </div>
    <template v-else-if="!aiConv">
      <div class="ai-config-prompt">
        <div class="ai-empty-text">{{ t('ai.no_active_terminal') }}</div>
      </div>
    </template>
    <template v-else>
    <div class="ai-header">
      <span class="ai-title" :title="tabTitle">{{ tabTitle }}</span>
      <button class="ai-header-btn" @click="doReset" :title="t('ai.new_conversation')">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>
      </button>
    </div>

    <div class="ai-messages" ref="messagesContainer" @contextmenu="onContextMenu">
      <div v-if="conversationMessages.length === 0" class="ai-empty">
        <div class="ai-empty-icon">&#x1F916;</div>
        <div class="ai-empty-text">{{ t('ai.input_placeholder') }}</div>
      </div>

      <template v-for="(msg, idx) in conversationMessages" :key="idx">
        <div v-if="msg.role === 'user'" class="ai-msg ai-msg-user">
          <div class="ai-msg-header ai-msg-header-right">
            <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
            <span class="ai-msg-time">{{ formatTime(msg.timestamp) }}</span>
            <span class="ai-msg-sender">{{ t('ai.me') }}</span>
          </div>
          <div class="ai-msg-bubble user-bubble">{{ msg.content }}</div>
        </div>

        <div v-else-if="msg.role === 'assistant'" class="ai-msg ai-msg-assistant">
          <div class="ai-msg-header">
            <span class="ai-msg-sender">{{ t('ai.aid') }}</span>
            <span class="ai-msg-time">{{ formatTime(msg.timestamp) }}</span>
            <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
          </div>
          <div class="ai-msg-bubble assistant-bubble">
            <div v-html="renderMarkdown(msg.content)" />
          </div>
        </div>

        <div v-else-if="msg.role === 'command'" class="ai-msg ai-msg-command">
          <div class="ai-msg-header">
            <span class="ai-msg-sender">{{ t('ai.aid') }}</span>
            <span class="ai-msg-time">{{ formatTime(msg.timestamp) }}</span>
            <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
          </div>
          <div class="command-block" :class="{ 'command-danger': msg.dangerous }">
            <div class="command-header">
              <span v-if="msg.dangerous" class="danger-badge">⚠️ {{ t('ai.dangerous_command') }}</span>
              <span v-else class="safe-badge">✅ {{ t('ai.safe_command') }}</span>
              <button v-if="aiConv?.waitingForCommand.value && msg === lastCommand" class="ai-stop-btn" @click="aiConv?.stopWaitingForCommand()" :title="t('ai.stop_waiting')" />
              <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
              </button>
            </div>
            <pre class="command-text">{{ msg.command }}</pre>
            <div v-if="aiConv?.showConfirm.value && aiConv?.pendingToolId.value === msg.toolCallId" class="command-actions">
              <button class="cmd-btn cmd-cancel" @click="doCancel">{{ t('ai.cancel') }}</button>
              <button class="cmd-btn cmd-confirm" :class="{ 'cmd-confirm-danger': msg.dangerous }" @click="doConfirm">{{ t('ai.execute') }}</button>
            </div>
            <div v-else-if="aiConv?.showConfirm.value && aiConv?.pendingToolId.value !== msg.toolCallId" class="command-pending">
              {{ t('ai.thinking') }}
            </div>
            <div v-else-if="!msg.dangerous && aiConv?.autoExecute && msg.autoExecStatus === 'completed'" class="command-auto command-auto-done">
              ✅ {{ t('ai.auto_execute_done') }}
            </div>
            <div v-if="!msg.dangerous && aiConv?.autoExecute && msg.autoExecStatus === 'executing'" class="command-footer">
              <span class="command-auto-running">⏳ {{ t('ai.auto_executing') }}</span>
            </div>
          </div>
        </div>

        <div v-else-if="msg.role === 'result'" class="ai-msg ai-msg-result">
          <div class="ai-msg-header">
            <span class="ai-msg-sender">{{ t('ai.command_output') }}</span>
            <span class="ai-msg-time">{{ formatTime(msg.timestamp) }}</span>
            <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
          </div>
          <pre class="result-block">{{ msg.content }}</pre>
        </div>

        <div v-else-if="msg.role === 'error'" class="ai-msg ai-msg-error">
          <div class="error-text">
            {{ msg.content }}
            <button class="copy-btn copy-btn-sm" @click="copyMessage(msg)" :title="t('ai.copy')">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
          </div>
        </div>

        <div v-else-if="msg.role === 'thinking'" class="ai-msg ai-msg-thinking">
          <div class="thinking-dots">
            <span class="dot" />
            <span class="dot" />
            <span class="dot" />
          </div>
          <span class="thinking-label">{{ msg.content || t('ai.thinking') }}</span>
        </div>
      </template>
    </div>

    <div class="ai-input-area">
      <textarea
        v-model="inputText"
        class="ai-input"
        :placeholder="t('ai.input_placeholder')"
        rows="2"
        @keydown="onKeydown"
        :disabled="aiConv?.busy.value"
      />
      <button
        class="ai-send-btn"
        :class="{ 'ai-send-btn-cancel': aiConv?.busy.value }"
        :title="aiConv?.busy.value ? t('ai.cancel_conversation') : t('ai.send')"
        @click="aiConv?.busy.value ? aiConv?.cancelConversation() : doSend()"
        :disabled="!aiConv?.busy.value && !inputText.trim()"
      >
        <svg v-if="!aiConv?.busy.value" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 2 11 13" /><path d="m22 2-7 20-4-9-9-4Z" />
        </svg>
        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <div v-if="ctx.show" class="ai-ctx-overlay" @click="hideCtx" @contextmenu.prevent="hideCtx" />
    <div v-if="ctx.show" class="ai-ctx-menu" :style="{ left: ctx.x + 'px', top: ctx.y + 'px' }">
      <button class="ai-ctx-item" @click="ctxAction('copy')">{{ t('ai.copy') }}</button>
      <button class="ai-ctx-item" @click="ctxAction('selectAll')">{{ t('ai.select_all') }}</button>
    </div>
    </template>
  </div>
</template>

<style scoped>
.ai-sidebar {
  width: 100%;
  min-width: 0;
  background: var(--bg-base);
  border-left: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  height: 100%;
}

.ai-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  background: var(--bg-mantle);
  border-bottom: 1px solid var(--bg-surface0);
  flex-shrink: 0;
}

.ai-title {
  font-size: 13px;
  font-weight: 400;
  color: var(--text-sub0);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ai-header-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  background: none;
  color: var(--text-sub0);
  cursor: pointer;
  border-radius: 4px;
  flex-shrink: 0;
}
.ai-header-btn:hover {
  background: var(--bg-surface1);
  color: var(--text);
}
.ai-header-btn svg {
  width: 14px;
  height: 14px;
}

.ai-messages {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.ai-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-overlay0);
}

.ai-empty-icon {
  font-size: 32px;
  opacity: 0.5;
}

.ai-empty-text {
  font-size: 13px;
}

.ai-config-prompt {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 24px;
  text-align: center;
  color: var(--text-overlay0);
  cursor: pointer;
  user-select: none;
}
.ai-config-prompt:hover {
  color: var(--text-sub0);
}
.ai-config-prompt:hover .ai-empty-icon {
  opacity: 0.8;
}

.ai-config-title {
  font-size: 14px;
  font-weight: 600;
}

.ai-config-sub {
  font-size: 12px;
  color: var(--accent);
}

.ai-msg {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.ai-msg-user {
  align-items: flex-end;
}

.ai-msg-assistant,
.ai-msg-command,
.ai-msg-result,
.ai-msg-error,
.ai-msg-thinking {
  align-items: flex-start;
}

.ai-msg-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-left: 4px;
}
.ai-msg-header-right {
  justify-content: flex-end;
  padding-left: 0;
  padding-right: 4px;
}

.ai-msg-sender {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-sub0);
}

.ai-msg-time {
  font-size: 10px;
  color: var(--text-sub0);
  opacity: 0;
  transition: opacity 0.15s;
}

.ai-msg-bubble {
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
  line-height: 1.5;
  max-width: 100%;
  word-break: break-word;
}

.user-bubble {
  background: var(--accent);
  color: var(--bg-base);
  border-bottom-right-radius: 2px;
  max-width: 85%;
  align-self: flex-end;
}

.assistant-bubble {
  background: var(--bg-mantle);
  color: var(--text);
  border: 1px solid var(--bg-surface0);
  border-bottom-left-radius: 2px;
  width: 100%;
}

.assistant-bubble :deep(p) {
  margin: 0 0 8px;
}
.assistant-bubble :deep(p:last-child) {
  margin-bottom: 0;
}
.assistant-bubble :deep(pre) {
  background: var(--bg-crust);
  padding: 8px;
  border-radius: 4px;
  overflow-x: auto;
  font-size: 12px;
  margin: 8px 0;
}
.assistant-bubble :deep(code) {
  background: var(--bg-crust);
  padding: 1px 4px;
  border-radius: 3px;
  font-size: 12px;
}
.assistant-bubble :deep(pre code) {
  background: none;
  padding: 0;
}
.assistant-bubble :deep(ul),
.assistant-bubble :deep(ol) {
  margin: 4px 0;
  padding-left: 20px;
}
.assistant-bubble :deep(strong) {
  color: var(--accent);
}
.assistant-bubble :deep(a) {
  color: var(--accent);
  text-decoration: underline;
}
.assistant-bubble :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 8px 0;
  font-size: 12px;
}
.assistant-bubble :deep(th),
.assistant-bubble :deep(td) {
  border: 1px solid var(--bg-surface1);
  padding: 5px 8px;
  text-align: left;
  white-space: normal;
}
.assistant-bubble :deep(th) {
  background: var(--bg-surface0);
  font-weight: 600;
  color: var(--text);
}
.assistant-bubble :deep(td) {
  background: var(--bg-crust);
  color: var(--text);
}
.assistant-bubble :deep(tr:hover td) {
  background: var(--bg-surface0);
}

.command-block {
  background: var(--bg-mantle);
  border: 1px solid var(--success);
  border-radius: 6px;
  overflow: hidden;
  width: 100%;
}
.command-block.command-danger {
  border-color: var(--danger);
}

.command-header {
  padding: 6px 10px;
  background: var(--bg-surface0);
  font-size: 11px;
  color: var(--success);
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 4px;
}
.command-header .copy-btn {
  opacity: 1;
}

.danger-badge {
  color: var(--danger);
}
.safe-badge {
  color: var(--success);
}

.command-text {
  padding: 8px 10px;
  margin: 0;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  color: var(--success);
  white-space: pre-wrap;
  word-break: break-all;
}

.command-actions {
  display: flex;
  gap: 6px;
  padding: 8px 10px;
  border-top: 1px solid var(--bg-surface0);
}

.cmd-btn {
  padding: 5px 14px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  font-weight: 500;
}

.cmd-cancel {
  background: var(--bg-surface1);
  color: var(--text);
}
.cmd-cancel:hover {
  background: var(--text-overlay0);
}

.cmd-confirm {
  background: var(--success);
  color: var(--bg-base);
  font-weight: 600;
}
.cmd-confirm:hover {
  background: var(--teal);
}
.cmd-confirm-danger {
  background: var(--danger);
}
.cmd-confirm-danger:hover {
  background: var(--rosewater);
}

.command-pending {
  padding: 8px 10px;
  font-size: 11px;
  color: var(--text-overlay0);
  border-top: 1px solid var(--bg-surface0);
}

.command-auto {
  padding: 6px 10px;
  font-size: 11px;
  color: var(--accent);
  border-top: 1px solid var(--bg-surface0);
}

.command-auto-done {
  color: var(--success);
}

.command-auto-running {
  color: var(--accent);
  white-space: nowrap;
  font-size: 11px;
}

.result-block {
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface0);
  border-radius: 4px;
  padding: 8px 10px;
  margin: 0;
  font-family: Consolas, "Courier New", monospace;
  font-size: 11px;
  color: var(--text-sub0);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
  width: 100%;
}

.error-text {
  font-size: 12px;
  color: var(--danger);
  padding: 4px 8px;
}

.ai-msg-thinking {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.thinking-dots {
  display: flex;
  gap: 4px;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  animation: blink 1.4s infinite both;
}
.dot:nth-child(2) { animation-delay: 0.2s; }
.dot:nth-child(3) { animation-delay: 0.4s; }

@keyframes blink {
  0%, 80%, 100% { opacity: 0.3; }
  40% { opacity: 1; }
}

.thinking-label {
  font-size: 12px;
  color: var(--text-overlay0);
  font-style: italic;
}

.ai-input-area {
  position: relative;
  padding: 8px 12px;
  border-top: 1px solid var(--bg-surface0);
  background: var(--bg-mantle);
  flex-shrink: 0;
}

.ai-stop-btn {
  width: 12px;
  height: 12px;
  margin-left: auto;
  padding: 0;
  border: none;
  border-radius: 2px;
  background: var(--danger);
  cursor: pointer;
  flex-shrink: 0;
  opacity: 0.85;
  transition: opacity 0.15s;
}
.ai-stop-btn:hover {
  opacity: 1;
}

.command-footer {
  padding: 6px 10px;
  background: var(--bg-mantle);
  border-top: 1px solid var(--bg-surface0);
  display: flex;
  align-items: center;
  gap: 8px;
}

.ai-input {
  width: 100%;
  padding: 8px 44px 8px 10px;
  background: var(--bg-base);
  border: 1px solid var(--bg-surface1);
  border-radius: 6px;
  color: var(--text);
  font-size: 13px;
  font-family: inherit;
  resize: none;
  outline: none;
  min-height: 36px;
  max-height: 80px;
}
.ai-input:focus {
  border-color: var(--accent);
}
.ai-input:disabled {
  opacity: 0.5;
}

.ai-send-btn {
  position: absolute;
  right: 18px;
  bottom: 18px;
  width: 26px;
  height: 26px;
  padding: 0;
  background: var(--accent);
  color: var(--bg-base);
  border: none;
  border-radius: 50%;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s, transform 0.1s;
  box-shadow: 0 1px 3px rgba(0,0,0,0.25);
}
.ai-send-btn svg {
  width: 14px;
  height: 14px;
}
.ai-send-btn:hover:not(:disabled) {
  background: var(--accent-hover);
  transform: scale(1.05);
}
.ai-send-btn:active:not(:disabled) {
  transform: scale(0.95);
}
.ai-send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
  box-shadow: none;
}
.ai-send-btn-cancel {
  background: var(--danger);
}
.ai-send-btn-cancel:hover:not(:disabled) {
  background: var(--rosewater, var(--danger));
}

.copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  color: var(--text-sub0);
  cursor: pointer;
  padding: 2px;
  border-radius: 3px;
  opacity: 0;
  transition: opacity 0.15s, background 0.15s, color 0.15s;
  flex-shrink: 0;
  margin-left: 6px;
}
.copy-btn svg {
  width: 12px;
  height: 12px;
}
.copy-btn:hover {
  background: var(--bg-surface1);
  color: var(--text);
}
.copy-btn-sm {
  padding: 1px;
}
.copy-btn-sm svg {
  width: 11px;
  height: 11px;
}
.ai-msg:hover > .ai-msg-header .copy-btn,
.ai-msg:hover > .ai-msg-header .ai-msg-time,
.ai-msg:hover .command-header .copy-btn {
  opacity: 1;
}

.ai-ctx-overlay {
  position: fixed;
  inset: 0;
  z-index: 9998;
}
.ai-ctx-menu {
  position: fixed;
  z-index: 9999;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 6px;
  padding: 4px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  min-width: 120px;
}
.ai-ctx-item {
  display: block;
  width: 100%;
  text-align: left;
  background: none;
  border: none;
  color: var(--text);
  font-size: 12px;
  padding: 6px 12px;
  border-radius: 4px;
  cursor: pointer;
}
.ai-ctx-item:hover {
  background: var(--accent);
  color: var(--bg-base);
}
</style>
