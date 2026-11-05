<script setup lang="ts">
/**
 * "Server not reachable since 12:03" (PLAN-SERVER-CLIENT §5, card Server
 * 5/7): above the stage while this PC uses a server it cannot reach. Pages
 * from the server keep their last data, greyed and without actions
 * (ModuleStage); this PC's own modules keep working.
 */
import { useServerLink } from '@quantsuite/core'

const link = useServerLink()

function hhmm(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

const shown = computed(() => link.isClient.value && link.status.value && link.status.value.state !== 'online')
const text = computed(() => {
  const s = link.status.value
  if (!s?.connection) return ''
  const name = s.connection.name
  if (s.state === 'connecting' && !s.since) return `Connecting to ${name}…`
  if (s.state === 'refused') return `${name} refused this PC: ${s.message ?? ''}`
  return `Server ${name} not reachable since ${hhmm(s.since)} — reconnecting. Its pages show their last data.`
})

function settings() {
  window.dispatchEvent(new CustomEvent('qss:settings', { detail: { section: 'server' } }))
}
</script>

<template>
  <div v-if="shown" class="qsb-banner" :data-state="link.status.value?.state" role="status">
    <span>{{ text }}</span>
    <button v-if="link.status.value?.state !== 'refused'" class="qsb-btn" @click="link.retry()">Try now</button>
    <button v-else class="qsb-btn" @click="settings">Settings</button>
  </div>
</template>

<style scoped>
.qsb-banner { display: flex; align-items: center; gap: 12px; padding: 7px 12px; margin-bottom: 8px; border-radius: var(--qss-radius-lg, 10px); border: 1px solid color-mix(in srgb, var(--qss-warning, #d9a441) 50%, transparent); background: color-mix(in srgb, var(--qss-warning, #d9a441) 12%, transparent); font-size: 12px; color: var(--qss-text); }
.qsb-banner[data-state='refused'] { border-color: color-mix(in srgb, var(--qss-error, #e5534b) 50%, transparent); background: color-mix(in srgb, var(--qss-error, #e5534b) 12%, transparent); }
.qsb-banner span { flex: 1; }
.qsb-btn { padding: 3px 10px; border-radius: 6px; border: 1px solid var(--qss-border); background: var(--qss-bg-card); color: var(--qss-text); font: inherit; font-size: 11px; cursor: pointer; }
</style>
