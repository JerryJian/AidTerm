import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const transparency = ref(parseFloat(localStorage.getItem('aidterm_transparency') || '1'))
  const backgroundImage = ref(localStorage.getItem('aidterm_background_image') || '')
  const minimizeToTray = ref(localStorage.getItem('aidterm_minimize_tray') === 'true')
  const closeToTray = ref(localStorage.getItem('aidterm_close_tray') === 'true')

  watch(transparency, (v) => {
    localStorage.setItem('aidterm_transparency', String(v))
  })

  watch(backgroundImage, (v) => {
    localStorage.setItem('aidterm_background_image', v)
  })

  watch(minimizeToTray, (v) => {
    localStorage.setItem('aidterm_minimize_tray', String(v))
  })

  watch(closeToTray, (v) => {
    localStorage.setItem('aidterm_close_tray', String(v))
  })

  return {
    transparency,
    backgroundImage,
    minimizeToTray,
    closeToTray,
  }
})
