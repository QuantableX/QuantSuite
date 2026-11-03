<script setup lang="ts">
/**
 * The live orchestration graph (round 2, card 08): the orchestrator in the
 * middle, its workers around it (radial, further rings past twelve), a
 * dotted edge to each. Every real message sends a particle along its edge,
 * sender → receiver; idle edges stay calm. With reduced motion an edge only
 * lights up briefly. Full size: zoom with the wheel, pan by dragging the
 * background, double-click fits. Compact (the right panel): icons only.
 *
 * Performance: a particle is one small element moved by the Web Animations
 * API with `transform` — the compositor runs it, no script per frame — and
 * only while the graph is on screen (not hidden, not a deactivated
 * KeepAlive page); highlights switch without transitions, bars scale.
 * Driven by the crew's `pilot-crew` event — no polling. A one-second clock
 * ticks the elapsed times only while a worker works.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, reactive, ref, watch } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { CENTER_COLOR, elapsed, fitScale, radialLayout, STATE_LABEL, travelOf, type Graph, type GraphNode, type NodeState } from '#pilot/utils/crewGraph'
import { useMascotStore } from '#pilot/stores/mascot'
import type { CrewSnapshot, LiveState } from '#pilot/types'

const props = defineProps<{ graph: Graph; snap?: CrewSnapshot | null; compact?: boolean }>()
/** The mascot leads the crew (round 2, card 05): it juggles when three or more work. */
const mascot = useMascotStore()
function faceOf(s: NodeState): LiveState {
  return s === 'working' ? 'working' : s === 'waiting' ? 'waiting' : s === 'error' ? 'error' : 'idle'
}
const emit = defineEmits<{ open: [threadId: string] }>()

const NODE = computed(() => (props.compact ? { w: 46, h: 46 } : { w: 176, h: 78 }))
const CENTER = computed(() => (props.compact ? { w: 54, h: 54 } : { w: 268, h: 70 }))

// ── Size, layout, zoom and pan ──
const stage = ref<HTMLElement | null>(null)
const size = reactive({ w: 0, h: 0 })
let ro: ResizeObserver | null = null
const layout = computed(() => radialLayout(props.graph.nodes.length, Math.max(size.w, 200), Math.max(size.h, 160), NODE.value.w, NODE.value.h, props.compact ? 6 : 18, CENTER.value.w, CENTER.value.h))
/** User zoom/pan; null = fit. */
const view = ref<{ k: number; x: number; y: number } | null>(null)
const fitted = computed(() => {
  const k = fitScale(layout.value, { width: size.w, height: size.h })
  return { k, x: (size.w - layout.value.width * k) / 2, y: (size.h - layout.value.height * k) / 2 }
})
const t = computed(() => view.value ?? fitted.value)
const worldStyle = computed(() => ({ width: `${layout.value.width}px`, height: `${layout.value.height}px`, transform: `translate(${t.value.x}px, ${t.value.y}px) scale(${t.value.k})` }))

function onWheel(e: WheelEvent) {
  if (props.compact) return
  e.preventDefault()
  const cur = t.value
  const k = Math.min(2, Math.max(0.35, cur.k * (e.deltaY < 0 ? 1.1 : 1 / 1.1)))
  const r = stage.value!.getBoundingClientRect()
  const px = e.clientX - r.left
  const py = e.clientY - r.top
  // Zoom around the pointer.
  view.value = { k, x: px - ((px - cur.x) * k) / cur.k, y: py - ((py - cur.y) * k) / cur.k }
}
const pan = ref<{ id: number; x: number; y: number; ox: number; oy: number } | null>(null)
function onDown(e: PointerEvent) {
  if (props.compact || e.button !== 0 || (e.target as HTMLElement).closest('.qp-gn, .qp-gc')) return
  const cur = t.value
  pan.value = { id: e.pointerId, x: e.clientX, y: e.clientY, ox: cur.x, oy: cur.y }
  view.value = { ...cur }
  try {
    stage.value?.setPointerCapture(e.pointerId)
  } catch {
    /* synthetic pointer */
  }
}
function onMove(e: PointerEvent) {
  const p = pan.value
  if (!p || p.id !== e.pointerId || !view.value) return
  view.value = { ...view.value, x: p.ox + e.clientX - p.x, y: p.oy + e.clientY - p.y }
}
function onUp(e: PointerEvent) {
  if (pan.value?.id === e.pointerId) pan.value = null
}
function fit() {
  view.value = null
}
defineExpose({ fit })

