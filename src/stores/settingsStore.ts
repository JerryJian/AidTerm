import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const backgroundOpacity = ref(parseFloat(localStorage.getItem('aidterm_background_opacity') || '1'))
  const backgroundImage = ref(localStorage.getItem('aidterm_background_image') || '')
  const scrollback = ref(parseInt(localStorage.getItem('aidterm_scrollback') || '100000', 10))

  watch(backgroundOpacity, (v) => {
    localStorage.setItem('aidterm_background_opacity', String(v))
  })

  watch(backgroundImage, (v) => {
    localStorage.setItem('aidterm_background_image', v)
  })

  watch(scrollback, (v) => {
    localStorage.setItem('aidterm_scrollback', String(v))
  })

  return {
    backgroundOpacity,
    backgroundImage,
    scrollback,
  }
})
