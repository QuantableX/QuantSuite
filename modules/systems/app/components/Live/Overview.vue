<script setup lang="ts">
import type { LiveResult } from '#systems/types'
import SystemsBacktestMetricsTable from '#systems/components/Backtest/MetricsTable.vue'
import SystemsLiveRanking from '#systems/components/Live/Ranking.vue'
import SystemsLiveSingleAsset from '#systems/components/Live/SingleAsset.vue'

const props = defineProps<{ result: LiveResult }>()
const single = computed(() => props.result.mode === 'single_asset')
const holding = computed(() => {
  const point = props.result.tracking?.heldAsset.at(-1)
  return point ? point.symbol ?? 'USD' : '—'
})
</script>

<template>
  <section class="card qs-live-overview" aria-label="Current stats">
    <div class="qs-live-overview__body">
      <SystemsBacktestMetricsTable v-if="result.tracking" :result="result.tracking" strategy-only />
      <div v-else class="qs-live-overview__empty">No performance data in this window.</div>
      <div class="qs-live-overview__standings">
        <SystemsLiveSingleAsset v-if="single" :result="result" compact />
        <SystemsLiveRanking v-else :result="result" compact />
        <div class="qs-live-overview__positions" aria-label="Positions">
          <span>Last holding <strong>{{ holding }}</strong></span>
          <span>Next position <strong>{{ result.best ?? '—' }}</strong></span>
          <span v-if="result.marketFilter?.enabled" class="pill">TOTAL {{ result.marketFilter.bullish ? 'bullish' : 'cash' }}</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.qs-live-overview { container: live-overview / inline-size; display: flex; min-height: 0; overflow: auto; }
.qs-live-overview__body { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); flex: 1; min-width: 0; min-height: 0; }
.qs-live-overview__body > * { min-width: 0; min-height: 0; }
.qs-live-overview__body :deep(.card.card) { border: 0; border-radius: 0; background: transparent; }
.qs-live-overview__body > :deep(.qs-metrics) { overflow: auto; }
.qs-live-overview__standings { display: flex; flex-direction: column; border-left: 1px solid var(--qs-border); }
.qs-live-overview__standings > :deep(.card) { flex: 1; min-height: 0; }
.qs-live-overview__positions { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; padding: 10px 12px; border-top: 1px solid var(--qs-border); font-size: 11px; color: var(--qs-text-muted); }
.qs-live-overview__positions strong { margin-left: 6px; color: var(--qs-text); }
.qs-live-overview__positions .pill { font-size: 10px; }
.qs-live-overview__body :deep(.qs-single-live) { padding: 12px 14px; }
.qs-live-overview__body :deep(.qs-single-live__metrics) { gap: 14px; margin: 0 0 12px; }
.qs-live-overview__empty { padding: 12px 14px; font-size: 13px; color: var(--qs-text-muted); }
@container live-overview (max-width: 480px) {
  .qs-live-overview__body { grid-template-columns: 1fr; grid-template-rows: 344px minmax(220px, 1fr); min-height: 564px; }
  .qs-live-overview__standings { border-left: 0; border-top: 1px solid var(--qs-border); }
}
</style>
