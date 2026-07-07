import { createI18n } from 'vue-i18n'
import zhCN from './zh-CN'
import enUS from './en-US'

const savedLang = localStorage.getItem('aidterm_language') || 'zh-CN'

export const i18n = createI18n({
  legacy: true,
  locale: savedLang,
  fallbackLocale: 'en-US',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
  },
})

export function setLanguage(lang: string) {
  i18n.global.locale = lang as 'zh-CN' | 'en-US'
  localStorage.setItem('aidterm_language', lang)
}
