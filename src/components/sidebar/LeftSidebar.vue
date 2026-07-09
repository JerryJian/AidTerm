<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSessionStore } from '../../stores/sessionStore'
import type { SavedSession } from '../../types'

const { t } = useI18n()

import SessionPanel from '../session/SessionPanel.vue'
import KeyManagerPanel from '../keychain/KeyManagerPanel.vue'
import SnippetPanel from '../snippet/SnippetPanel.vue'
import TriggerPanel from '../trigger/TriggerPanel.vue'
import KnownHostsPanel from '../keychain/KnownHostsPanel.vue'

const sessionStore = useSessionStore()

const emit = defineEmits<{
  connectSession: [session: SavedSession]
  newSession: []
  editSession: [session: SavedSession]
  close: []
}>()

const activeTab = ref<'sessions' | 'keys' | 'snippets' | 'triggers' | 'hosts'>('sessions')

onMounted(() => {
  if (!sessionStore.loaded) sessionStore.load()
})
</script>

<template>
  <div class="sidebar">
    <div class="sidebar-tabs">
      <button class="st-tab" :class="{ active: activeTab === 'sessions' }" @click="activeTab = 'sessions'" :title="t('session_panel.title')">📋</button>
      <button class="st-tab" :class="{ active: activeTab === 'keys' }" @click="activeTab = 'keys'" :title="t('keychain.title')">🔑</button>
      <button class="st-tab" :class="{ active: activeTab === 'snippets' }" @click="activeTab = 'snippets'" :title="t('snippet.title')">⚡</button>
      <button class="st-tab" :class="{ active: activeTab === 'triggers' }" @click="activeTab = 'triggers'" :title="t('trigger.title')">🔫</button>
      <button class="st-tab" :class="{ active: activeTab === 'hosts' }" @click="activeTab = 'hosts'" :title="t('keychain.known_hosts')">🖂</button>
      <div class="st-spacer" />
      <button class="st-tab close-btn" @click="emit('close')" :title="t('titlebar.close')">✕</button>
    </div>

    <!-- Sessions -->
    <div v-if="activeTab === 'sessions'" class="sidebar-body no-pad">
      <SessionPanel
        @connect-session="(s) => emit('connectSession', s)"
        @new-session="emit('newSession')"
        @edit-session="(s) => emit('editSession', s)"
      />
    </div>

    <!-- Keys -->
    <div v-if="activeTab === 'keys'" class="sidebar-body">
      <KeyManagerPanel />
    </div>

    <!-- Snippets -->
    <div v-if="activeTab === 'snippets'" class="sidebar-body">
      <SnippetPanel />
    </div>

    <!-- Triggers -->
    <div v-if="activeTab === 'triggers'" class="sidebar-body">
      <TriggerPanel />
    </div>

    <!-- Known Hosts -->
    <div v-if="activeTab === 'hosts'" class="sidebar-body">
      <KnownHostsPanel />
    </div>
  </div>
</template>

<style scoped>
.sidebar {
  height: 100%;
  background: var(--bg-mantle);
  border-right: 1px solid var(--bg-surface0);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.sidebar-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 6px;
  border-bottom: 1px solid var(--bg-surface0);
  background: var(--bg-base);
  flex-shrink: 0;
}
.st-tab {
  border: none; background: none; color: var(--text-sub0);
  cursor: pointer; padding: 4px 6px; border-radius: 4px;
  font-size: 14px; line-height: 1;
}
.st-tab:hover { background: var(--bg-surface0); color: var(--text); }
.st-tab.active { background: var(--bg-surface1); color: var(--accent); }
.st-spacer { flex: 1; }
.close-btn { font-size: 12px; }
.sidebar-body {
  flex: 1; overflow-y: auto;
  display: flex; flex-direction: column;
}
.sidebar-body.no-pad {
  padding: 0;
}
</style>
