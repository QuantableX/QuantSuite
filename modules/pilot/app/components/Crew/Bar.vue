<script setup lang="ts">
/**
 * Above the orchestrator's message box (round 2, card 07): what its crew
 * waits on from the user. A decision gate — approve or reject, with an
 * optional note the orchestrator hears — and every worker that waits on an
 * approval or a question, answered right here with the worker's own
 * request panel (T3 #28516: child approvals surface in the parent).
 */
import { computed, reactive, watch } from 'vue'
import { useCrewStore } from '#pilot/stores/crew'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { usePilotStore } from '#pilot/stores/pilot'
import { gateParts, pendingGates } from '#pilot/utils/crew'

const props = defineProps<{ threadId: string }>()
const crews = useCrewStore()
const threads = useThreadStore()
const sb = useSidebarStore()
const store = usePilotStore()

const snap = computed(() => {
  const s = crews.ofThread(props.threadId)
  return s && s.crew.orchestratorId === props.threadId ? s : null
})
const gates = computed(() => pendingGates(snap.value))
const notes = reactive<Record<string, string>>({})
const busy = reactive<Record<string, boolean>>({})

async function decide(id: string, approve: boolean) {
  busy[id] = true
  if (await crews.decide(id, approve, notes[id] ?? '')) delete notes[id]
  busy[id] = false
}

/** Workers whose thread waits on the user, with their task. */
const waiting = computed(() =>
  (snap.value?.tasks ?? [])
    .filter((t) => t.workerId && !['done', 'failed', 'stopped'].includes(t.status) && sb.statusOf(t.workerId) === 'waiting')
    .map((t) => ({ task: t, id: t.workerId as string }))
)
// A waiting worker's requests need its view: load it once (events keep it current).
watch(
  () => waiting.value.map((w) => w.id),
  (ids) => {
    for (const id of ids) if (!threads.views[id]?.loaded) void threads.open(id)
  },
  { immediate: true }
)
const shownWorkers = computed(() => waiting.value.filter((w) => threads.pending(w.id).length > 0))
</script>

<template>
  <div v-if="gates.length || shownWorkers.length" class="qp-cbar">
    <div v-for="g in gates" :key="g.id" class="qp-gate" role="group" :aria-label="`Decision: ${gateParts(g.text).name}`">
      <div class="qp-gate-head">
        <span class="qp-gate-tag">Decision</span>
        <span class="qp-gate-name">{{ gateParts(g.text).name }}</span>
      </div>
      <div v-if="gateParts(g.text).question" class="qp-gate-q">{{ gateParts(g.text).question }}</div>
      <div class="qp-gate-row">
        <input v-model="notes[g.id]" class="qp-gate-note" placeholder="Note (optional)" title="A note the orchestrator hears with your decision" @keydown.enter.prevent="decide(g.id, true)" />
        <button class="qp-btn qp-btn-sm qp-btn-danger" :disabled="busy[g.id]" @click="decide(g.id, false)">Reject</button>
        <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="busy[g.id]" @click="decide(g.id, true)">Approve</button>
      </div>
    </div>
    <div v-for="w in shownWorkers" :key="w.id" class="qp-cworker">
      <div class="qp-cworker-head">
        <PilotAgentIcon :agent="w.task.provider || null" :size="12" status="waiting" />
        <span><b>{{ w.task.key }}</b> waits for you</span>
        <button class="qp-link" title="Open the worker's thread" @click="store.openSession(w.id)">Open</button>
      </div>
      <PilotThreadApprovals :thread-id="w.id" foreign />
    </div>
    <div v-if="crews.error" class="qp-cbar-error">{{ crews.error }}</div>
  </div>
</template>

<style scoped>
.qp-cbar { display: flex; flex-direction: column; gap: 6px; margin: 0 0 6px; }
.qp-gate {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 7%, var(--qss-bg-raised, #1f1f25));
}
.qp-gate-head { display: flex; align-items: baseline; gap: 8px; }
.qp-gate-tag { font-size: 11px; font-weight: 600; color: var(--qss-warning, #d29a3f); }
.qp-gate-name { font-size: 13px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-gate-q { font-size: 12.5px; line-height: 1.45; color: var(--qss-text, #d4d4d8); white-space: pre-wrap; }
.qp-gate-row { display: flex; align-items: center; gap: 6px; }
.qp-gate-note {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 6px;
  background: var(--qss-bg, #17171c);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
}
.qp-gate-note:focus { outline: none; border-color: var(--qss-accent, #7aa2f7); }
.qp-cworker { display: flex; flex-direction: column; gap: 4px; }
.qp-cworker-head { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-cworker-head b { color: var(--qss-text, #d4d4d8); font-weight: 600; }
.qp-cbar-error { font-size: 11.5px; color: var(--qss-error, #f87171); }
</style>
