<script setup lang="ts">
import type { BacktestResult } from '#systems/types'
import SystemsBacktestEquityChart from '#systems/components/Backtest/EquityChart.vue'
import SystemsBacktestMetricsTable from '#systems/components/Backtest/MetricsTable.vue'
import SystemsBacktestForcedRotations from '#systems/components/Backtest/ForcedRotations.vue'

const props = defineProps<{ result: BacktestResult; emptyMessage?: string }>()
const hasCurve = computed(() => props.result.strategies?.some(run => run.equityStrategy.length) ?? !!props.result.equityStrategy.length)
</script>

<template>
  <div class="qs-results">
    <SystemsBacktestEquityChart v-if="hasCurve" :result="result" class="qs-results__chart" />
    <div v-else class="card qs-results__chart qs-results__empty">{{ emptyMessage ?? 'No equity data in this window.' }}</div>
    <div class="qs-results__grid">
      <slot name="overview"><SystemsBacktestMetricsTable :result="result" /></slot>
      <SystemsBacktestForcedRotations :result="result" />
    </div>
  </div>
</template>

<style scoped>
/* Both evaluation views share the backtest's 344px details row. The chart
   receives the remaining space; small windows scroll inside the cards. */
.qs-results { container: result-panels / inline-size; display: flex; flex-direction: column; flex: 1; min-height: 0; gap: 12px; }
.qs-results__chart { flex: 1 1 0; min-height: 230px; }
.qs-results__grid { flex: 0 1 344px; min-height: 180px; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.qs-results__grid > * { min-width: 0; min-height: 0; overflow: auto; }
.qs-results__empty { display: flex; align-items: center; justify-content: center; padding: 20px; font-size: 13px; color: var(--qs-text-muted); }
@container result-panels (max-width: 620px) {
  .qs-results__grid { grid-template-columns: 1fr; grid-template-rows: repeat(2, minmax(0, 1fr)); flex: 0 0 660px; }
}
</style>