// ── Nodes ──
const pos = (i: number) => layout.value.nodes[i] ?? layout.value.center
function nodeStyle(n: GraphNode, i: number) {
  const p = pos(i)
  return { left: `${p.x - NODE.value.w / 2}px`, top: `${p.y - NODE.value.h / 2}px`, width: `${NODE.value.w}px`, height: `${NODE.value.h}px`, '--c': n.color }
}
const centerStyle = computed(() => {
  const c = layout.value.center
  return { left: `${c.x - CENTER.value.w / 2}px`, top: `${c.y - CENTER.value.h / 2}px`, width: `${CENTER.value.w}px`, height: `${CENTER.value.h}px`, '--c': CENTER_COLOR }
})
function iconStatus(s: NodeState): 'working' | 'waiting' | 'error' | 'idle' {
  return s === 'working' ? 'working' : s === 'waiting' ? 'waiting' : s === 'error' ? 'error' : 'idle'
}
const hovered = ref<string | null>(null)

// The one-second clock: only while a worker works and the graph is on screen.
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null
const anyWorking = computed(() => props.graph.nodes.some((n) => n.state === 'working'))
function syncClock() {
  const want = visible.value && anyWorking.value && !props.compact
  if (want && !clock) clock = setInterval(() => (now.value = Date.now()), 1000)
  if (!want && clock) {
    clearInterval(clock)
    clock = null
  }
}
function timeOf(n: GraphNode): string {
  if (!n.startedAt) return ''
  return elapsed((n.finishedAt ?? now.value) - n.startedAt)
}

function open(threadId: string | null) {
  if (threadId) emit('open', threadId)
}

// ── Visibility: the loop and the clock run only on screen ──
const active = ref(false)
const pageVisible = ref(typeof document === 'undefined' ? true : !document.hidden)
const visible = computed(() => active.value && pageVisible.value)
function onVisibility() {
  pageVisible.value = !document.hidden
}
watch(visible, (v) => {
  if (!v) dropParticles()
  syncClock()
})
watch(anyWorking, syncClock)

// ── Particles: compositor animations of plain elements ──
const reduced = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
const pgroup = ref<HTMLElement | null>(null)
const particles = new Set<Animation>()
const MAX_PARTICLES = 48
const hot = reactive<Record<string, number>>({})
const centerHot = ref(0)

