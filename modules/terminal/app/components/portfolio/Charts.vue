<script setup lang="ts">
import type { PortfolioHolding } from '#terminal/types/portfolio'
import { assetColor, portfolioMoney as money } from '#terminal/utils/portfolio'

const props = defineProps<{
  holdings: PortfolioHolding[]
  history: { at: string; value: number | null; cost: number; gain: number | null }[]
}>()
const emit = defineEmits<{ select: [assetId: string] }>()
const hovered = ref<string | null>(null)
const positive = computed(() => props.holdings.filter((h) => h.quantity > 0 && h.value !== null && h.value > 0))
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
watch(positive, () => { if (!positive.value.some((h) => h.asset.id === hovered.value)) hovered.value = null })
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
    <section class="qp-chart" aria-label="Portfolio allocation">
      <header><h2>Allocation</h2><span>Current priced holdings</span></header>
      <div v-if="slices.length" class="qp-allocation">
        <svg viewBox="0 0 200 200" role="group" aria-label="Holdings allocation pie chart">
          <path v-for="s in slices" :key="s.asset.id" :d="s.path" :fill="assetColor(s.asset.id)" class="qp-slice" :class="{ 'is-dim': hovered && hovered !== s.asset.id }" role="button" tabindex="0" :aria-label="`Edit ${s.asset.symbol}: ${money(s.value)}, ${pct(s.share)}`" @pointerenter="hovered = s.asset.id" @pointerleave="hovered = null" @focus="hovered = s.asset.id" @blur="hovered = null" @click="emit('select', s.asset.id)" @keydown.enter.prevent="emit('select', s.asset.id)" @keydown.space.prevent="emit('select', s.asset.id)"><title>{{ s.asset.name }} · {{ money(s.value) }} · {{ pct(s.share) }}</title></path>
        </svg>
        <ul><li v-for="s in slices" :key="s.asset.id"><button type="button" @click="emit('select', s.asset.id)" @pointerenter="hovered = s.asset.id" @pointerleave="hovered = null"><i :style="{ background: assetColor(s.asset.id) }" /><span>{{ s.asset.symbol }}<small>{{ s.asset.name }}</small></span><strong>{{ pct(s.share) }}<small>{{ money(s.value) }}</small></strong></button></li></ul>
      </div>
      <p v-else class="qp-chart__empty">Add a holding and a current price to see its allocation.</p>
    </section>
    <section class="qp-chart" aria-label="Recorded holdings value">
      <header><h2>Holdings value</h2><span>Recorded daily valuations</span></header>
      <template v-if="history.length">
        <div class="qp-chart__key"><span>Value</span><span>Remaining cost</span><strong>{{ money(history.at(-1)?.value ?? null) }}</strong></div>
        <svg viewBox="0 0 520 180" class="qp-history" role="img" aria-label="Recorded holdings value and remaining cost over time">
          <path :d="series.costPath" class="qp-history__cost" /><path :d="series.valuePath" class="qp-history__value" />
          <circle v-for="p in series.points.filter(p => p.y !== null)" :key="p.at" :cx="p.x" :cy="p.y!" r="3" class="qp-history__dot"><title>{{ dateLabel(p.at) }} · Value {{ money(p.value) }} · Cost {{ money(p.cost) }}</title></circle>
        </svg>
        <div class="qp-chart__dates"><span>{{ dateLabel(history[0]!.at) }}</span><span>{{ dateLabel(history.at(-1)!.at) }}</span></div>
        <p>Starts with your first recorded valuation. Buys and sells change holdings value; gains are shown separately. Missing prices leave gaps.</p>
        <details><summary>Recorded values</summary><div class="qp-history__table"><table><thead><tr><th>Date</th><th>Value</th><th>Cost</th><th>Total gain</th></tr></thead><tbody><tr v-for="p in [...history].reverse()" :key="p.at"><td>{{ dateLabel(p.at) }}</td><td>{{ money(p.value) }}</td><td>{{ money(p.cost) }}</td><td>{{ money(p.gain) }}</td></tr></tbody></table></div></details>
      </template>
      <p v-else class="qp-chart__empty">Valuations are recorded when you save holdings or refresh market prices.</p>
    </section>
  </div>
</template>

<style scoped>
.qp-charts { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 16px; }
.qp-chart { min-width: 0; border: 1px solid var(--border); border-radius: 10px; padding: 18px; background: var(--surface-1); }
.qp-chart header, .qp-chart__dates, .qp-chart__key { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
.qp-chart h2 { font-size: 13px; margin: 0; font-weight: 600; }
.qp-chart header span, .qp-chart p, .qp-chart__dates, .qp-chart__key, .qp-chart summary { font-size: 11px; color: var(--text-secondary); }
.qp-chart p { line-height: 1.6; margin: 12px 0 0; }
.qp-allocation { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-top: 14px; }
.qp-allocation svg { flex: 1 1 140px; width: 40%; min-width: 0; max-width: 230px; }
.qp-slice { stroke: var(--surface-1); stroke-width: 1; cursor: pointer; }
.qp-slice:focus { outline: none; }
.qp-slice:focus-visible { stroke: var(--text-primary); stroke-width: 1.5; }
.qp-slice.is-dim { opacity: .4; }
.qp-allocation ul { flex: 1 1 150px; list-style: none; margin: 0; padding: 0; max-height: 220px; overflow: auto; }
.qp-allocation button { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; align-items: center; gap: 8px; width: 100%; padding: 8px 4px; border: 0; border-radius: 4px; text-align: left; color: var(--text-primary); background: transparent; cursor: pointer; font-size: 11px; }
.qp-allocation button:hover { background: var(--surface-2); }
.qp-allocation i { width: 8px; height: 8px; border-radius: 2px; }
.qp-allocation small { display: block; font-size: 10px; color: var(--text-secondary); overflow-wrap: anywhere; }
.qp-allocation strong { text-align: right; font-weight: 500; }
.qp-chart__key { margin-top: 18px; }
.qp-chart__key > span:first-child { color: var(--accent); }
.qp-chart__key strong { margin-left: auto; color: var(--text-primary); font-size: 16px; }
.qp-history { width: 100%; display: block; margin-top: 10px; overflow: visible; }
.qp-history__value { fill: none; stroke: var(--accent); stroke-width: 2; }
.qp-history__cost { fill: none; stroke: var(--muted); stroke-width: 1.5; stroke-dasharray: 4 4; }
.qp-history__dot { fill: var(--accent); }
.qp-chart__empty { padding: 35px 10px; text-align: center; }
.qp-chart details { margin-top: 10px; }
.qp-chart summary { cursor: pointer; }
.qp-history__table { overflow: auto; max-height: 160px; margin-top: 8px; }
.qp-history__table table { width: 100%; border-collapse: collapse; font-size: 10px; }
.qp-history__table th, .qp-history__table td { padding: 5px; text-align: right; white-space: nowrap; }
@container (max-width: 780px) { .qp-charts { grid-template-columns: minmax(0, 1fr); } }
</style>
