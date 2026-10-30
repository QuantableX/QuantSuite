<script setup lang="ts">
/**
 * What happens to the thread beyond the next message (card 15), above the
 * box: a usage limit and its reset (Resume at reset, Snooze until reset), a
 * scheduled continuation, the history the next message carries to a new
 * agent, the conversation being open in the CLI's own terminal, and — for
 * Claude after a long pause on a large conversation — Compact first.
 */
import { computed, watch } from 'vue'
import { AlarmClock, ArrowRightLeft, Archive, Clock, SquareTerminal, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const store = usePilotStore()
const sidebar = useSidebarStore()

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const life = computed(() => view.value?.life ?? { resumeAt: null, limitResetAt: null, handoffFrom: null, tui: false })
const limited = computed(() => view.value?.state === 'limited')
const running = computed(() => Boolean(view.value?.turnId))
const agent = computed(() => store.adapterLabel(thread.value?.provider || 'claude'))

function when(ms: number): string {
  const d = new Date(ms)
  const time = d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  return d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${time}`
}

/** Claude's prompt cache is gone after an hour: compacting a large conversation first is cheaper. */
const suggestCompact = computed(() => {
  const t = thread.value
  const tokens = view.value?.usage.contextTokens ?? t?.contextTokens ?? 0
  return (t?.provider || 'claude') === 'claude' && !running.value && !life.value.tui && tokens > 200_000 && Date.now() - (t?.lastActivityAt ?? Date.now()) > 3_600_000
})
watch(
  suggestCompact,
  (on) => {
    if (on && view.value && view.value.compactFirst === null) view.value.compactFirst = true
  },
  { immediate: true }
)

function resumeAtReset() {
  if (life.value.limitResetAt) void threads.schedule(props.threadId, life.value.limitResetAt + 60_000)
}
function snoozeUntilReset() {
  const at = life.value.limitResetAt
  if (at) void sidebar.snooze(props.threadId, at, when(at))
}
</script>

<template>
  <div class="qp-life">
    <div v-if="life.tui" class="qp-life-row">
      <SquareTerminal :size="13" />
      <span class="qp-life-text">Open in the terminal — {{ agent }} has this conversation there. Close it to continue here.</span>
      <button class="qp-life-btn" title="Close the terminal and bring the conversation back" @click="threads.tuiDone(threadId)">Take it back</button>
    </div>

    <div v-else-if="limited" class="qp-life-row warn">
      <AlarmClock :size="13" />
      <span class="qp-life-text">
        Usage limit reached<template v-if="life.limitResetAt"> — resets at {{ when(life.limitResetAt) }}</template>.
        <template v-if="life.resumeAt">Continues by itself at {{ when(life.resumeAt) }}.</template>
      </span>
      <template v-if="life.resumeAt">
        <button class="qp-life-btn" @click="threads.schedule(threadId, null)">Cancel</button>
      </template>
      <template v-else-if="life.limitResetAt">
        <button class="qp-life-btn" title="Continue the turn when the allowance returns" @click="resumeAtReset">Resume at reset</button>
        <button class="qp-life-btn" title="Hide the thread until then" @click="snoozeUntilReset">Snooze until reset</button>
      </template>
    </div>

    <div v-else-if="life.resumeAt" class="qp-life-row">
      <Clock :size="13" />
      <span class="qp-life-text">Continues by itself at {{ when(life.resumeAt) }}.</span>
      <button class="qp-life-btn" @click="threads.schedule(threadId, null)">Cancel</button>
    </div>

    <div v-if="life.handoffFrom && !life.tui" class="qp-life-row muted">
      <ArrowRightLeft :size="13" />
      <span class="qp-life-text">
        The next message brings the conversation from {{ life.handoffFrom }} to {{ agent }} — the newest 200 messages, 64 KiB at most; tools and reasoning stay behind.
      </span>
    </div>

    <div v-if="suggestCompact && view" class="qp-life-row muted">
      <Archive :size="13" />
      <span class="qp-life-text">A long pause on a large conversation — compacting it first saves resending all of it.</span>
      <span class="qp-life-seg" role="radiogroup" aria-label="Next message">
        <button class="qp-life-seg-btn" role="radio" :aria-checked="view.compactFirst === true" :class="{ on: view.compactFirst === true }" @click="view.compactFirst = true">Compact first</button>
        <button class="qp-life-seg-btn" role="radio" :aria-checked="view.compactFirst === false" :class="{ on: view.compactFirst === false }" @click="view.compactFirst = false">Full</button>
      </span>
      <button class="qp-life-x" aria-label="Dismiss" @click="view.compactFirst = false"><X :size="12" /></button>
    </div>
  </div>
</template>

<style scoped>
.qp-life { display: flex; flex-direction: column; gap: 4px; max-width: 780px; width: 100%; margin: 0 auto; padding: 0 24px; }
.qp-life:empty { display: none; }
.qp-life-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg-raised, #1f1f25);
  font-size: 12px;
  color: var(--qss-text, #d4d4d8);
}
.qp-life-row.warn { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, var(--qss-border, #47474f)); color: var(--qss-warning, #d29a3f); }
.qp-life-row.warn .qp-life-text { color: var(--qss-text, #d4d4d8); }
.qp-life-row.muted { color: var(--qss-text-secondary, #9a9aa5); }
.qp-life-text { flex: 1; min-width: 0; line-height: 1.45; }
.qp-life-btn {
  flex-shrink: 0;
  padding: 2px 9px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-life-btn:hover { background: var(--qss-bg-hover, #313139); }
.qp-life-seg { display: inline-flex; flex-shrink: 0; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; overflow: hidden; }
.qp-life-seg-btn { padding: 2px 8px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; cursor: pointer; }
.qp-life-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-life-x { display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-life-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
