<script setup lang="ts">
/**
 * Suite → General — the shell's section in the unified settings modal (V3).
 * Owns suite-level behaviour: the suite version and updates, workspace startup
 * and Windows autostart today;
 * agent approval modes and motion preferences collect here as they land.
 */
import { qs } from '@quantsuite/core'

const ws = useWorkspaces()

/**
 * Autostart is read back from the OS entry, not from `core.db` — the user can
 * remove it in Task Manager and the toggle has to follow (ARCHITECTURE.md §10).
 * `null` while unknown: in a plain browser there is no backend to ask, so the
 * row renders disabled instead of claiming the setting is off.
 */
const autostart = ref<boolean | null>(null)
const autostartBusy = ref(false)
const autostartError = ref<string | null>(null)

function inTauri(): boolean {
  return import.meta.client && '__TAURI__' in window
}

async function toggleAutostart() {
  if (autostart.value === null || autostartBusy.value) return
  const next = !autostart.value
  autostartBusy.value = true
  autostartError.value = null
  try {
    await qs.core.setAutostart(next)
    // Confirm against the OS rather than assuming the write took.
    autostart.value = await qs.core.autostartEnabled()
  } catch (e) {
    console.error('[shell] toggling autostart failed', e)
    autostartError.value = e instanceof Error ? e.message : String(e)
  } finally {
    autostartBusy.value = false
  }
}

onMounted(async () => {
  if (!ws.ready.value) ws.load()
  if (!inTauri()) return
  try {
    autostart.value = await qs.core.autostartEnabled()
  } catch (e) {
    console.error('[shell] reading autostart failed', e)
    autostartError.value = e instanceof Error ? e.message : String(e)
  }
})
</script>

<template>
  <div>
    <SuiteUpdates />

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Reopen last workspace on start</p>
        <p class="qsu-hint">Skip the picker and land directly in the folder you last had open.</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': ws.reopenLast.value }"
        role="switch"
        :aria-checked="ws.reopenLast.value"
        @click="ws.setReopenLast(!ws.reopenLast.value)"
      />
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Start QuantSuite with Windows</p>
        <p class="qsu-hint">
          Launches minimised to the tray when you sign in. Open it from the tray icon.
        </p>
        <p v-if="autostartError" class="qsu-hint qss-hint-error">{{ autostartError }}</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': autostart === true }"
        role="switch"
        :aria-checked="autostart === true"
        :disabled="autostart === null || autostartBusy"
        @click="toggleAutostart"
      />
    </div>
  </div>
</template>

<style scoped>
.qss-hint-error {
  color: var(--qss-error);
}
</style>
