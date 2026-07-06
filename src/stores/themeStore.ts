import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

export type ThemeMode = 'dark' | 'light'

export const useThemeStore = defineStore('theme', () => {
  const saved = localStorage.getItem('tndterm_theme')
  const mode = ref<ThemeMode>((saved as ThemeMode) || 'dark')

  function setMode(m: ThemeMode) {
    mode.value = m
    localStorage.setItem('tndterm_theme', m)
  }

  function toggle() {
    setMode(mode.value === 'dark' ? 'light' : 'dark')
  }

  watch(mode, (m) => {
    document.documentElement.setAttribute('data-theme', m)
  }, { immediate: true })

  return { mode, setMode, toggle }
})