function endpoints(nodeId: string, inward: boolean) {
  const i = props.graph.nodes.findIndex((n) => n.id === nodeId)
  if (i < 0) return null
  const a = layout.value.center
  const b = pos(i)
  return inward ? { x1: b.x, y1: b.y, x2: a.x, y2: a.y } : { x1: a.x, y1: a.y, x2: b.x, y2: b.y }
}
function light(nodeId: string, ms: number) {
  const until = Date.now() + ms
  hot[nodeId] = until
  setTimeout(() => {
    if (hot[nodeId] === until) delete hot[nodeId]
  }, ms)
}
function glowCenter(ms = 700) {
  const until = Date.now() + ms
  centerHot.value = until
  setTimeout(() => {
    if (centerHot.value === until) centerHot.value = 0
  }, ms)
}
function spawn(nodeId: string, inward: boolean) {
  if (!visible.value) return
  const dur = props.compact ? 700 : 1100
  light(nodeId, (reduced ? 600 : dur + 250))
  if (inward) setTimeout(() => glowCenter(), reduced ? 0 : dur)
  const e = endpoints(nodeId, inward)
  if (reduced || !e || !pgroup.value || particles.size >= MAX_PARTICLES) return
  const node = props.graph.nodes.find((n) => n.id === nodeId)
  const el = document.createElement('span')
  el.className = 'qp-gp'
  el.style.background = inward ? (node?.color ?? CENTER_COLOR) : CENTER_COLOR
  pgroup.value.appendChild(el)
  const at = (x: number, y: number) => `translate(${x}px, ${y}px)`
  const anim = el.animate(
    [
      { transform: at(e.x1, e.y1), opacity: 0 },
      { transform: at(e.x1 + (e.x2 - e.x1) * 0.1, e.y1 + (e.y2 - e.y1) * 0.1), opacity: 1, offset: 0.1 },
      { transform: at(e.x1 + (e.x2 - e.x1) * 0.9, e.y1 + (e.y2 - e.y1) * 0.9), opacity: 1, offset: 0.9 },
      { transform: at(e.x2, e.y2), opacity: 0 },
    ],
    { duration: dur, easing: 'ease-in-out' }
  )
  particles.add(anim)
  const done = () => {
    particles.delete(anim)
    el.remove()
  }
  anim.onfinish = done
  anim.oncancel = done
}
function dropParticles() {
  for (const a of [...particles]) a.cancel()
  particles.clear()
}

// New messages of a crew: one particle each (a burst is staggered).
let lastN = -1
watch(
  () => props.snap?.messages.at(-1)?.n ?? 0,
  () => {
    const snap = props.snap
    if (!snap || props.graph.kind !== 'crew') return
    const fresh = lastN < 0 ? [] : snap.messages.filter((m) => m.n > lastN)
    lastN = snap.messages.at(-1)?.n ?? 0
    fresh.slice(-12).forEach((m, i) => {
      const tr = travelOf(m, snap)
      if (!tr) return
      setTimeout(() => ('center' in tr ? glowCenter() : spawn(tr.node, tr.inward)), i * 140)
    })
  },
  { immediate: true }
)
// Subagents: a particle out when one starts, back when it finishes.
let lastStates: Record<string, NodeState> | null = null
watch(
  () => (props.graph.kind === 'subagents' ? props.graph.nodes.map((n) => `${n.id}:${n.state}`).join('|') : ''),
  () => {
    if (props.graph.kind !== 'subagents') return
    const now: Record<string, NodeState> = Object.fromEntries(props.graph.nodes.map((n) => [n.id, n.state]))
    if (lastStates) {
      for (const [id, s] of Object.entries(now)) {
        const was = lastStates[id]
        if (!was && s === 'working') spawn(id, false)
        else if (was === 'working' && s !== 'working') spawn(id, true)
      }
    }
    lastStates = now
  },
  { immediate: true }
)

onMounted(() => {
  active.value = inActiveKeepAliveTree()
  document.addEventListener('visibilitychange', onVisibility)
  ro = new ResizeObserver(() => {
    if (!stage.value) return
    size.w = stage.value.clientWidth
    size.h = stage.value.clientHeight
  })
  if (stage.value) ro.observe(stage.value)
  syncClock()
})
onActivated(() => (active.value = true))
onDeactivated(() => (active.value = false))
onBeforeUnmount(() => {
  active.value = false
  document.removeEventListener('visibilitychange', onVisibility)
  ro?.disconnect()
  dropParticles()
  if (clock) clearInterval(clock)
})
</script>

