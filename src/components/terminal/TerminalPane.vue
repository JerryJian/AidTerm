<script setup lang="ts">
import TerminalWrapper from './TerminalWrapper.vue'
import type { TerminalTab } from '../../types'

defineProps<{
  tab: TerminalTab
}>()

const emit = defineEmits<{
  newSsh: []
}>()
</script>

<template>
  <div class="terminal-pane">
    <TerminalWrapper :ssh-info="tab.sshInfo" :telnet-info="tab.telnetInfo" @newSsh="emit('newSsh')" />
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
