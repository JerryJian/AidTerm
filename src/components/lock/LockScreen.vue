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
  background: var(--overlay-heavy);
  backdrop-filter: blur(8px);
}

.lock-box {
  background: var(--bg-base);
  border: 1px solid var(--bg-surface0);
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
  color: var(--text);
  margin-bottom: 20px;
}

.lock-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.lock-hint {
  font-size: 13px;
  color: var(--text-sub0);
  margin-bottom: 4px;
}

.lock-input {
  padding: 10px 14px;
  background: var(--bg-mantle);
  border: 1px solid var(--bg-surface1);
  border-radius: 6px;
  color: var(--text);
  font-size: 14px;
  outline: none;
}
.lock-input:focus {
  border-color: var(--accent);
}

.lock-btn {
  padding: 10px;
  background: var(--accent);
  color: var(--bg-base);
  border: none;
  border-radius: 6px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.lock-btn:hover {
  background: var(--accent-hover);
}

.lock-error {
  margin-top: 10px;
  font-size: 13px;
  color: var(--danger);
}
</style>
