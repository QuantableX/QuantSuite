<script setup lang="ts">
/**
 * A thread's crew in the right panel (round 2, card 07): the objective,
 * every task with its worker, state and progress, and what the user can
 * do — open a worker, retry it, swap its agent, stop it or the whole crew,
 * resume, read the message log. A failed or stalled worker stays in view
 * with its reason. Shown for the orchestrator and for each of its workers.
 */
import { computed, ref } from 'vue'
import { useCrewStore } from '#pilot/stores/crew'
import { usePilotStore } from '#pilot/stores/pilot'
import { useAppStore } from '#pilot/stores/app'
import { counts, iconStatus, STATUS_LABEL, settled, statusTone, taskDetail } from '#pilot/utils/crew'
import { crewGraph } from '#pilot/utils/crewGraph'
import type { AgentSetup } from '#pilot/utils/agentMenu'
import type { CrewTask } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const crews = useCrewStore()
const store = usePilotStore()
const app = useAppStore()

const snap = computed(() => crews.ofThread(props.threadId))
const crew = computed(() => snap.value?.crew ?? null)
const tasks = computed(() => snap.value?.tasks ?? [])
const c = computed(() => counts(tasks.value))
const isLead = computed(() => crew.value?.orchestratorId === props.threadId)
/** The compact live graph (round 2, card 08). */
const graph = computed(() => (snap.value ? crewGraph(snap.value, (id) => store.sessions.find((s) => s.id === id)?.provider) : null))
const leadTitle = computed(() => (crew.value ? snap.value?.titles[crew.value.orchestratorId] || 'Orchestrator' : ''))
const canResume = computed(() => crew.value && (crew.value.status === 'stopped' || (crew.value.status === 'running' && c.value.running === 0 && tasks.value.some((t) => t.status === 'waiting'))))

function open(id: string | null) {
  if (!id) return
  app.setView('conversation')
  void store.openSession(id)
}

const busy = ref<string | null>(null)
async function act(key: string, fn: () => Promise<boolean>) {
  busy.value = key
  await fn()
  busy.value = null
}

// Swap a worker's agent: the agent menu, then a retry with that agent.
const swapFor = ref<CrewTask | null>(null)
const swapAnchor = ref<HTMLElement | null>(null)
function askSwap(t: CrewTask, e: MouseEvent) {
  swapAnchor.value = e.currentTarget as HTMLElement
  swapFor.value = t
}
function pickSwap(setup: AgentSetup) {
  const t = swapFor.value
  swapFor.value = null
  if (t) void act(t.id, () => crews.retry(t, setup.provider, setup.model || undefined))
}

function pct(t: CrewTask): number {
  return t.status === 'done' ? 100 : Math.max(0, Math.min(100, t.progress))
}
</script>

