<script setup lang="ts">
/**
 * QuantScript's section in the unified settings modal: the interpreter the
 * scripts run with, where the scripts and the versions live.
 */
import { invoke } from '@tauri-apps/api/core'
import { useWorkbenchStore } from '#script/stores/workbench'
import type { PythonInfo } from '#script/types'

const wb = useWorkbenchStore()
const busy = ref(false)
const notice = ref('')

onMounted(() => {
  if (!wb.listing && !wb.listingLoading) void wb.loadListing()
})

async function copyFolder() {
  if (!wb.listing?.indicators_dir) return
  try {
    await navigator.clipboard.writeText(wb.listing.indicators_dir)
    notice.value = 'Script folder path copied.'
  } catch (err) {
    notice.value = String(err)
  }
}

async function redetect() {
  busy.value = true
  notice.value = ''
  try {
    const info = await invoke<PythonInfo>('plugin:script|script_python', { refresh: true })
    notice.value = info.ok ? `Using ${info.command} (${info.source}).` : (info.error ?? 'No interpreter found.')
    await wb.loadListing(true)
  } catch (err) {
    notice.value = String(err)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="qsc-settings">
    <section class="qsc-set-block">
      <h3 class="qsc-set-title">Python</h3>
      <p class="qsc-set-hint">
        Listing the registry and checking a script run <span class="mono">python -m smithery.quantscript</span>,
        which needs numpy and pandas. <span class="mono">QUANTSCRIPT_PYTHON</span> (or QuantSystems'
        <span class="mono">QUANTSYSTEMS_PYTHON</span>) names the interpreter; otherwise
        <span class="mono">py -3</span>, <span class="mono">python3</span> and <span class="mono">python</span> are probed.
      </p>
      <div class="qsc-set-row">
        <span class="qsc-dot" :class="wb.python ? (wb.python.ok ? 'is-ok' : 'is-bad') : ''" />
        <span class="mono qsc-set-value">{{ wb.python?.command ?? '—' }}</span>
        <span class="qsc-set-status">{{ wb.python ? (wb.python.ok ? `${wb.python.source} · numpy and pandas import` : wb.python.error) : '' }}</span>
      </div>
      <div class="qsc-set-row">
        <button class="qsc-btn" :disabled="busy" @click="redetect">{{ busy ? 'Probing…' : 'Re-detect' }}</button>
      </div>
    </section>

    <section class="qsc-set-block">
      <h3 class="qsc-set-title">Your script folder</h3>
      <p class="qsc-set-hint">Put your Python indicator scripts in QuantScript/indicators inside the software folder, then refresh the library. New installations start empty. Scripts and Forge parameter versions in this folder are excluded from setup.exe. Set QUANTSCRIPT_INDICATORS_DIR before launching to use another folder.</p>
      <button class="qsc-btn" :disabled="!wb.listing?.indicators_dir" @click="copyFolder">Copy folder path</button>
      <div class="qsc-set-row">
        <span class="qsc-set-label">scripts</span>
        <span class="mono qsc-set-value" :title="wb.listing?.indicators_dir">{{ wb.listing?.indicators_dir ?? '—' }}</span>
      </div>
      <div class="qsc-set-row">
        <span class="qsc-set-label">versions</span>
        <span class="mono qsc-set-value" :title="wb.listing?.db_path">{{ wb.listing?.db_path ?? '—' }}</span>
      </div>
      <p class="qsc-set-hint">
        The files are the source of truth — the forge, QuantSystems and QuantAlgo import them. Every save
        is checked in a sandbox copy of the package first and recorded as a version; a restore is a new
        version, never a rewind. A running QuantSystems engine loads the registry at start: restart it to
        pick up a changed script.
      </p>
    </section>

    <p v-if="notice" class="qsc-set-notice">{{ notice }}</p>
  </div>
</template>

<style scoped>
.qsc-settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
}
.qsc-set-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qsc-set-title {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qss-text-muted);
}
.qsc-set-hint {
  font-size: 12px;
  line-height: 1.5;
  color: var(--qss-text-secondary);
}
.qsc-set-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.qsc-set-label {
  width: 64px;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.qsc-set-value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--qss-text);
}
.qsc-set-status {
  font-size: 12px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-set-notice {
  font-size: 12px;
  color: var(--qss-text-secondary);
}
</style>
