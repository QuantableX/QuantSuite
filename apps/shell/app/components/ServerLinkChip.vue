<script setup lang="ts">
/**
 * The server this PC uses, in the title bar (card Server 5/7). Shown only in
 * client mode — a PC with its own data keeps the bar clean.
 */
import { useServerLink } from '@quantsuite/core'

const link = useServerLink()

const label = computed(() => {
  const s = link.status.value
  if (!s?.connection) return ''
  switch (s.state) {
    case 'online':
      return s.connection.name
    case 'connecting':
      return `${s.connection.name} · connecting`
    case 'refused':
      return `${s.connection.name} · refused`
    default:
      return `${s.connection.name} · offline`
  }
})

const title = computed(() => {
  const s = link.status.value
  if (!s?.connection) return ''
  const role = { full: 'Full', no_trading: 'No trading', read_only: 'Read-only' }[s.role ?? s.connection.role] ?? ''
  const rtt = s.rtt_ms != null ? `, ${Math.round(s.rtt_ms)} ms` : ''
  const state = s.message ?? (s.state === 'online' ? `Connected to ${s.connection.address}${rtt}` : '')
  return `This PC uses the server ${s.connection.name} (${role}). ${state}`
})

function open() {
  window.dispatchEvent(new CustomEvent('qss:settings', { detail: { section: 'server' } }))
}
</script>

<template>
  <button v-if="link.isClient.value && link.status.value?.connection" class="qsl-chip" :data-state="link.status.value.state" :title="title" @click="open">
    <span class="qsl-dot" />
    {{ label }}
  </button>
</template>

<style scoped>
.qsl-chip { display: inline-flex; align-items: center; gap: 6px; margin-left: 12px; padding: 2px 10px; border-radius: 999px; border: 1px solid var(--qss-border); background: transparent; color: var(--qss-text-secondary); font: inherit; font-size: 11px; cursor: pointer; -webkit-app-region: no-drag; }
.qsl-chip:hover { color: var(--qss-text); background: var(--qss-bg-hover); }
.qsl-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-success, #3fb950); }
.qsl-chip[data-state='connecting'] .qsl-dot { background: var(--qss-warning, #d9a441); }
.qsl-chip[data-state='offline'] .qsl-dot,
.qsl-chip[data-state='refused'] .qsl-dot { background: var(--qss-error, #e5534b); }
</style>