<template>
  <div v-if="snap && crew" class="qp-crew">
    <div class="qp-crew-head">
      <span class="qp-crew-obj" :title="crew.objective">{{ crew.objective || 'Crew' }}</span>
      <span class="qp-crew-count" :title="`${c.done} of ${c.total} tasks done · ${snap.maxParallel} workers at once`"><span class="mono">{{ c.done }}/{{ c.total }}</span> done</span>
    </div>
    <div v-if="!isLead" class="qp-crew-lead">
      Worker of <button class="qp-link" :title="`Open ${leadTitle}`" @click="open(crew.orchestratorId)">{{ leadTitle }}</button>
    </div>
    <div v-if="crew.status === 'stopped'" class="qp-crew-state">Stopped</div>
    <PilotCrewGraph v-if="graph && tasks.length" :graph="graph" :snap="snap" compact @open="open" />

    <div v-for="t in tasks" :key="t.id" class="qp-ctask" :class="[`is-${statusTone(t.status)}`, { me: t.workerId === threadId }]">
      <button class="qp-ctask-main" :disabled="!t.workerId" :title="t.workerId ? `Open ${t.key}'s thread` : t.note || t.spec" @click="open(t.workerId)">
        <PilotAgentIcon :agent="t.provider || null" :size="12" :status="iconStatus(t.status)" />
        <span class="qp-ctask-key">{{ t.key }}</span>
        <span class="qp-ctask-state">{{ STATUS_LABEL[t.status] }}</span>
        <span v-if="t.status === 'running' || t.status === 'asking' || t.status === 'stalled' || t.status === 'done'" class="qp-ctask-pct mono">{{ pct(t) }}%</span>
      </button>
      <div v-if="!settled(t.status) && t.status !== 'waiting'" class="qp-ctask-bar"><span :style="{ transform: `scaleX(${pct(t) / 100})` }" /></div>
      <div v-if="taskDetail(t)" class="qp-ctask-note" :title="taskDetail(t)">{{ taskDetail(t) }}</div>
      <div class="qp-ctask-actions">
        <span v-if="t.attempts > 1" class="qp-ctask-try">attempt <span class="mono">{{ t.attempts }}</span></span>
        <button
          v-if="t.status === 'failed' || t.status === 'stalled' || t.status === 'stopped'"
          class="qp-mini"
          :disabled="busy === t.id"
          title="Start this task again with a new worker; it hears why the last attempt ended"
          @click="act(t.id, () => crews.retry(t))"
        >
          Retry
        </button>
        <button v-if="t.status !== 'done'" class="qp-mini" :disabled="busy === t.id" title="Start it again with another agent" @click="askSwap(t, $event)">Swap agent</button>
        <button v-if="!settled(t.status)" class="qp-mini" :disabled="busy === t.id" title="Stop this worker" @click="act(t.id, () => crews.stop(crew!.id, t.id))">Stop</button>
      </div>
    </div>

    <div v-if="crews.error" class="qp-crew-error">{{ crews.error }}</div>
    <div class="qp-actions">
      <button class="qp-mini" title="The crew as a live graph with its message bus" @click="app.setView('crew')">Crew view</button>
      <button class="qp-mini" :title="`Every message of this crew (${snap.messageCount})`" @click="crews.logOpen = crew.id">Message log</button>
      <button v-if="canResume" class="qp-mini" title="Start the tasks that are ready" :disabled="busy === 'crew'" @click="act('crew', () => crews.resume(crew!.id))">Resume</button>
      <button
        v-if="crew.status === 'running' && c.open > 0"
        class="qp-mini danger"
        title="Stop every worker and the orchestrator's turn"
        :disabled="busy === 'crew'"
        @click="act('crew', () => crews.stop(crew!.id))"
      >
        Stop crew
      </button>
    </div>

    <PilotAgentMenu v-if="swapFor" :anchor="swapAnchor" title="Swap agent" :provider="swapFor.provider || null" :model="swapFor.model" @pick="pickSwap" @close="swapFor = null" />
  </div>
</template>

<style scoped>
.qp-crew { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.qp-crew-head { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
.qp-crew-obj { flex: 1; min-width: 0; font-size: 12px; font-weight: 600; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-crew-count { flex: none; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-crew-lead { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-crew-state { font-size: 11px; color: var(--qss-warning, #d29a3f); }
.qp-crew-error { font-size: 11px; color: var(--qss-error, #f87171); }
.qp-ctask { display: flex; flex-direction: column; gap: 3px; padding: 5px 6px; border-radius: 6px; min-width: 0; }
.qp-ctask:hover,
.qp-ctask.me { background: var(--qss-bg-hover, #313139); }
.qp-ctask-main { display: flex; align-items: center; gap: 7px; min-width: 0; padding: 0; border: none; background: transparent; color: inherit; text-align: left; cursor: pointer; }
.qp-ctask-main:disabled { cursor: default; }
.qp-ctask-key { flex: 1; min-width: 0; font-size: 12px; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ctask-state { flex: none; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.is-run .qp-ctask-state { color: var(--qss-accent, #7aa2f7); }
.is-wait .qp-ctask-state,
.is-warn .qp-ctask-state { color: var(--qss-warning, #d29a3f); }
.is-ok .qp-ctask-state { color: var(--qss-success, #34d399); }
.is-bad .qp-ctask-state { color: var(--qss-error, #f87171); }
.qp-ctask-pct { flex: none; width: 34px; text-align: right; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ctask-bar { height: 3px; border-radius: 2px; background: var(--qss-border-subtle, #35353d); overflow: hidden; margin-left: 19px; }
.qp-ctask-bar span { display: block; height: 100%; background: var(--qss-accent, #7aa2f7); transform-origin: left; transition: transform 0.3s ease; }
.is-wait .qp-ctask-bar span,
.is-warn .qp-ctask-bar span { background: var(--qss-warning, #d29a3f); }
.qp-ctask-note { margin-left: 19px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
.is-bad .qp-ctask-note { color: var(--qss-error, #f87171); }
.qp-ctask-actions { display: none; align-items: center; gap: 4px; margin-left: 19px; flex-wrap: wrap; }
.qp-ctask:hover .qp-ctask-actions,
.qp-ctask:focus-within .qp-ctask-actions,
.is-bad .qp-ctask-actions,
.is-warn .qp-ctask-actions { display: flex; }
.qp-actions { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 2px; }
.qp-mini { padding: 2px 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-mini:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-mini:disabled { opacity: 0.45; cursor: default; }
.qp-mini.danger { color: var(--qss-error, #f87171); }
.qp-ctask-try { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); margin-right: 2px; }
</style>
