import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export const useSettingsStore = defineStore('settings', () => {
  const transparency = ref(parseFloat(localStorage.getItem('aidterm_transparency') || '1'))
  const backgroundImage = ref(localStorage.getItem('aidterm_background_image') || '')

  watch(transparency, (v) => {
    localStorage.setItem('aidterm_transparency', String(v))
  })

  watch(backgroundImage, (v) => {
    localStorage.setItem('aidterm_background_image', v)
  })

  return {
    transparency,
    backgroundImage,
  }
})
