<script setup lang="ts">
import type { LiveResult } from '#systems/types'
import SystemsLiveRanking from '#systems/components/Live/Ranking.vue'
import SystemsLiveScoreMatrix from '#systems/components/Live/ScoreMatrix.vue'
import SystemsLiveSingleAsset from '#systems/components/Live/SingleAsset.vue'
import SystemsBacktestMetricsTable from '#systems/components/Backtest/MetricsTable.vue'

const props = defineProps<{ result: LiveResult }>()
const panel = ref('standings')
const single = computed(() => props.result.mode === 'single_asset')
const metrics = computed(() => props.result.tracking?.metricsStrategy)
const holding = computed(() => {
  const point = props.result.tracking?.heldAsset.at(-1)
  return point ? point.symbol ?? 'USD' : '—'
})
const panels = computed(() => [
  { key: 'standings', label: single.value ? 'Current signal' : 'Standings' },
  { key: 'performance', label: 'Performance' },
  ...(!single.value ? [{ key: 'matrix', label: 'Score matrix' }] : []),
])
watch(single, () => { panel.value = 'standings' })

function fmt(value: number | null | undefined, suffix: string): string {
  return value == null || !Number.isFinite(value) ? '—' : `${value.toFixed(2)}${suffix}`
}
</script>

<template>
  <section class="card qs-live-overview">
    <header class="qs-live-overview__head">
      <h3>Current stats</h3>
      <span v-if="result.marketFilter?.enabled" class="pill">TOTAL {{ result.marketFilter.bullish ? 'bullish' : 'cash' }}</span>
    </header>
    <div class="qs-live-overview__stats">
      <div><span>Net Return</span><strong class="mono">{{ fmt(metrics?.netReturnMultiplier, '×') }}</strong></div>
      <div><span>Max Drawdown</span><strong class="mono">{{ fmt(metrics?.maxDrawdownPct, '%') }}</strong></div>
      <div><span>Last holding</span><strong>{{ holding }}</strong></div>
      <div><span>Next position</span><strong>{{ result.best ?? '—' }}</strong></div>
    </div>
    <nav class="qs-live-overview__tabs" aria-label="Live evaluation details">
      <button v-for="item in panels" :key="item.key" type="button"
        :aria-pressed="panel === item.key" @click="panel = item.key">{{ item.label }}</button>
    </nav>
    <div class="qs-live-overview__body">
      <SystemsLiveSingleAsset v-if="panel === 'standings' && single" :result="result" compact />
      <SystemsLiveRanking v-else-if="panel === 'standings'" :result="result" />
      <SystemsBacktestMetricsTable v-else-if="panel === 'performance' && result.tracking" :result="result.tracking" />
      <SystemsLiveScoreMatrix v-else-if="panel === 'matrix'" :result="result" />
    </div>
  </section>
</template>

<style scoped>
.qs-live-overview { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.qs-live-overview__head { display: flex; align-items: center; gap: 10px; padding: 12px 14px 8px; }
.qs-live-overview__head h3 { margin: 0; font-size: 13px; font-weight: 600; }
.qs-live-overview__head .pill { margin-left: auto; }
.qs-live-overview__stats { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; padding: 0 14px 12px; }
.qs-live-overview__stats > div { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
.qs-live-overview__stats span { font-size: 10px; color: var(--qs-text-muted); }
.qs-live-overview__stats strong { font-size: 14px; overflow-wrap: anywhere; }
.qs-live-overview__tabs { display: flex; gap: 4px; padding: 0 10px; border-bottom: 1px solid var(--qs-border); }
.qs-live-overview__tabs button { padding: 7px 9px; font: inherit; font-size: 12px; border: 0; border-bottom: 2px solid transparent; background: transparent; color: var(--qs-text-muted); cursor: pointer; }
.qs-live-overview__tabs button[aria-pressed='true'] { color: var(--qs-text); border-bottom-color: var(--qs-accent); }
.qs-live-overview__body { display: flex; flex: 1; min-height: 0; overflow: auto; }
.qs-live-overview__body > * { flex: 1; min-width: 0; }
.qs-live-overview__body :deep(.card.card) { border: 0; border-radius: 0; background: transparent; }
.qs-live-overview__body :deep(.qs-single-live) { padding: 12px 14px; }
.qs-live-overview__body :deep(.qs-single-live__metrics) { gap: 14px; margin: 0 0 12px; }
</style>
