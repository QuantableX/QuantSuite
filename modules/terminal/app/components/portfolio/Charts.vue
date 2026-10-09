<script setup lang="ts">
import type { PortfolioHolding } from '#terminal/types/portfolio'
import { assetColor, portfolioCashColor, portfolioMoney as money } from '#terminal/utils/portfolio'

const props = defineProps<{
  holdings: PortfolioHolding[]
  cash: number
  history: { at: string; value: number | null; cost: number; gain: number | null }[]
}>()
const emit = defineEmits<{ select: [assetId: string]; cash: [] }>()
const hovered = ref<string | null>(null)
const mode = ref<'allocation' | 'history'>('allocation')
const positive = computed(() => [
  { key: 'cash', assetId: null, symbol: 'Cash', name: 'USD', value: props.cash, color: portfolioCashColor },
  ...props.holdings.filter((h) => h.quantity > 0).map((h) => ({ key: `asset:${h.asset.id}`, assetId: h.asset.id, symbol: h.asset.symbol, name: h.asset.name, value: h.value, color: assetColor(h.asset.id) })),
].filter((h) => h.value !== null && h.value > 0))
const total = computed(() => positive.value.reduce((sum, h) => sum + h.value!, 0))
const slices = computed(() => {
  let start = 0
  const point = (value: number) => `${100 + 90 * Math.sin(value * Math.PI * 2)} ${100 - 90 * Math.cos(value * Math.PI * 2)}`
  return positive.value.map((h) => {
    const share = h.value! / total.value
    const end = start + share
    const path = positive.value.length === 1 ? 'M 100 10 A 90 90 0 0 1 100 190 A 90 90 0 0 1 100 10 Z'
      : `M 100 100 L ${point(start)} A 90 90 0 0 1 ${point((start + end) / 2)} A 90 90 0 0 1 ${point(end)} Z`
    start = end
    return { ...h, share, path }
  })
})
watch(positive, () => { if (!positive.value.some((h) => h.key === hovered.value)) hovered.value = null })
function select(assetId: string | null) { if (assetId === null) emit('cash'); else emit('select', assetId) }
const pct = (share: number) => new Intl.NumberFormat('en-US', { style: 'percent', maximumFractionDigits: 1 }).format(share)
const dateLabel = (at: string) => new Date(at).toLocaleDateString()
const series = computed(() => {
  const points = props.history
  const values = points.flatMap((p) => p.value === null ? [p.cost] : [p.value, p.cost])
  const min = Math.min(0, ...values), max = Math.max(1, ...values)
  const start = Date.parse(points[0]?.at ?? '')
  const span = Math.max(1, Date.parse(points.at(-1)?.at ?? '') - start)
  const x = (at: string) => points.length === 1 ? 260 : 20 + (Date.parse(at) - start) / span * 480
  const y = (v: number) => 155 - (v - min) / (max - min) * 135
  let connected = false
  const valuePath = points.map((p) => {
    if (p.value === null) { connected = false; return '' }
    const cmd = connected ? 'L' : 'M'; connected = true
    return `${cmd} ${x(p.at)} ${y(p.value)}`
  }).join(' ')
  return { valuePath, costPath: points.map((p, i) => `${i ? 'L' : 'M'} ${x(p.at)} ${y(p.cost)}`).join(' '), points: points.map((p) => ({ ...p, x: x(p.at), y: p.value === null ? null : y(p.value) })), max }
})
</script>

<template>
  <div class="qp-charts">
    <nav class="qp-chart-tabs" aria-label="Portfolio chart"><button :aria-pressed="mode === 'allocation'" @click="mode = 'allocation'">Allocation</button><button :aria-pressed="mode === 'history'" @click="mode = 'history'">Value history</button></nav>
    <section v-if="mode === 'allocation'" class="qp-chart qp-chart--allocation" aria-label="Portfolio allocation">
      <header><h2>Allocation</h2><span>Cash &amp; investments</span></header>
      <div v-if="slices.length" class="qp-allocation">
        <svg viewBox="0 0 200 200" role="group" aria-label="Portfolio allocation pie chart">
          <path v-for="s in slices" :key="s.key" :d="s.path" :fill="s.color" class="qp-slice" :class="{ 'is-dim': hovered && hovered !== s.key }" role="button" tabindex="0" :aria-label="`Edit ${s.symbol}: ${money(s.value)}, ${pct(s.share)}`" @pointerenter="hovered = s.key" @pointerleave="hovered = null" @focus="hovered = s.key" @blur="hovered = null" @click="select(s.assetId)" @keydown.enter.prevent="select(s.assetId)" @keydown.space.prevent="select(s.assetId)"><title>{{ s.name }} · {{ money(s.value) }} · {{ pct(s.share) }}</title></path>
        </svg>
        <ul><li v-for="s in slices" :key="s.key"><button type="button" @click="select(s.assetId)" @pointerenter="hovered = s.key" @pointerleave="hovered = null"><i :style="{ background: s.color }" /><span>{{ s.symbol }}<small>{{ s.name }}</small></span><strong>{{ pct(s.share) }}<small>{{ money(s.value) }}</small></strong></button></li></ul>
      </div>
      <p v-else class="qp-chart__empty">Add cash or a priced holding to see your allocation.</p>
    </section>
    <section v-else class="qp-chart" aria-label="Recorded portfolio value">
      <header><h2>Portfolio value</h2><span>Recorded daily valuations</span></header>
      <template v-if="history.length">
        <div class="qp-chart__key"><span>Value</span><span>Remaining cost</span><strong>{{ money(history.at(-1)?.value ?? null) }}</strong></div>
        <svg viewBox="0 0 520 180" class="qp-history" role="img" aria-label="Recorded portfolio value and remaining investment cost over time">
          <path :d="series.costPath" class="qp-history__cost" /><path :d="series.valuePath" class="qp-history__value" />
          <circle v-for="p in series.points.filter(p => p.y !== null)" :key="p.at" :cx="p.x" :cy="p.y!" r="3" class="qp-history__dot"><title>{{ dateLabel(p.at) }} · Value {{ money(p.value) }} · Cost {{ money(p.cost) }}</title></circle>
        </svg>
        <div class="qp-chart__dates"><span>{{ dateLabel(history[0]!.at) }}</span><span>{{ dateLabel(history.at(-1)!.at) }}</span></div>
        <p>Includes cash and investments. Deposits and withdrawals change portfolio value; gains are shown separately. Missing prices leave gaps.</p>
        <details><summary>Recorded values</summary><div class="qp-history__table"><table><thead><tr><th>Date</th><th>Value</th><th>Cost</th><th>Total gain</th></tr></thead><tbody><tr v-for="p in [...history].reverse()" :key="p.at"><td>{{ dateLabel(p.at) }}</td><td>{{ money(p.value) }}</td><td>{{ money(p.cost) }}</td><td>{{ money(p.gain) }}</td></tr></tbody></table></div></details>
      </template>
      <p v-else class="qp-chart__empty">Valuations are recorded when you save portfolio changes or refresh market prices.</p>
    </section>
  </div>
