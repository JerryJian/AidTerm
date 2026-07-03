import { createApp } from 'vue'
import { createPinia } from 'pinia'

// Must import xterm.css before mounting Vue to ensure styles are available
import '@xterm/xterm/css/xterm.css'

import App from './App.vue'
import { i18n } from './i18n'

const app = createApp(App)
app.use(createPinia())
app.use(i18n)
app.mount('#app')
