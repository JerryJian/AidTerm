<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useSftpStore } from '../../stores/sftpStore'

const props = defineProps<{
  connId: string
  remotePath: string
}>()

const emit = defineEmits<{
  close: []
}>()

const sftpStore = useSftpStore()
const content = ref('')
const loading = ref(true)
const saving = ref(false)
const saved = ref(false)
const error = ref('')
const fileName = ref('')

onMounted(async () => {
  fileName.value = props.remotePath.split('/').pop() || props.remotePath
  try {
    content.value = await sftpStore.readFile(props.remotePath)
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
})

async function doSave() {
  saving.value = true
  saved.value = false
  error.value = ''
  try {
    await sftpStore.writeFile(props.remotePath, content.value)
    saved.value = true
    setTimeout(() => { saved.value = false }, 2000)
  } catch (e: any) {
    error.value = String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="editor">
    <div class="editor-header">
      <span class="editor-title">📝 {{ fileName }}</span>
      <div class="editor-actions">
        <span v-if="saved" class="editor-saved">已保存</span>
        <button class="btn btn-save" :disabled="saving || loading" @click="doSave">
          {{ saving ? '保存中...' : '保存' }}
        </button>
        <button class="btn btn-close" @click="emit('close')">✕</button>
      </div>
    </div>
    <div v-if="loading" class="editor-loading">加载中...</div>
    <div v-else-if="error" class="editor-error">{{ error }}</div>
    <textarea v-else v-model="content" class="editor-textarea" spellcheck="false"></textarea>
  </div>
</template>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  background: #1e1e2e;
}

.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: #181825;
  border-bottom: 1px solid #313244;
}

.editor-title {
  font-size: 13px;
  font-weight: 600;
  color: #cdd6f4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.editor-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.editor-saved {
  font-size: 12px;
  color: #a6e3a1;
}

.btn {
  padding: 6px 14px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}
.btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.btn-save {
  background: #89b4fa;
  color: #1e1e2e;
  font-weight: 600;
}
.btn-save:hover:not(:disabled) {
  background: #74c7ec;
}

.btn-close {
  background: #313244;
  color: #a6adc8;
  padding: 6px 10px;
}
.btn-close:hover {
  background: #45475a;
  color: #cdd6f4;
}

.editor-loading,
.editor-error {
  padding: 20px;
  text-align: center;
  color: #a6adc8;
  font-size: 13px;
}
.editor-error {
  color: #f38ba8;
}

.editor-textarea {
  flex: 1;
  padding: 12px;
  background: #1e1e2e;
  border: none;
  color: #cdd6f4;
  font-family: Consolas, "Courier New", monospace;
  font-size: 13px;
  line-height: 1.5;
  resize: none;
  outline: none;
  tab-size: 4;
}
.editor-textarea:focus {
  background: #181825;
}
</style>
