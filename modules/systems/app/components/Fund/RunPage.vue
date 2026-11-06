<script setup lang="ts">
/** A fund's Backtest and Live pages: the same layout as a system's, with the fund's details beside its metrics. */
import { useFundsStore, type FundMode } from '#systems/stores/funds'
import { FUND_METHODS, usesPerformance } from '#systems/utils/funds'
import { useActiveView } from '#systems/composables/useActiveView'
import SystemsBacktestEquityChart from '#systems/components/Backtest/EquityChart.vue'
import SystemsBacktestMetricsTable from '#systems/components/Backtest/MetricsTable.vue'
import SystemsFundDetails from '#systems/components/Fund/Details.vue'
import type { Fund, FundMethod } from '#systems/types'

const props = defineProps<{ mode: FundMode }>()
const funds = useFundsStore()
const { fundId } = useActiveView()
const fund = computed(() => funds.byId(fundId.value))
const state = computed(() => funds.stateFor(fundId.value, props.mode))
const result = computed(() => state.value.result)
const title = computed(() => props.mode === 'live' ? 'Live' : 'Backtest')
const hasCurve = computed(() => result.value?.strategies?.some(run => run.equityStrategy.length) ?? false)
const saveError = ref<string | null>(null)

function run() {
  if (fund.value) void funds.run(fund.value.id, props.mode)
}
/** Changing the rule here persists at once, like a system's Backtest pickers. The members do not run again. */
async function change(patch: Partial<Fund>) {
  if (!fund.value) return
  saveError.value = null
  try {
    await funds.save({ ...fund.value, ...patch })
    if (result.value) run()
  } catch (e) {
    saveError.value = String(e)
  }
}
onMounted(() => { void funds.load() })
</script>

<template>
  <div v-if="fund" class="qs-fund-run">
    <header class="qs-fund-run__head">
      <h1 class="qs-fund-run__title">{{ title }}</h1>
      <div class="qs-fund-run__controls">
        <label class="qs-fund-run__control"><span class="label">Method</span>
          <select class="select" :value="fund.method" :disabled="state.isRunning" @change="change({ method: ($event.target as HTMLSelectElement).value as FundMethod })">
            <option v-for="m in FUND_METHODS" :key="m.id" :value="m.id" :disabled="m.id === 'fixed' && !Object.keys(fund.fixedPct).length">{{ m.label }}</option>
          </select>
        </label>
        <label v-if="usesPerformance(fund.method)" class="qs-fund-run__control"><span class="label">Lookback</span>
          <select class="select" :value="fund.lookbackDays" :disabled="state.isRunning" @change="change({ lookbackDays: Number(($event.target as HTMLSelectElement).value) })">
            <option v-for="d in [30, 90, 180, 365]" :key="d" :value="d">{{ d }} days</option>
          </select>
        </label>
        <button class="btn btn-primary" :disabled="state.isRunning" @click="run">
          {{ state.isRunning ? (funds.runningKey === `${fund.id}:${mode}` ? 'Running…' : 'Queued…') : mode === 'live' ? 'Update' : 'Run Backtest' }}
        </button>
      </div>
    </header>

    <div v-if="state.isRunning" class="qs-fund-run__status">
      <div class="qs-bar" role="progressbar" aria-label="Fund progress" :aria-valuenow="Math.round(state.progressValue * 100)" :aria-valuemin="0" :aria-valuemax="100">
        <div class="qs-bar__fill" :style="{ width: `${Math.round(state.progressValue * 100)}%` }" />
      </div>
      <span class="qs-fund-run__progress" role="status" :title="state.progressLabel">{{ state.progressLabel || 'Working…' }}</span>
    </div>
    <p v-if="state.error" class="qs-fund-run__error" role="alert">{{ state.error }}</p>
    <p v-if="saveError" class="qs-fund-run__error" role="alert">{{ saveError }}</p>

    <div v-if="result" class="qs-fund-run__results">
      <SystemsBacktestEquityChart v-if="hasCurve" :result="result" class="qs-fund-run__chart" />
      <div v-else class="card qs-fund-run__chart qs-fund-run__empty">No equity data in this window.</div>
      <div class="qs-fund-run__grid">
        <SystemsBacktestMetricsTable :result="result" />
        <SystemsFundDetails :result="result" />
      </div>
    </div>
    <div v-else-if="!state.isRunning" class="qs-fund-run__empty card">
      {{ mode === 'live' ? 'Update to see today\'s split and how the fund has done since its live start.' : 'Run a backtest to compare every split method with Equal.' }}
    </div>
  </div>
  <div v-else-if="funds.loaded" class="qs-fund-run__empty card">This fund no longer exists.</div>
</template>

<style scoped>
.qs-fund-run { container: fund-run / inline-size; display: flex; flex-direction: column; gap: var(--qss-gap); height: 100%; min-height: 0; }
.qs-fund-run__head { flex-shrink: 0; display: flex; align-items: flex-end; justify-content: space-between; flex-wrap: wrap; gap: var(--qss-gap); min-width: 0; }
.qs-fund-run__title { margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.qs-fund-run__controls { display: flex; align-items: flex-end; flex-wrap: wrap; gap: var(--qss-gap-tight); min-width: 0; }
.qs-fund-run__control { display: flex; flex-direction: column; min-width: 130px; }
.qs-fund-run__status { flex: 0 0 26px; display: flex; align-items: center; gap: 10px; min-width: 0; }
.qs-fund-run__status .qs-bar { flex: 1; }
.qs-fund-run__progress { flex: 0 1 45%; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--qss-text-label); color: var(--qs-text-muted); }
.qs-fund-run__error { flex-shrink: 0; margin: 0; color: var(--qs-error); font-size: var(--qss-text-label); overflow-wrap: anywhere; }
.qs-fund-run__results { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: var(--qss-gap); }
.qs-fund-run__chart { flex: 1 1 0; min-height: clamp(180px, calc(100% - 272px), 230px); }
.qs-fund-run__grid { flex: 0 1 312px; min-height: 260px; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--qss-gap); }
.qs-fund-run__grid > * { min-width: 0; min-height: 0; overflow: auto; }
.qs-fund-run__empty { flex: 1; display: flex; align-items: center; justify-content: center; padding: var(--qss-panel-pad); font-size: var(--qss-text-label); color: var(--qs-text-muted); text-align: center; }
@container fund-run (max-width: 620px) {
  .qs-fund-run__grid { grid-template-columns: 1fr; grid-template-rows: repeat(2, minmax(260px, 1fr)); flex: 0 0 540px; }
  .qs-fund-run__results { overflow-y: auto; }
}
</style>
