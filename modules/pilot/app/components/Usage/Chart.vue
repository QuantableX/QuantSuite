<script setup lang="ts">
/**
 * Stacked bars over the period (card 18): one bar per hour or day, one
 * segment per series in a fixed order and color (the validated categorical
 * slots), a 2px surface gap between segments, rounded data ends on the
 * baseline's far side, a recessive grid. Hover or arrow keys show every
 * series of a bar; the table view lists the same numbers without hovering.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { Bucket } from '#pilot/utils/usage'

export interface Series {
  id: string
  label: string
  /** A categorical slot: `--qp-viz-1` … */
  color: string
}

const props = defineProps<{
  buckets: Bucket[]
  series: Series[]
  bucket: 'hour' | 'day'
  format: (n: number) => string
  /** What the bars measure, for the table and screen readers. */
  label: string
}>()

const root = ref<HTMLElement | null>(null)
const width = ref(640)
const HEIGHT = 190
const PAD = { top: 10, right: 8, bottom: 22, left: 52 }
let observer: ResizeObserver | null = null
onMounted(() => {
  observer = new ResizeObserver(() => {
    if (root.value) width.value = Math.max(240, root.value.clientWidth)
  })
  if (root.value) observer.observe(root.value)
})
onBeforeUnmount(() => observer?.disconnect())

const stackOf = (b: Bucket) => props.series.reduce((s, x) => s + (b.values[x.id] ?? 0), 0)

/** A round top for the axis: 1, 2 or 5 times a power of ten. */
function niceMax(v: number): number {
  if (v <= 0) return 1
  const p = 10 ** Math.floor(Math.log10(v))
  for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p
  return 10 * p
}
const top = computed(() => niceMax(Math.max(0, ...props.buckets.map(stackOf))))
const plotW = computed(() => width.value - PAD.left - PAD.right)
const plotH = HEIGHT - PAD.top - PAD.bottom
const band = computed(() => plotW.value / Math.max(1, props.buckets.length))
const barW = computed(() => Math.max(2, Math.min(28, band.value * 0.68)))
const y = (v: number) => PAD.top + plotH - (v / top.value) * plotH
const ticks = computed(() => [0, 0.25, 0.5, 0.75, 1].map((f) => f * top.value))

