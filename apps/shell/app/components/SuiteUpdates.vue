<script setup lang="ts">
/** Settings → General: the suite version and its one update channel. */
import { Download, RefreshCw } from 'lucide-vue-next'

const { currentVersion, update, available, busy, status, error, check, install } = useSuiteUpdates()
</script>

<template>
  <div class="qsu-row suite-updates" :aria-busy="busy">
    <div class="qsu-row-text">
      <p class="qsu-label">QuantSuite <span class="suite-updates-version">v{{ currentVersion }}</span></p>
      <p class="qsu-hint" role="status" aria-live="polite" :class="{ 'suite-updates-error': error }">{{ status }}</p>
      <p v-if="available" class="qsu-hint">Save your work before installing. All suite windows will restart.</p>
      <details v-if="available && update?.notes" class="qsu-hint">
        <summary>What’s new</summary><p class="suite-updates-notes">{{ update.notes }}</p>
      </details>
    </div>
    <div class="suite-updates-actions">
      <button class="qsu-btn" :disabled="busy" @click="check"><RefreshCw :size="13" />Check for updates</button>
      <button v-if="available" class="qsu-btn qsu-btn--primary" :disabled="busy" @click="install"><Download :size="13" />Install &amp; restart</button>
    </div>
  </div>
</template>

<style scoped>
.suite-updates-version { margin-left: 6px; font-weight: 400; color: var(--qss-text-muted); font-variant-numeric: tabular-nums; }
.suite-updates-error { color: var(--qss-error); overflow-wrap: anywhere; }
.suite-updates-actions { display: flex; flex-shrink: 0; gap: 8px; }
.suite-updates-actions .qsu-btn { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
.suite-updates-actions .qsu-btn:disabled { opacity: 0.55; cursor: wait; }
summary { cursor: pointer; }
.suite-updates-notes { margin: 6px 0 0; white-space: pre-wrap; max-height: 180px; overflow: auto; }
</style>
