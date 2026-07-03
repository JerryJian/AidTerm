<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const emit = defineEmits<{
  unlocked: []
}>()

const password = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const isSettingPassword = ref(false)
const error = ref('')
const hasPassword = ref(false)

const STORAGE_KEY = 'tndterm_lock_password'

onMounted(() => {
  hasPassword.value = !!localStorage.getItem(STORAGE_KEY)
  if (!hasPassword.value) {
    isSettingPassword.value = true
  }
})

function doUnlock() {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (!stored) {
    error.value = t('lock_screen.no_password')
    return
  }
  if (password.value === stored) {
    emit('unlocked')
  } else {
    error.value = t('lock_screen.wrong_password')
    password.value = ''
  }
}

function doSetPassword() {
  if (newPassword.value !== confirmPassword.value) {
    error.value = t('lock_screen.password_mismatch')
    return
  }
  if (!newPassword.value) return
  localStorage.setItem(STORAGE_KEY, newPassword.value)
  hasPassword.value = true
  isSettingPassword.value = false
  password.value = ''
  error.value = ''
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    if (isSettingPassword.value) {
      doSetPassword()
    } else {
      doUnlock()
    }
  }
}
</script>

<template>
  <div class="lock-overlay" @keydown="onKeydown">
    <div class="lock-box">
      <div class="lock-icon">🔒</div>
      <h2 class="lock-title">{{ t('lock_screen.title') }}</h2>

      <div v-if="isSettingPassword && !hasPassword" class="lock-form">
        <p class="lock-hint">{{ t('lock_screen.set_password') }}</p>
        <input
          v-model="newPassword"
          type="password"
          class="lock-input"
          :placeholder="t('lock_screen.set_password')"
          autofocus
        />
        <input
          v-model="confirmPassword"
          type="password"
          class="lock-input"
          :placeholder="t('lock_screen.confirm_password')"
        />
        <button class="lock-btn" @click="doSetPassword">{{ t('lock_screen.set') }}</button>
      </div>

      <div v-else class="lock-form">
        <input
          v-model="password"
          type="password"
          class="lock-input"
          :placeholder="t('lock_screen.placeholder')"
          autofocus
        />
        <button class="lock-btn" @click="doUnlock">{{ t('lock_screen.unlock') }}</button>
      </div>

      <p v-if="error" class="lock-error">{{ error }}</p>
    </div>
  </div>
</template>

<style scoped>
.lock-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(24, 24, 37, 0.95);
  backdrop-filter: blur(8px);
}

.lock-box {
  background: #1e1e2e;
  border: 1px solid #313244;
  border-radius: 12px;
  padding: 32px;
  width: 360px;
  text-align: center;
}

.lock-icon {
  font-size: 48px;
  margin-bottom: 12px;
}

.lock-title {
  font-size: 20px;
  font-weight: 600;
  color: #cdd6f4;
  margin-bottom: 20px;
}

.lock-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.lock-hint {
  font-size: 13px;
  color: #a6adc8;
  margin-bottom: 4px;
}

.lock-input {
  padding: 10px 14px;
  background: #181825;
  border: 1px solid #45475a;
  border-radius: 6px;
  color: #cdd6f4;
  font-size: 14px;
  outline: none;
}
.lock-input:focus {
  border-color: #89b4fa;
}

.lock-btn {
  padding: 10px;
  background: #89b4fa;
  color: #1e1e2e;
  border: none;
  border-radius: 6px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.lock-btn:hover {
  background: #74c7ec;
}

.lock-error {
  margin-top: 10px;
  font-size: 13px;
  color: #f38ba8;
}
</style>
