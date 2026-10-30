<script setup lang="ts">
/**
 * Subscription windows (card 18): per agent, each window as % used with
 * its reset countdown. Codex answers when asked (at most every five minutes,
 * Refresh asks now); Claude Code reports its windows during turns and the
 * last report stays, with its age. A window without a reading says so — it
 * is never shown as 0%. `compact` is `/usage` above the message box.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useUsageStore } from '#pilot/stores/usage'
import { usePilotStore } from '#pilot/stores/pilot'
import { countdown, windowName } from '#pilot/utils/usage'

const props = defineProps<{ provider?: string; compact?: boolean }>()
const usage = useUsageStore()
const store = usePilotStore()

const shown = computed(() => (usage.limits ?? []).filter((l) => !props.provider || l.provider === props.provider))

// The countdowns move on while the page is open.
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null
onMounted(() => (timer = setInterval(() => (now.value = Date.now()), 30_000)))
onBeforeUnmount(() => timer && clearInterval(timer))

function ago(at: number): string {
  const m = Math.round((now.value - at) / 60_000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.round(m / 60)
  return h < 48 ? `${h}h ago` : `${Math.round(h / 24)}d ago`
}
const clock = (at: number) => {
  const d = new Date(at)
  const time = d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  return d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString('en-US', { weekday: 'short' })} ${time}`
}
const level = (pct: number | null) => (pct === null ? 'none' : pct >= 100 ? 'out' : pct >= 90 ? 'high' : 'ok')
</script>

<template>
  <div class="qp-limits" :class="{ compact }">
    <div v-if="!usage.limits && usage.limitsLoading" class="qp-limits-empty">Asking for the current windows…</div>
    <section v-for="l in shown" :key="l.provider" class="qp-limits-card">
      <header class="qp-limits-head">
        <span class="qp-limits-name">{{ store.adapterLabel(l.provider) }}</span>
        <span v-if="l.at" class="qp-limits-age">reported {{ ago(l.at) }}</span>
      </header>
      <div v-for="w in l.windows" :key="w.kind" class="qp-limit" :class="`is-${level(w.usedPct)}`">
        <span class="qp-limit-name">{{ windowName(w.kind) }}</span>
        <span class="qp-limit-bar" role="meter" :aria-valuenow="w.usedPct ?? undefined" aria-valuemin="0" aria-valuemax="100" :aria-label="`${windowName(w.kind)} used`">
          <span v-if="w.usedPct !== null" class="qp-limit-fill" :style="{ width: `${Math.min(100, w.usedPct)}%` }" />
        </span>
        <span class="qp-limit-pct">
          {{ w.usedPct === null ? 'not reported' : `${Math.round(w.usedPct)}% used` }}
          <em v-if="level(w.usedPct) === 'out'" class="qp-limit-flag">used up</em>
          <em v-else-if="level(w.usedPct) === 'high'" class="qp-limit-flag">almost used up</em>
        </span>
        <span class="qp-limit-reset">{{ w.resetsAt ? `resets in ${countdown(w.resetsAt, now)} · ${clock(w.resetsAt)}` : 'no reset time' }}</span>
      </div>
      <p v-if="!l.windows.length && !l.error" class="qp-limits-note">
        {{ l.provider === 'claude' ? 'Claude Code reports its windows during a turn — none reported yet. Send a message, then look again.' : 'No windows reported.' }}
      </p>
      <p v-if="l.error" class="qp-limits-note is-err">{{ l.error }}{{ l.windows.length ? ' — the last reading is shown.' : '' }}</p>
    </section>
    <p v-if="!compact" class="qp-limits-foot">Bars show how much of each window is used. Limits may include use of the same account in other apps.</p>
  </div>
</template>

<style scoped>
.qp-limits { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 18px; display: flex; flex-direction: column; gap: 12px; }
.qp-limits.compact { flex: none; padding: 0; overflow: visible; }
.qp-limits-card { padding: 12px 14px; border-radius: 10px; border: 1px solid var(--qss-border-subtle, #35353d); background: var(--qss-bg-raised, #1f1f25); display: flex; flex-direction: column; gap: 8px; }
.compact .qp-limits-card { padding: 0; border: none; background: transparent; gap: 5px; }
.qp-limits-head { display: flex; align-items: baseline; gap: 10px; }
.qp-limits-name { font-size: 13px; font-weight: 600; }
.compact .qp-limits-name { font-size: 12px; }
.qp-limits-age { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
/* Fixed text columns, so the bars and resets line up across windows and agents. */
.qp-limit { display: grid; grid-template-columns: 120px minmax(80px, 1fr) 190px 210px; align-items: center; gap: 12px; font-size: 12px; }
.compact .qp-limit { grid-template-columns: 96px minmax(60px, 1fr) 170px 200px; gap: 10px; }
.qp-limit-name { color: var(--qss-text-secondary, #9a9aa5); }
.qp-limit-bar { position: relative; height: 6px; border-radius: 3px; background: var(--qss-bg-card, #292930); overflow: hidden; }
.qp-limit-fill { position: absolute; inset: 0 auto 0 0; border-radius: 3px; background: var(--qss-text-secondary, #9a9aa5); }
.is-high .qp-limit-fill { background: var(--qss-warning, #d29a3f); }
.is-out .qp-limit-fill { background: var(--qss-error, #f87171); }
.qp-limit-pct { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text, #d4d4d8); text-align: right; }
.is-none .qp-limit-pct { font-family: inherit; color: var(--qss-text-muted, #6e6e7a); }
.qp-limit-flag { margin-left: 6px; font-family: var(--qss-font-sans, sans-serif); font-style: normal; font-size: 11px; color: var(--qss-warning, #d29a3f); }
.is-out .qp-limit-flag { color: var(--qss-error, #f87171); }
.qp-limit-reset { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); text-align: right; }
.qp-limits-note { margin: 0; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-limits-note.is-err { color: var(--qss-warning, #d29a3f); }
.qp-limits-empty,
.qp-limits-foot { margin: 0; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
</style>
