<script setup lang="ts">
/**
 * The Crew view (round 2, card 08), beside Conversation | Board | GitHub:
 * the open thread's crew — or the newest one, or the thread's own native
 * subagents — as a live graph, with the message bus underneath: every
 * crew message with its time, the sender → receiver in their colours and
 * the text; filter by worker; a click jumps to the message in its thread.
 * The footer says how many agents and messages there are.
 */
import { computed, nextTick, ref, watch } from 'vue'
import { MessagesSquare, Maximize2 } from 'lucide-vue-next'
import { useCrewStore } from '#pilot/stores/crew'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useAppStore } from '#pilot/stores/app'
import { subagents } from '#pilot/utils/timeline'
import { party } from '#pilot/utils/crew'
import { busTime, CENTER_COLOR, colorOf, crewGraph, footerLine, jumpTarget, subagentGraph, type Graph } from '#pilot/utils/crewGraph'
import type { CrewMessage, CrewSnapshot } from '#pilot/types'

const crews = useCrewStore()
const store = usePilotStore()
const threads = useThreadStore()
const app = useAppStore()

/** `crew:<id>` or `subagents:<threadId>`; empty = follow the open thread. */
const picked = ref('')
const activeCrew = computed(() => crews.ofThread(store.activeId))
const newest = computed(() => [...crews.list].sort((a, b) => b.crew.createdAt - a.crew.createdAt)[0] ?? null)
const activeSubs = computed(() => (store.activeId ? subagents(threads.view(store.activeId)?.items ?? []) : []))
const providerOf = (id: string) => store.sessions.find((s) => s.id === id)?.provider
const source = computed<{ snap: CrewSnapshot | null; graph: Graph | null }>(() => {
  const p = picked.value
  if (p.startsWith('crew:')) {
    const snap = crews.crews[p.slice(5)] ?? null
    return { snap, graph: snap ? crewGraph(snap, providerOf) : null }
  }
  if (p.startsWith('subagents:') || (!p && !activeCrew.value && activeSubs.value.length)) {
    const t = store.active
    return { snap: null, graph: t ? subagentGraph({ id: t.id, title: t.title, provider: t.provider }, activeSubs.value) : null }
  }
  const snap = activeCrew.value ?? newest.value
  return { snap, graph: snap ? crewGraph(snap, providerOf) : null }
})
const snap = computed(() => source.value.snap)
const graph = computed(() => source.value.graph)
const options = computed(() => [
  ...[...crews.list].sort((a, b) => b.crew.createdAt - a.crew.createdAt).map((c) => ({ value: `crew:${c.crew.id}`, label: `${c.crew.objective || 'Crew'} — ${c.titles[c.crew.orchestratorId] || 'orchestrator'}` })),
  ...(store.active && activeSubs.value.length ? [{ value: `subagents:${store.active.id}`, label: `Subagents of ${store.active.title || 'this thread'}` }] : []),
])
const current = computed(() => picked.value || (snap.value ? `crew:${snap.value.crew.id}` : store.active && activeSubs.value.length ? `subagents:${store.active.id}` : ''))

// The open thread's subagents need its view.
watch(
  () => store.activeId,
  (id) => {
    if (id && !threads.views[id]?.loaded) void threads.open(id)
  },
  { immediate: true }
)

// ── The bus ──
const filter = ref('')
const bus = computed<CrewMessage[]>(() => {
  const all = snap.value?.messages ?? []
  return filter.value ? all.filter((m) => m.taskId === filter.value) : all
})
function colorFor(id: string): string {
  const s = snap.value
  if (!s) return 'var(--qss-text-secondary, #9a9aa5)'
  if (id === s.crew.orchestratorId) return CENTER_COLOR
  const member = s.members.find((m) => m.threadId === id)
  const i = member?.taskId ? s.tasks.findIndex((t) => t.id === member.taskId) : -1
  return i >= 0 ? colorOf(i) : 'var(--qss-text-secondary, #9a9aa5)'
}
const list = ref<HTMLElement | null>(null)
const stick = ref(true)
function onScroll() {
  const el = list.value
  if (el) stick.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}
watch(
  () => bus.value.length,
  async () => {
    if (!stick.value) return
    await nextTick()
    if (list.value) list.value.scrollTop = list.value.scrollHeight
  },
  { immediate: true }
)

async function jump(m: CrewMessage) {
  const s = snap.value
  if (!s) return
  const target = jumpTarget(m, s)
  if (!target || !store.sessions.some((x) => x.id === target.threadId)) return
  await openThread(target.threadId)
  // The find bar of that thread (parity 15) brings the message into view.
  await nextTick()
  setTimeout(() => window.dispatchEvent(new CustomEvent('qp-find-in-thread', { detail: target })), 350)
}
async function openThread(id: string) {
  app.setView('conversation')
  await store.openSession(id)
}

const graphRef = ref<{ fit: () => void } | null>(null)
</script>