function label(at: number, long = false): string {
  const d = new Date(at)
  if (props.bucket === 'hour') return d.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  return d.toLocaleDateString('en-US', long ? { weekday: 'short', month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric' })
}
/** At most about eight labels on the x axis. */
const every = computed(() => Math.max(1, Math.ceil(props.buckets.length / Math.max(2, Math.floor(plotW.value / 70)))))

interface Seg {
  key: string
  x: number
  y: number
  h: number
  color: string
  /** The bar's far end: rounded. */
  end: boolean
}
const GAP = 2
const bars = computed(() =>
  props.buckets.map((b, i) => {
    const x = PAD.left + i * band.value + (band.value - barW.value) / 2
    const segs: Seg[] = []
    let acc = 0
    const present = props.series.filter((s) => (b.values[s.id] ?? 0) > 0)
    present.forEach((s, j) => {
      const v = b.values[s.id] ?? 0
      const y0 = y(acc)
      const y1 = y(acc + v)
      acc += v
      // The gap comes off the upper segment's bottom; slivers keep at least a pixel.
      const h = Math.max(1, y0 - y1 - (j > 0 ? GAP : 0))
      segs.push({ key: s.id, x, y: y1, h, color: s.color, end: j === present.length - 1 })
    })
    return { at: b.at, x, segs }
  })
)

function path(s: Seg): string {
  const r = s.end ? Math.min(4, s.h / 2, barW.value / 2) : 0
  const w = barW.value
  return `M${s.x},${s.y + s.h}V${s.y + r}Q${s.x},${s.y} ${s.x + r},${s.y}H${s.x + w - r}Q${s.x + w},${s.y} ${s.x + w},${s.y + r}V${s.y + s.h}Z`
}

// ── Hover and keyboard ──
const active = ref<number | null>(null)
function onMove(e: PointerEvent) {
  const box = (e.currentTarget as SVGElement).getBoundingClientRect()
  const i = Math.floor((e.clientX - box.left - PAD.left) / band.value)
  active.value = i >= 0 && i < props.buckets.length ? i : null
}
function onKey(e: KeyboardEvent) {
  const n = props.buckets.length
  if (!n) return
  if (e.key === 'ArrowRight') active.value = Math.min(n - 1, (active.value ?? -1) + 1)
  else if (e.key === 'ArrowLeft') active.value = Math.max(0, (active.value ?? n) - 1)
  else if (e.key === 'Home') active.value = 0
  else if (e.key === 'End') active.value = n - 1
  else return
  e.preventDefault()
}
const tip = computed(() => {
  const i = active.value
  if (i === null) return null
  const b = props.buckets[i]
  const x = PAD.left + (i + 0.5) * band.value
  return {
    title: label(b.at, true),
    total: stackOf(b),
    rows: [...props.series].reverse().map((s) => ({ ...s, value: b.values[s.id] ?? 0 })),
    left: Math.min(Math.max(x, 90), width.value - 90),
  }
})

const table = ref(false)
</script>

<template>
  <div ref="root" class="qp-chart">
    <div class="qp-chart-head">
      <div class="qp-chart-legend" aria-hidden="true">
        <span v-for="s in series" :key="s.id" class="qp-chart-key"><i :style="{ background: s.color }" />{{ s.label }}</span>
      </div>
      <button class="qp-chart-toggle" :aria-pressed="table" @click="table = !table">{{ table ? 'Chart' : 'Table' }}</button>
    </div>

    <div v-if="!table" class="qp-chart-plot">
      <svg
        :width="width"
        :height="HEIGHT"
        role="img"
        :aria-label="`${label} per ${bucket}; use the arrow keys for each ${bucket}'s numbers`"
        tabindex="0"
        @pointermove="onMove"
        @pointerleave="active = null"
        @keydown="onKey"
        @blur="active = null"
      >
        <g class="qp-chart-grid">
          <template v-for="t in ticks" :key="t">
            <line :x1="PAD.left" :x2="width - PAD.right" :y1="y(t)" :y2="y(t)" />
            <text :x="PAD.left - 8" :y="y(t) + 4" text-anchor="end">{{ format(t) }}</text>
          </template>
        </g>
        <rect v-if="active !== null" class="qp-chart-band" :x="PAD.left + active * band" :y="PAD.top" :width="band" :height="plotH" />
        <g v-for="(b, i) in bars" :key="b.at" :class="{ dim: active !== null && active !== i }">
          <path v-for="s in b.segs" :key="s.key" :d="path(s)" :fill="s.color" />
        </g>
        <g class="qp-chart-x">
          <template v-for="(b, i) in buckets" :key="b.at">
            <text v-if="i % every === 0" :x="PAD.left + (i + 0.5) * band" :y="HEIGHT - 6" text-anchor="middle">{{ label(b.at) }}</text>
          </template>
        </g>
      </svg>
      <div v-if="tip" class="qp-chart-tip" :style="{ left: `${tip.left}px` }" role="status">
        <div class="qp-chart-tip-title">{{ tip.title }}</div>
        <div v-for="r in tip.rows" :key="r.id" class="qp-chart-tip-row">
          <i :style="{ background: r.color }" />
          <strong>{{ format(r.value) }}</strong>
          <span>{{ r.label }}</span>
        </div>
        <div class="qp-chart-tip-total"><strong>{{ format(tip.total) }}</strong><span>total</span></div>
      </div>
    </div>

    <div v-else class="qp-chart-table-wrap">
      <table class="qp-chart-table">
        <thead>
          <tr>
            <th>{{ bucket === 'hour' ? 'Hour' : 'Day' }}</th>
            <th v-for="s in series" :key="s.id">{{ s.label }}</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="b in [...buckets].reverse()" :key="b.at">
            <td>{{ label(b.at, true) }}</td>
            <td v-for="s in series" :key="s.id" class="num">{{ format(b.values[s.id] ?? 0) }}</td>
            <td class="num">{{ format(stackOf(b)) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.qp-chart { position: relative; min-width: 0; }
.qp-chart-head { display: flex; align-items: center; gap: 12px; margin-bottom: 6px; }
.qp-chart-legend { display: flex; flex-wrap: wrap; gap: 12px; flex: 1; min-width: 0; }
.qp-chart-key { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-chart-key i { width: 10px; height: 10px; border-radius: 2px; }
.qp-chart-toggle { padding: 2px 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-chart-toggle:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-chart-plot { position: relative; }
.qp-chart-plot svg { display: block; outline: none; }
.qp-chart-plot svg:focus-visible { box-shadow: 0 0 0 1px var(--qss-accent, #7aa2f7); border-radius: 6px; }
.qp-chart-grid line { stroke: var(--qss-border-subtle, #35353d); stroke-width: 1; }
.qp-chart-grid text,
.qp-chart-x text { fill: var(--qss-text-muted, #6e6e7a); font-size: 10.5px; font-family: var(--qss-font-mono, ui-monospace, monospace); }
.qp-chart-band { fill: var(--qss-bg-hover, #313139); opacity: 0.5; }
g.dim path { opacity: 0.55; }
.qp-chart-tip {
  position: absolute;
  top: 0;
  transform: translateX(-50%);
  min-width: 150px;
  padding: 7px 9px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  pointer-events: none;
  font-size: 11.5px;
  z-index: 2;
}
.qp-chart-tip-title { margin-bottom: 4px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-chart-tip-row,
.qp-chart-tip-total { display: flex; align-items: center; gap: 6px; padding: 1px 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-chart-tip-row i { width: 10px; height: 2px; border-radius: 1px; flex-shrink: 0; }
.qp-chart-tip-total { margin-top: 3px; padding-top: 4px; border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-chart-tip strong { font-family: var(--qss-font-mono, ui-monospace, monospace); font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-chart-table-wrap { max-height: 260px; overflow-y: auto; }
.qp-chart-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.qp-chart-table th { position: sticky; top: 0; background: var(--qss-bg, #18181e); text-align: right; font-weight: 500; color: var(--qss-text-muted, #6e6e7a); padding: 3px 8px; }
.qp-chart-table th:first-child,
.qp-chart-table td:first-child { text-align: left; }
.qp-chart-table td { padding: 3px 8px; border-top: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text, #d4d4d8); }
.qp-chart-table td.num { text-align: right; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
</style>