<template>
  <div
    ref="stage"
    class="qp-graph"
    :class="{ compact, panning: !!pan }"
    @wheel="onWheel"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
    @dblclick.self="fit"
  >
    <div class="qp-graph-world" :style="worldStyle">
      <svg class="qp-graph-edges" :width="layout.width" :height="layout.height" aria-hidden="true">
        <line
          v-for="(n, i) in graph.nodes"
          :key="n.id"
          :x1="layout.center.x"
          :y1="layout.center.y"
          :x2="pos(i).x"
          :y2="pos(i).y"
          class="qp-ge"
          :class="{ hot: hot[n.id], [`s-${n.state}`]: true }"
          :style="{ '--c': n.color }"
        />
      </svg>
      <div ref="pgroup" class="qp-graph-particles" aria-hidden="true" />

      <!-- The orchestrator -->
      <button class="qp-gc" :class="{ hot: centerHot, [`s-${graph.center.state}`]: true }" :style="centerStyle" :title="`${graph.center.title} — ${graph.center.sub}`" @click="open(graph.center.threadId)">
        <PilotAgentIcon :agent="graph.center.provider || null" :size="compact ? 18 : 18" :status="iconStatus(graph.center.state)" />
        <span v-if="!compact && mascot.prefs.on" class="qp-gc-face" aria-hidden="true">
          <PilotFace :state="faceOf(graph.center.state)" :busy="graph.nodes.filter((n) => n.state === 'working').length" :agent="graph.center.provider || null" :interactive="false" />
        </span>
        <span v-if="!compact" class="qp-gc-text">
          <span class="qp-gc-title">{{ graph.center.title }}</span>
          <span class="qp-gc-sub">{{ graph.center.sub }}</span>
        </span>
      </button>

      <!-- The workers -->
      <button
        v-for="(n, i) in graph.nodes"
        :key="n.id"
        class="qp-gn"
        :class="[`s-${n.state}`, { hot: hot[n.id], link: !!n.threadId }]"
        :style="nodeStyle(n, i)"
        :aria-label="`${n.name}: ${STATE_LABEL[n.state]}${n.progress !== null ? `, ${n.progress}%` : ''}`"
        @click="open(n.threadId)"
        @pointerenter="hovered = n.id"
        @pointerleave="hovered === n.id && (hovered = null)"
      >
        <template v-if="compact">
          <PilotAgentIcon :agent="n.provider || null" :size="16" :status="iconStatus(n.state)" />
          <span class="qp-gn-mini">{{ n.name }}</span>
          <span v-if="n.progress !== null" class="qp-gn-minipct mono">{{ n.progress }}%</span>
        </template>
        <template v-else>
          <span class="qp-gn-row">
            <PilotAgentIcon :agent="n.provider || null" :size="14" :status="iconStatus(n.state)" />
            <span class="qp-gn-name">{{ n.name }}</span>
            <span v-if="n.attempts > 1" class="qp-gn-try mono" :title="`attempt ${n.attempts}`">#{{ n.attempts }}</span>
            <span v-if="n.progress !== null" class="qp-gn-pct mono">{{ n.progress }}%</span>
          </span>
          <span class="qp-gn-task">{{ n.task }}</span>
          <span class="qp-gn-foot">
            <span v-if="n.progress !== null" class="qp-gn-bar"><i :style="{ transform: `scaleX(${n.progress / 100})` }" /></span>
            <span v-else class="qp-gn-fill" />
            <span class="qp-gn-state">{{ STATE_LABEL[n.state] }}</span>
            <span class="qp-gn-time mono">{{ timeOf(n) }}</span>
          </span>
        </template>
        <span v-if="hovered === n.id && n.latest && !compact" class="qp-gn-hover" role="tooltip">{{ n.latest }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.qp-graph { position: relative; flex: 1; min-height: 0; overflow: hidden; cursor: grab; user-select: none; }
.qp-graph.compact { cursor: default; height: 190px; flex: none; }
.qp-graph.panning { cursor: grabbing; }
.qp-graph-world { position: absolute; left: 0; top: 0; transform-origin: 0 0; }
.qp-graph-edges { position: absolute; left: 0; top: 0; overflow: visible; pointer-events: none; }
.qp-ge { stroke: var(--c); stroke-width: 1.2; stroke-dasharray: 2 6; stroke-linecap: round; opacity: 0.32; }
.qp-ge.s-idle,
.qp-ge.s-stopped { opacity: 0.16; }
.qp-ge.hot { opacity: 0.95; stroke-width: 1.8; }
.compact .qp-ge { stroke-width: 1; }
.qp-graph-particles { position: absolute; left: 0; top: 0; width: 0; height: 0; pointer-events: none; }
:deep(.qp-gp) { position: absolute; left: -3.5px; top: -3.5px; width: 7px; height: 7px; border-radius: 50%; will-change: transform, opacity; }
.compact :deep(.qp-gp) { left: -2.5px; top: -2.5px; width: 5px; height: 5px; }

.qp-gc {
  position: absolute;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
  border: 1px solid color-mix(in srgb, var(--c) 70%, transparent);
  border-radius: 14px;
  background: color-mix(in srgb, var(--c) 10%, var(--qss-bg-raised, #1f1f25));
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--c) 10%, transparent), 0 8px 24px rgba(0, 0, 0, 0.35);
  color: var(--qss-text, #d4d4d8);
  text-align: left;
  cursor: pointer;
}
.qp-gc.hot { box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 28%, transparent), 0 8px 24px rgba(0, 0, 0, 0.35); }
.compact .qp-gc { justify-content: center; padding: 0; border-radius: 50%; }
.qp-gc-text { display: flex; flex-direction: column; min-width: 0; }
.qp-gc-face { flex: none; width: 34px; margin: -6px -4px -6px -6px; }
.qp-gc-title { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-gc-sub { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.qp-gn {
  position: absolute;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
  padding: 7px 10px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-left: 3px solid var(--c);
  border-radius: 10px;
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text, #d4d4d8);
  text-align: left;
  cursor: default;
}
.qp-gn.link { cursor: pointer; }
.qp-gn.link:hover { border-color: color-mix(in srgb, var(--c) 60%, var(--qss-border-subtle, #35353d)); border-left-color: var(--c); }
.qp-gn.hot { box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 22%, transparent); }
.qp-gn.s-stopped,
.qp-gn.s-idle { opacity: 0.72; }
.qp-gn-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
.qp-gn-name { flex: 1; min-width: 0; font-size: 12.5px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-gn-try { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-gn-pct { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-gn-task { font-size: 11px; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-gn-foot { display: flex; align-items: center; gap: 6px; }
.qp-gn-bar { flex: 1; height: 3px; border-radius: 2px; background: var(--qss-border-subtle, #35353d); overflow: hidden; }
.qp-gn-bar i { display: block; height: 100%; background: var(--c); transform-origin: left; transition: transform 0.4s ease; }
.qp-gn-fill { flex: 1; }
.qp-gn-state { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.s-working .qp-gn-state { color: var(--qss-accent, #7aa2f7); }
.s-waiting .qp-gn-state { color: var(--qss-warning, #d29a3f); }
.s-done .qp-gn-state { color: var(--qss-success, #34d399); }
.s-error .qp-gn-state { color: var(--qss-error, #f87171); }
.qp-gn.s-error { border-color: color-mix(in srgb, var(--qss-error, #f87171) 45%, transparent); border-left-color: var(--c); }
.qp-gn-time { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); min-width: 34px; text-align: right; }
.qp-gn-hover {
  position: absolute;
  left: 0;
  top: calc(100% + 6px);
  z-index: 2;
  width: max(100%, 240px);
  padding: 6px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  background: var(--qss-bg-overlay, #26262d);
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.4);
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--qss-text, #d4d4d8);
  white-space: normal;
}

.compact .qp-gn { align-items: center; justify-content: center; gap: 1px; padding: 0; border: none; border-radius: 50%; background: transparent; }
.qp-gn-mini { max-width: 64px; font-size: 10.5px; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-gn-minipct { font-size: 10px; color: var(--qss-text-muted, #6e6e7a); }
.compact .s-working .qp-gn-mini { color: var(--qss-text, #d4d4d8); }
</style>
