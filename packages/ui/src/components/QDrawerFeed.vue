<script setup lang="ts">
/**
 * The drawer's Feed tab (PLAN-V2 E4) — everything that happened across the
 * suite, straight off the event bus: settings changes, entity writes, bot
 * fills, window lifecycle, agent steps. History from `recent_events`, then
 * live via the bus's `'*'` subscription (built for exactly this).
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { bus, type QsEvent } from '@quantsuite/core'

const events = ref<QsEvent[]>([])
const live = ref(false)
const MAX = 300

function push(e: QsEvent) {
  events.value.unshift(e)
  if (events.value.length > MAX) events.value.length = MAX
}

function timeOf(ts: number) {
  return new Date(ts).toLocaleTimeString(undefined, { hour12: false })
}

function summarize(e: QsEvent): string {
  try {
    const p = e.payload as Record<string, unknown> | null
    if (!p || typeof p !== 'object') return ''
    const interesting = ['scope', 'key', 'kind', 'id', 'module', 'name', 'path', 'label', 'message']
    return interesting
      .filter((k) => typeof p[k] === 'string')
      .slice(0, 3)
      .map((k) => `${k}=${p[k]}`)
      .join('  ')
  } catch {
    return ''
  }
}

let off: (() => void) | undefined
let disposed = false

onMounted(async () => {
  try {
    const recent = await bus.recentEvents(150)
    events.value = recent.slice().sort((a, b) => b.ts - a.ts)
    live.value = true
  } catch {
    live.value = false // plain browser — no bus backend
  }
  // A tab switch unmounts this inside the round-trip above — the unmount hook
  // then ran with `off` still undefined and the `'*'` handler stayed in the
  // bus for good, firing on every suite event into a dead component.
  const stop = bus.on('*', push)
  if (disposed) stop()
  else off = stop
})
onUnmounted(() => {
  disposed = true
  off?.()
})
</script>

<template>
  <div class="dfw">
    <p v-if="!live && !events.length" class="dfw-empty">
      The feed reads the suite's event bus — it runs in the app, not in a plain browser.
    </p>
    <p v-else-if="!events.length" class="dfw-empty">Quiet so far. Everything the suite does lands here.</p>

    <ol v-else class="dfw-list">
      <li v-for="e in events" :key="e.id" class="dfw-row">
        <span class="dfw-time">{{ timeOf(e.ts) }}</span>
        <span class="dfw-source">{{ e.source }}</span>
        <span class="dfw-topic">{{ e.topic }}</span>
        <span class="dfw-detail">{{ summarize(e) }}</span>
      </li>
    </ol>
  </div>
</template>

<style scoped>
.dfw {
  height: 100%;
  overflow-y: auto;
  padding: 8px 14px;
}

.dfw-empty {
  margin: 24px auto;
  max-width: 52ch;
  text-align: center;
  color: var(--qss-text-muted);
  font-size: 12px;
}

.dfw-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.dfw-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 3px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--qss-border-subtle) 40%, transparent);
  font: 400 11px/1.6 var(--qss-font-mono);
}

.dfw-time {
  flex-shrink: 0;
  color: var(--qss-text-muted);
}
.dfw-source {
  flex-shrink: 0;
  width: 52px;
  color: var(--qss-text-muted);
}
.dfw-topic {
  flex-shrink: 0;
  color: var(--qss-text);
}
.dfw-detail {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text-muted);
}
</style>
