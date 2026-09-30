<script setup lang="ts">
import type { LiveResult, PerformanceMetrics } from '#systems/types'
import SystemsLiveRanking from '#systems/components/Live/Ranking.vue'
import SystemsLiveScoreMatrix from '#systems/components/Live/ScoreMatrix.vue'
import SystemsLiveSingleAsset from '#systems/components/Live/SingleAsset.vue'

const props = defineProps<{ result: LiveResult }>()
const single = computed(() => props.result.mode === 'single_asset')
const metrics = computed(() => props.result.tracking?.metricsStrategy)
const holding = computed(() => {
  const point = props.result.tracking?.heldAsset.at(-1)
  return point ? point.symbol ?? 'USD' : '—'
})
const metricItems: { key: keyof PerformanceMetrics; label: string; suffix: string }[] = [
  { key: 'netReturnMultiplier', label: 'Net Return', suffix: '×' },
  { key: 'maxDrawdownPct', label: 'Max Drawdown', suffix: '%' },
  { key: 'sharpe', label: 'Sharpe', suffix: '' },
  { key: 'sortino', label: 'Sortino', suffix: '' },
  { key: 'omega', label: 'Omega', suffix: '' },
  { key: 'cagrPct', label: 'CAGR', suffix: '%' },
  { key: 'meanAllPct', label: 'Mean Return', suffix: '%' },
  { key: 'meanPosPct', label: 'Positive Return', suffix: '%' },
  { key: 'meanNegPct', label: 'Negative Return', suffix: '%' },
  { key: 'stddevAllPct', label: 'Volatility', suffix: '%' },
  { key: 'stddevPosPct', label: 'Positive Volatility', suffix: '%' },
  { key: 'stddevNegPct', label: 'Negative Volatility', suffix: '%' },
]

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
    <div class="qs-live-overview__stats" tabindex="0" role="region" aria-label="All performance metrics">
      <div v-for="item in metricItems" :key="item.key">
        <span>{{ item.label }}</span><strong class="mono">{{ fmt(metrics?.[item.key], item.suffix) }}</strong>
      </div>
    </div>
    <div class="qs-live-overview__positions" aria-label="Positions">
      <span>Last holding <strong>{{ holding }}</strong></span>
      <span>Next position <strong>{{ result.best ?? '—' }}</strong></span>
    </div>
    <div class="qs-live-overview__body" :class="{ 'qs-live-overview__body--split': !single }">
      <SystemsLiveSingleAsset v-if="single" :result="result" compact />
      <template v-else>
        <SystemsLiveRanking :result="result" compact />
        <SystemsLiveScoreMatrix :result="result" compact />
      </template>
    </div>
  </section>
</template>

<style scoped>
.qs-live-overview { container: live-overview / inline-size; display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.qs-live-overview__head { display: flex; align-items: center; gap: 10px; padding: 8px 14px 6px; }
.qs-live-overview__head h3 { margin: 0; font-size: 13px; font-weight: 600; }
.qs-live-overview__head .pill { margin-left: auto; }
.qs-live-overview__stats { flex-shrink: 0; display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 8px; padding: 0 14px 10px; }
.qs-live-overview__stats > div { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.qs-live-overview__stats span { font-size: 10px; line-height: 1.2; color: var(--qs-text-muted); }
.qs-live-overview__stats strong { font-size: 14px; line-height: 1.25; overflow-wrap: anywhere; }
.qs-live-overview__positions { flex-shrink: 0; display: flex; flex-wrap: wrap; gap: 8px 20px; padding: 6px 14px; border-block: 1px solid var(--qs-border); font-size: 11px; color: var(--qs-text-muted); }
.qs-live-overview__positions strong { margin-left: 6px; color: var(--qs-text); }
.qs-live-overview__body { display: flex; flex: 1; min-height: 0; overflow: auto; }
.qs-live-overview__body--split { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.15fr); overflow: hidden; }
.qs-live-overview__body > * { flex: 1; min-width: 0; }
.qs-live-overview__body :deep(.card.card) { border: 0; border-radius: 0; background: transparent; }
.qs-live-overview__body--split :deep(.qs-mx.card) { border-left: 1px solid var(--qs-border); }
.qs-live-overview__body :deep(.qs-single-live) { padding: 12px 14px; }
.qs-live-overview__body :deep(.qs-single-live__metrics) { gap: 14px; margin: 0 0 12px; }
@container live-overview (max-width: 580px) {
  .qs-live-overview__stats { display: flex; overflow-x: auto; gap: 16px; }
  .qs-live-overview__stats > div { flex: 0 0 auto; min-width: 78px; }
}
</style>
