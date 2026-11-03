<script setup lang="ts">
/**
 * The connections between canvas windows (round 2, card 09), drawn in world
 * coordinates under the windows: a curve from the facing sides, coloured by
 * state — working flows (a dot runs along it), waiting is amber, error red,
 * done calm. A crew message travels its edge like in the crew graph
 * (card 08). Strokes keep their width at every zoom. A click selects an
 * edge; Disconnect (or Delete) removes a saved one. The line of a port drag
 * in progress is drawn here too.
 *
 * Dots run on the compositor (Web Animations, `transform`), only while the
 * canvas is on screen.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useCrewStore } from '#pilot/stores/crew'
import { travelOf } from '#pilot/utils/crewGraph'
import { connecting, useCanvasEdges, type DrawnEdge } from '../../utils/canvasLinks'
import { anchors, rectOf, type Pt } from '../../utils/edges'

const props = defineProps<{ scale: number }>()
const canvas = useCanvasStore()
const workspaces = useWorkspacesStore()
const crews = useCrewStore()
const { edges, windows } = useCanvasEdges()

const selected = ref<string | null>(null)
const selectedEdge = computed(() => edges.value.find((e) => e.id === selected.value) ?? null)
function select(e: DrawnEdge) {
  selected.value = selected.value === e.id ? null : e.id
}
function disconnect() {
  const e = selectedEdge.value
  if (!e || e.kind === 'crew' || !workspaces.contentWorkspaceId) return
  canvas.removeEdge(workspaces.contentWorkspaceId, e.id)
  selected.value = null
}
function onKey(ev: KeyboardEvent) {
  if (!selected.value) return
  if (ev.key === 'Escape') selected.value = null
  else if ((ev.key === 'Delete' || ev.key === 'Backspace') && !(ev.target as HTMLElement)?.closest('input, textarea, [contenteditable]')) {
    ev.preventDefault()
    disconnect()
  }
}

// The port drag's line: from the source window's facing side to the pointer.
const pending = computed(() => {
  const c = connecting.value
  if (!c) return null
  const w = windows.value.find((x) => x.id === c.from)
  if (!w) return null
  const a = anchors(rectOf(w), { x: c.x - 1, y: c.y - 1, width: 2, height: 2 })
  return `M ${a.p0.x} ${a.p0.y} L ${c.x} ${c.y}`
})

// ── Visibility ──
const active = ref(false)
const pageVisible = ref(typeof document === 'undefined' ? true : !document.hidden)
const visible = computed(() => active.value && pageVisible.value)
const onVisibility = () => (pageVisible.value = !document.hidden)
const reduced = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

// ── Dots: a loop per working edge, one-shot per crew message ──
const layer = ref<HTMLElement | null>(null)
const flows = new Map<string, { key: string; anim: Animation; el: HTMLElement }>()
const COLORS: Record<string, string> = { working: '#7aa2f7', waiting: '#d29a3f', error: '#f87171', done: '#34d399', idle: '#8a8a95' }

function keyframes(points: Pt[], reverse = false) {
  const pts = reverse ? [...points].reverse() : points
  return pts.map((p, i) => ({ transform: `translate(${p.x}px, ${p.y}px)`, opacity: i === 0 || i === pts.length - 1 ? 0 : 1 }))
}
function dot(color: string): HTMLElement {
  const el = document.createElement('span')
  el.className = 'cv-dot'
  el.style.background = color
  layer.value!.appendChild(el)
  return el
}
function syncFlows() {
  const want = new Map<string, DrawnEdge>()
  if (visible.value && !reduced && layer.value) for (const e of edges.value) if (e.state === 'working') want.set(e.id, e)
  for (const [id, f] of flows) {
    const e = want.get(id)
    if (!e || f.key !== e.geometry.d) {
      f.anim.cancel()
      f.el.remove()
      flows.delete(id)
    }
  }
  for (const [id, e] of want) {
    if (flows.has(id)) continue
    const el = dot(COLORS.working!)
    const anim = el.animate(keyframes(e.geometry.points), { duration: 1800, iterations: Infinity, easing: 'linear' })
    flows.set(id, { key: e.geometry.d, anim, el })
  }
}
function dropFlows() {
  for (const f of flows.values()) {
    f.anim.cancel()
    f.el.remove()
  }
  flows.clear()
}
watch([edges, visible], syncFlows)

// A crew message: one dot along its task's edge, sender → receiver.
const lastN = new Map<string, number>()
const flashing = ref<Record<string, number>>({})
function flash(id: string, ms: number) {
  const until = Date.now() + ms
  flashing.value = { ...flashing.value, [id]: until }
  setTimeout(() => {
    if (flashing.value[id] === until) {
      const next = { ...flashing.value }
      delete next[id]
      flashing.value = next
    }
  }, ms)
}
watch(
  () => crews.list.map((c) => `${c.crew.id}:${c.messages.at(-1)?.n ?? 0}`).join('|'),
  () => {
    for (const snap of crews.list) {
      const n = snap.messages.at(-1)?.n ?? 0
      const before = lastN.get(snap.crew.id)
      lastN.set(snap.crew.id, n)
      if (before === undefined || !visible.value) continue
      for (const m of snap.messages.filter((x) => x.n > before).slice(-8)) {
        const tr = travelOf(m, snap)
        if (!tr || 'center' in tr) continue
        const e = edges.value.find((x) => x.taskId === tr.node)
        if (!e) continue
        flash(e.id, reduced ? 600 : 1400)
        if (reduced || !layer.value) continue
        const el = dot(COLORS.working!)
        const a = el.animate(keyframes(e.geometry.points, tr.inward), { duration: 1100, easing: 'ease-in-out' })
        a.onfinish = () => el.remove()
        a.oncancel = () => el.remove()
      }
    }
  },
  { immediate: true }
)

onMounted(() => {
  active.value = inActiveKeepAliveTree()
  document.addEventListener('visibilitychange', onVisibility)
  window.addEventListener('keydown', onKey)
  syncFlows()
})
onActivated(() => (active.value = true))
onDeactivated(() => (active.value = false))
onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibility)
  window.removeEventListener('keydown', onKey)
  dropFlows()
})

const endR = computed(() => 3.5 / Math.max(0.2, props.scale))
const btnScale = computed(() => 1 / Math.max(0.2, props.scale))
</script>

<template>
  <div class="cv-edges" aria-hidden="false">
    <svg class="cv-edges-svg" width="1" height="1" overflow="visible">
      <g v-for="e in edges" :key="e.id" class="cv-edge" :class="[`s-${e.state}`, `k-${e.kind}`, { sel: selected === e.id, hot: flashing[e.id] }]">
        <path :d="e.geometry.d" class="cv-edge-hit" @pointerdown.stop @click.stop="select(e)" />
        <path :d="e.geometry.d" class="cv-edge-line" vector-effect="non-scaling-stroke" />
        <circle :cx="e.geometry.end.x" :cy="e.geometry.end.y" :r="endR" class="cv-edge-end" />
      </g>
      <path v-if="pending" :d="pending" class="cv-edge-pending" vector-effect="non-scaling-stroke" />
    </svg>
    <div ref="layer" class="cv-dots" />
    <button
      v-if="selectedEdge && selectedEdge.kind !== 'crew'"
      class="cv-edge-cut"
      :style="{ left: `${selectedEdge.geometry.mid.x}px`, top: `${selectedEdge.geometry.mid.y}px`, transform: `translate(-50%, -50%) scale(${btnScale})` }"
      title="Remove this connection (Delete)"
      @mousedown.stop
      @click.stop="disconnect"
    >
      Disconnect
    </button>
  </div>
</template>

<style scoped>
.cv-edges { position: absolute; left: 0; top: 0; width: 0; height: 0; z-index: 0; }
.cv-edges-svg { position: absolute; left: 0; top: 0; overflow: visible; pointer-events: none; }
.cv-edge-hit { fill: none; stroke: transparent; stroke-width: 14px; pointer-events: stroke; cursor: pointer; vector-effect: non-scaling-stroke; }
.cv-edge-line { fill: none; stroke: #9a9aa5; stroke-width: 2px; opacity: 0.8; }
.cv-edge-end { fill: #9a9aa5; opacity: 0.9; }
.s-idle .cv-edge-line { stroke-dasharray: 6 5; opacity: 0.75; }
.s-working .cv-edge-line { stroke: #7aa2f7; opacity: 0.85; }
.s-working .cv-edge-end { fill: #7aa2f7; }
.s-waiting .cv-edge-line { stroke: #d29a3f; opacity: 0.9; }
.s-waiting .cv-edge-end { fill: #d29a3f; }
.s-error .cv-edge-line { stroke: #f87171; opacity: 0.9; }
.s-error .cv-edge-end { fill: #f87171; }
.s-done .cv-edge-line { stroke: #34d399; opacity: 0.6; }
.s-done .cv-edge-end { fill: #34d399; opacity: 0.5; }
.k-crew .cv-edge-line { stroke-dasharray: 2 6; stroke-linecap: round; }
.k-crew.s-done .cv-edge-line,
.k-crew.s-working .cv-edge-line { stroke-dasharray: 2 6; }
.hot .cv-edge-line { opacity: 1; stroke-width: 2.4px; }
.sel .cv-edge-line { stroke-width: 2.6px; opacity: 1; }
.cv-edge-pending { fill: none; stroke: #7aa2f7; stroke-width: 1.8px; stroke-dasharray: 6 4; }
.cv-dots { position: absolute; left: 0; top: 0; width: 0; height: 0; pointer-events: none; }
:deep(.cv-dot) { position: absolute; left: -4px; top: -4px; width: 8px; height: 8px; border-radius: 50%; box-shadow: 0 0 6px currentColor; will-change: transform, opacity; }
.cv-edge-cut {
  position: absolute;
  z-index: 2;
  padding: 3px 9px;
  border: 1px solid var(--qc-border, #3f3f46);
  border-radius: 7px;
  background: var(--qc-bg-header, #1f1f25);
  color: var(--qc-text, #d4d4d8);
  font-size: 11.5px;
  white-space: nowrap;
  cursor: pointer;
  transform-origin: center;
}
.cv-edge-cut:hover { border-color: #f87171; color: #f87171; }
</style>