</template>

<style scoped>
.qp-charts { display: flex; flex-direction: column; min-height: 0; min-width: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-1); container-type: size; container-name: portfolio-chart; }
.qp-chart-tabs { display: flex; flex-shrink: 0; gap: 4px; padding: 10px 12px; border-bottom: 1px solid var(--border); }
.qp-chart-tabs button { padding: 6px 9px; font-size: 11px; border: 0; border-radius: 5px; background: transparent; color: var(--text-secondary); cursor: pointer; }
.qp-chart-tabs button[aria-pressed='true'] { background: var(--surface-2); color: var(--text-primary); }
.qp-chart { flex: 1; min-height: 0; min-width: 0; padding: 14px; overflow: auto; overscroll-behavior: contain; }
.qp-chart--allocation { display: flex; flex-direction: column; overflow: hidden; }
.qp-chart header { flex-shrink: 0; }
.qp-chart header, .qp-chart__dates, .qp-chart__key { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.qp-chart h2 { font-size: 13px; margin: 0; font-weight: 600; }
.qp-chart header span, .qp-chart p, .qp-chart__dates, .qp-chart__key, .qp-chart summary { font-size: 11px; color: var(--text-secondary); }
.qp-chart p { line-height: 1.6; margin: 12px 0 0; }
.qp-allocation { flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; gap: 14px; margin-top: 14px; }
.qp-allocation svg { flex: 0 1 auto; height: 45%; width: 100%; min-height: 0; max-width: 280px; max-height: 280px; }
.qp-slice { stroke: var(--surface-1); stroke-width: 1; cursor: pointer; }
.qp-slice:focus { outline: none; }
.qp-slice:focus-visible { stroke: var(--text-primary); stroke-width: 1.5; }
.qp-slice.is-dim { opacity: .4; }
.qp-allocation ul { flex: 1; min-height: 0; width: 100%; list-style: none; margin: 0; padding: 0; overflow: auto; overscroll-behavior: contain; }
.qp-allocation button { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; align-items: center; gap: 8px; width: 100%; padding: 8px 4px; border: 0; border-radius: 4px; text-align: left; color: var(--text-primary); background: transparent; cursor: pointer; font-size: 11px; }
.qp-allocation button:hover { background: var(--surface-2); }
.qp-allocation i { width: 8px; height: 8px; border-radius: 2px; }
.qp-allocation small { display: block; font-size: 10px; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-allocation strong { text-align: right; font-weight: 500; }
.qp-chart__key { margin-top: 18px; }
.qp-chart__key > span:first-child { color: var(--accent); }
.qp-chart__key strong { margin-left: auto; color: var(--text-primary); font-size: 16px; }
.qp-history { width: 100%; max-height: 200px; display: block; margin-top: 10px; overflow: visible; }
.qp-history__value { fill: none; stroke: var(--accent); stroke-width: 2; }
.qp-history__cost { fill: none; stroke: var(--muted); stroke-width: 1.5; stroke-dasharray: 4 4; }
.qp-history__dot { fill: var(--accent); }
.qp-chart__empty { padding: 35px 10px; text-align: center; }
.qp-chart details { margin-top: 10px; }
.qp-chart summary { cursor: pointer; }
.qp-history__table { overflow: auto; max-height: 160px; margin-top: 8px; }
.qp-history__table table { width: 100%; border-collapse: collapse; font-size: 10px; }
.qp-history__table th, .qp-history__table td { padding: 5px; text-align: right; white-space: nowrap; }
@container portfolio-chart (max-height: 360px) { .qp-chart { padding: 10px; } .qp-chart--allocation header { display: none; } .qp-allocation { flex-direction: row; gap: 10px; margin-top: 0; } .qp-allocation svg { height: 100%; width: 42%; max-width: 180px; flex-shrink: 0; } .qp-chart-tabs { padding: 6px 8px; } }
</style>
