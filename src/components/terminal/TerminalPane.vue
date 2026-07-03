<script setup lang="ts">
import TerminalWrapper from './TerminalWrapper.vue'
import type { TerminalTab } from '../../types'

defineProps<{
  tab: TerminalTab
}>()
</script>

<template>
  <div class="terminal-pane">
    <TerminalWrapper :ssh-info="tab.sshInfo" :telnet-info="tab.telnetInfo" />
  </div>
</template>

<style scoped>
.terminal-pane {
  flex: 1;
  display: flex;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
</style>
