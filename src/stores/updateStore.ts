import { defineStore } from 'pinia'
import { ref } from 'vue'
import { checkForUpdate } from '@/api'
import type { UpdateInfo } from '@/types'

export const useUpdateStore = defineStore('update', () => {
  const updateInfo = ref<UpdateInfo | null>(null)
  const checking = ref(false)
  const checkError = ref('')
  const dialogOpen = ref(false)

  async function checkForUpdates(opts: { autoOpen?: boolean; silent?: boolean } = {}) {
    const { autoOpen = true, silent = false } = opts
    checking.value = true
    if (!silent) checkError.value = ''
    try {
      updateInfo.value = await checkForUpdate()
      if (updateInfo.value.has_update && autoOpen) dialogOpen.value = true
      return updateInfo.value.has_update
    } catch (e: unknown) {
      if (!silent) {
        checkError.value = e instanceof Error ? e.message : String(e)
        updateInfo.value = null
      }
      return false
    } finally {
      checking.value = false
    }
  }

  return { updateInfo, checking, checkError, dialogOpen, checkForUpdates }
})