<template>
  <div class="qp-crewview">
    <div class="qp-cv-head">
      <select v-if="options.length" class="qp-cv-pick" :value="current" aria-label="Show" @change="picked = ($event.target as HTMLSelectElement).value">
        <option v-for="o in options" :key="o.value" :value="o.value">{{ o.label }}</option>
      </select>
      <span class="qp-cv-hint">Wheel to zoom · drag to pan · double-click to fit</span>
      <button v-if="graph" class="qp-btn qp-btn-sm" title="Fit the graph to the view" @click="graphRef?.fit()"><Maximize2 :size="12" /> Fit</button>
      <button v-if="snap" class="qp-btn qp-btn-sm" :title="`Every message of this crew, whole (${snap.messageCount})`" @click="crews.logOpen = snap.crew.id"><MessagesSquare :size="12" /> Full log</button>
    </div>

    <template v-if="graph">
      <!-- One graph per crew: switching crews never replays old messages as new ones. -->
      <PilotCrewGraph :key="current" ref="graphRef" :graph="graph" :snap="snap" @open="openThread" />

      <section v-if="snap" class="qp-bus" aria-label="Message bus">
        <div class="qp-bus-filter" role="group" aria-label="Show the messages of">
          <button class="qp-bus-chip" :class="{ on: !filter }" @click="filter = ''">All</button>
          <button v-for="(t, i) in snap.tasks" :key="t.id" class="qp-bus-chip" :class="{ on: filter === t.id }" :style="{ '--c': colorOf(i) }" @click="filter = filter === t.id ? '' : t.id">
            <i />{{ t.key }}
          </button>
        </div>
        <div ref="list" class="qp-bus-list" @scroll="onScroll">
          <button v-for="m in bus" :key="m.id" class="qp-bus-line" :class="`k-${m.kind}`" :title="m.text" @click="jump(m)">
            <span class="qp-bus-time mono">{{ busTime(m.at) }}</span>
            <span class="qp-bus-who"><b :style="{ color: colorFor(m.fromId) }">{{ party(m.fromId, snap) }}</b> → <b :style="{ color: colorFor(m.toId) }">{{ party(m.toId, snap) }}</b></span>
            <span class="qp-bus-text">{{ m.text.replace(/\s+/g, ' ') }}</span>
          </button>
          <div v-if="!bus.length" class="qp-bus-empty">No messages yet.</div>
        </div>
        <div class="qp-bus-foot">{{ footerLine(snap) }}</div>
      </section>
      <div v-else class="qp-bus-foot">Native subagents of this thread — they report to it directly, without a message bus.</div>
    </template>

    <div v-else class="qp-cv-empty">
      <p class="qp-cv-empty-title">No crew yet</p>
      <p class="qp-cv-empty-text">Ask an agent to lead one — "split this into workers" — and it starts them with QuantMCP's crew tools. A thread whose agent runs subagents shows them here too.</p>
    </div>
  </div>
</template>

<style scoped>
.qp-crewview { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-cv-head { display: flex; align-items: center; gap: 8px; padding: 10px 16px 6px; }
.qp-cv-pick { min-width: 0; max-width: 420px; height: 26px; padding: 0 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 7px; background: var(--qss-bg-raised, #1f1f25); color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; }
.qp-cv-hint { flex: 1; min-width: 0; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-bus { flex: none; height: 34%; min-height: 150px; max-height: 320px; display: flex; flex-direction: column; border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-bus-filter { display: flex; flex-wrap: wrap; gap: 4px; padding: 8px 16px 4px; }
.qp-bus-chip { display: inline-flex; align-items: center; gap: 5px; height: 22px; padding: 0 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 11px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-bus-chip i { width: 7px; height: 7px; border-radius: 50%; background: var(--c); }
.qp-bus-chip.on { border-color: var(--qss-accent, #7aa2f7); color: var(--qss-text, #d4d4d8); }
.qp-bus-list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 8px; }
.qp-bus-line { display: flex; align-items: baseline; gap: 10px; width: 100%; padding: 2px 8px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12px; text-align: left; cursor: pointer; }
.qp-bus-line:hover { background: var(--qss-bg-hover, #313139); }
.qp-bus-time { flex: none; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-bus-who { flex: none; color: var(--qss-text-muted, #6e6e7a); white-space: nowrap; }
.qp-bus-who b { font-weight: 600; }
.qp-bus-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-secondary, #9a9aa5); }
.k-ask .qp-bus-text,
.k-gate .qp-bus-text,
.k-escalation .qp-bus-text { color: var(--qss-warning, #d29a3f); }
.k-failed .qp-bus-text { color: var(--qss-error, #f87171); }
.k-done .qp-bus-text { color: var(--qss-text, #d4d4d8); }
.qp-bus-empty { padding: 8px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-bus-foot { flex: none; padding: 6px 16px 10px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-cv-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; padding: 24px; text-align: center; }
.qp-cv-empty-title { margin: 0; font-size: 14px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-cv-empty-text { margin: 0; max-width: 460px; font-size: 12.5px; line-height: 1.5; color: var(--qss-text-secondary, #9a9aa5); }
</style>
