import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const transparency = ref(parseFloat(localStorage.getItem('tndterm_transparency') || '1'))
  const backgroundImage = ref(localStorage.getItem('tndterm_background_image') || '')
  const minimizeToTray = ref(localStorage.getItem('tndterm_minimize_tray') === 'true')
  const closeToTray = ref(localStorage.getItem('tndterm_close_tray') === 'true')

  watch(transparency, (v) => {
    localStorage.setItem('tndterm_transparency', String(v))
  })

  watch(backgroundImage, (v) => {
    localStorage.setItem('tndterm_background_image', v)
  })

  watch(minimizeToTray, (v) => {
    localStorage.setItem('tndterm_minimize_tray', String(v))
  })

  watch(closeToTray, (v) => {
    localStorage.setItem('tndterm_close_tray', String(v))
  })

  return {
    transparency,
    backgroundImage,
    minimizeToTray,
    closeToTray,
  }
})
