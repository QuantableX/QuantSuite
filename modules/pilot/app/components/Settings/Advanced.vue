<script setup lang="ts">
/**
 * Settings → Advanced (card 19): where QuantPilot keeps its data, the
 * diagnostics a bug report needs (versions, paths, settings — variable
 * names, never their values), and Export logs: those plus the newest part
 * of the suite's log, saved to Downloads and shown in Explorer.
 */
import { onMounted, ref } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { pilot } from '#pilot/utils/invoke'

const store = usePilotStore()
const diag = ref('')
const notice = ref('')
const busy = ref(false)

onMounted(async () => {
  if (!store.status) void store.refreshStatus()
  diag.value = (await pilot<string>('plugin:pilot|pilot_diagnostics').catch((e) => String(e))) ?? ''
})

async function copy() {
  try {
    await navigator.clipboard.writeText(diag.value)
    notice.value = 'Copied.'
  } catch {
    notice.value = 'The clipboard is not available.'
  }
}
async function exportLogs() {
  busy.value = true
  try {
    const path = await pilot<string>('plugin:pilot|pilot_export_logs')
    notice.value = path ? `Saved ${path}` : ''
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <h3 class="qp-settings-title">Data</h3>
    <div class="qp-row">
      <code class="qp-path">{{ store.status?.dataDir || '…' }}</code>
      <button class="qp-btn qp-btn-sm" @click="pilot('plugin:pilot|pilot_open_data_dir')">Open</button>
    </div>
    <p class="qp-settings-desc">Threads (pilot.db), settings, keybindings and the price list live here.</p>

    <h3 class="qp-settings-title">Diagnostics</h3>
    <pre class="qp-diag">{{ diag || 'Collecting…' }}</pre>
    <div class="qp-row">
      <button class="qp-btn qp-btn-sm" :disabled="!diag" @click="copy">Copy</button>
      <button class="qp-btn qp-btn-sm" :disabled="busy" @click="exportLogs">{{ busy ? 'Saving…' : 'Export logs' }}</button>
      <span class="qp-notice">{{ notice }}</span>
    </div>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-row { display: flex; align-items: center; gap: 8px; }
.qp-path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-diag { margin: 0; max-height: 260px; overflow: auto; padding: 8px 10px; border-radius: 8px; border: 1px solid var(--qss-border-subtle, #35353d); background: var(--qss-bg-raised, #1f1f25); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; line-height: 1.5; white-space: pre-wrap; user-select: text; }
.qp-notice { font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
