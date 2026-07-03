import { createApp } from 'vue'
import { createPinia } from 'pinia'

// Must import xterm.css before mounting Vue to ensure styles are available
import '@xterm/xterm/css/xterm.css'

import App from './App.vue'

const app = createApp(App)
app.use(createPinia())
app.mount('#app')
