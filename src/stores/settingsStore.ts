import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const backgroundOpacity = ref(parseFloat(localStorage.getItem('aidterm_background_opacity') || '1'))
  const backgroundImage = ref(localStorage.getItem('aidterm_background_image') || '')
  const scrollback = ref(parseInt(localStorage.getItem('aidterm_scrollback') || '100000', 10))
  const adbAutoKill = ref(localStorage.getItem('aidterm_adb_auto_kill') !== 'false')
  const lockEnabled = ref(localStorage.getItem('aidterm_lock_enabled') !== 'false')
  const lockPassword = ref(localStorage.getItem('aidterm_lock_password') || '')
  const hasLockPassword = ref(!!localStorage.getItem('aidterm_lock_password'))

  watch(backgroundOpacity, (v) => {
    localStorage.setItem('aidterm_background_opacity', String(v))
  })

  watch(backgroundImage, (v) => {
    localStorage.setItem('aidterm_background_image', v)
  })

  watch(scrollback, (v) => {
    localStorage.setItem('aidterm_scrollback', String(v))
  })

  watch(adbAutoKill, (v) => {
    localStorage.setItem('aidterm_adb_auto_kill', String(v))
  })

  watch(lockEnabled, (v) => {
    localStorage.setItem('aidterm_lock_enabled', String(v))
  })

  function setLockPassword(password: string) {
    if (!password) return
    lockPassword.value = password
    hasLockPassword.value = true
    localStorage.setItem('aidterm_lock_password', password)
  }

  function clearLockPassword() {
    lockPassword.value = ''
    hasLockPassword.value = false
    localStorage.removeItem('aidterm_lock_password')
  }

  return {
    backgroundOpacity,
    backgroundImage,
    scrollback,
    adbAutoKill,
    lockEnabled,
    lockPassword,
    hasLockPassword,
    setLockPassword,
    clearLockPassword,
  }
})
