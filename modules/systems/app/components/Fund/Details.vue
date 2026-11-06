<script setup lang="ts">
/** A fund's result beside its metrics: today's split, the rebalances, how alike the members are, and the robustness checks. */
import type { BacktestResult, FundDetails } from '#systems/types'
import { money, percent } from '#systems/utils/funds'

const props = defineProps<{ result: BacktestResult & { fund: FundDetails } }>()
type Tab = 'split' | 'rebalances' | 'correlation' | 'robustness'
const tab = ref<Tab>('split')
const tabs: { id: Tab; label: string }[] = [
  { id: 'split', label: 'Split' },
  { id: 'rebalances', label: 'Rebalances' },
  { id: 'correlation', label: 'Correlation' },
  { id: 'robustness', label: 'Robustness' },
]
const fund = computed(() => props.result.fund)
const memberName = (id: string) => fund.value.members.find(m => m.id === id)?.name ?? id
const robust = computed(() => fund.value.robustness)
const mc = computed(() => robust.value.monteCarlo)
const mixed = computed(() => [...robust.value.lookbacks, ...robust.value.windows].some(row => row.beatsEqual === false))
const baseline = computed(() => fund.value.method === 'equal')

function beats(value: boolean | null) {
  return value === null ? '—' : value ? 'Yes' : 'No'
}
function heat(value: number | null) {
  return value == null ? undefined : { background: `color-mix(in srgb, var(--qs-accent) ${Math.round(Math.max(value, 0) * 45)}%, transparent)` }
}
</script>

<template>
  <div class="card qs-fund-details">
    <div class="qs-fund-details__tabs" role="tablist" aria-label="Fund details">
      <button v-for="t in tabs" :key="t.id" type="button" role="tab" :aria-selected="tab === t.id" :class="{ 'is-active': tab === t.id }" @click="tab = t.id">{{ t.label }}</button>
    </div>

    <div class="qs-fund-details__body" role="tabpanel">
      <template v-if="tab === 'split'">
        <table class="qs-fund-details__table">
          <thead><tr><th>System</th><th class="num">Held</th><th class="num">Split now</th><th class="num">Share</th></tr></thead>
          <tbody>
            <tr v-for="m in fund.members" :key="m.id">
              <td class="qs-fund-details__name" :title="m.name">{{ m.name }}</td>
              <td class="num mono">{{ money(m.heldDollars) }}</td>
              <td class="num mono">{{ money(m.targetDollars, 0) }}</td>
              <td class="num mono">{{ percent(m.targetShare) }}</td>
            </tr>
          </tbody>
        </table>
        <div class="qs-fund-details__facts">
          <span>Value <b class="mono">{{ money(fund.value) }}</b></span>
          <span>Cash <b class="mono">{{ money(fund.cash) }}</b></span>
          <span>Moving costs <b class="mono">{{ money(fund.cost) }}</b></span>
          <span>Next rebalance <b class="mono">{{ fund.nextRebalance ?? 'Never' }}</b></span>
          <span>Window <b class="mono">{{ fund.windowStart }} to {{ fund.windowEnd }}</b></span>
        </div>
      </template>

      <template v-else-if="tab === 'rebalances'">
        <p v-if="!fund.rebalances.length" class="qs-fund-details__note">No rebalance in this window.</p>
        <table v-else class="qs-fund-details__table">
          <thead><tr><th>Date</th><th v-for="m in fund.members" :key="m.id" class="num" :title="m.name">{{ m.name }}</th><th class="num">Cash</th><th class="num">Cost</th></tr></thead>
          <tbody>
            <tr v-for="r in fund.rebalances" :key="r.date">
              <td class="mono">{{ r.date }}</td>
              <td v-for="m in fund.members" :key="m.id" class="num mono">{{ money(r.values[m.id] ?? 0, 0) }}</td>
              <td class="num mono">{{ money(r.cash, 0) }}</td>
              <td class="num mono">{{ money(r.cost) }}</td>
            </tr>
          </tbody>
        </table>
      </template>

      <template v-else-if="tab === 'correlation'">
        <table class="qs-fund-details__table qs-fund-details__corr">
          <thead><tr><th /><th v-for="id in fund.correlation.ids" :key="id" class="num" :title="memberName(id)">{{ memberName(id) }}</th></tr></thead>
          <tbody>
            <tr v-for="(row, i) in fund.correlation.matrix" :key="fund.correlation.ids[i]">
              <th class="qs-fund-details__name" :title="fund.correlation.names[i]">{{ fund.correlation.names[i] }}</th>
              <td v-for="(value, j) in row" :key="j" class="num mono" :style="heat(value)">{{ value == null ? '—' : value.toFixed(2) }}</td>
            </tr>
          </tbody>
        </table>
        <p class="qs-fund-details__note">Daily returns. Systems near 1.00 move together, so together they are close to one bet.</p>
      </template>

      <template v-else>
        <p class="qs-fund-details__note" :class="{ 'qs-fund-details__warn': mixed }">
          {{ baseline ? 'Equal is the baseline the other methods are compared with.' : mixed ? 'This method does not beat Equal everywhere: treat it as unproven.' : 'This method beat Equal at every lookback and in every window with data.' }}
        </p>
        <table class="qs-fund-details__table">
          <thead><tr><th>Check</th><th class="num">CAGR</th><th class="num">Equal</th><th class="num">Max DD</th><th>Beats Equal</th></tr></thead>
          <tbody>
            <tr v-for="row in robust.lookbacks" :key="row.lookbackDays">
              <td>Lookback {{ row.lookbackDays }} days</td>
              <td class="num mono">{{ percent(row.cagr) }}</td><td class="num mono">{{ percent(row.equalCagr) }}</td><td class="num mono">{{ percent(row.maxDrawdown) }}</td><td>{{ beats(row.beatsEqual) }}</td>
            </tr>
            <tr v-for="row in robust.windows" :key="row.window">
              <td>Window {{ row.window }}</td>
              <td class="num mono">{{ percent(row.cagr) }}</td><td class="num mono">{{ percent(row.equalCagr) }}</td><td class="num mono">{{ percent(row.maxDrawdown) }}</td><td>{{ beats(row.beatsEqual) }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="mc && mc.cagr_p50 != null" class="qs-fund-details__note">
          Monte Carlo (300 reshuffled paths): CAGR {{ percent(mc.cagr_p5) }} to {{ percent(mc.cagr_p50) }} (5th percentile to median), typical max drawdown {{ percent(mc.dd_p50) }}, worst 5% {{ percent(mc.dd_p95) }}, chance of a 3-year loss {{ percent(mc.loss_3y) }}.
        </p>
        <p v-for="note in result.notes" :key="note" class="qs-fund-details__note">{{ note }}</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.qs-fund-details { display: flex; flex-direction: column; min-width: 0; min-height: 0; overflow: hidden; }
.qs-fund-details__tabs { flex-shrink: 0; display: flex; gap: 2px; padding: 6px 6px 0; border-bottom: 1px solid var(--qs-border); overflow-x: auto; }
.qs-fund-details__tabs button { padding: 6px 10px; border: 0; border-bottom: 2px solid transparent; background: transparent; color: var(--qs-text-secondary); cursor: pointer; font-size: 12px; white-space: nowrap; }
.qs-fund-details__tabs button:hover { color: var(--qs-text); }
.qs-fund-details__tabs button.is-active { border-bottom-color: var(--qs-accent); color: var(--qs-text); }
.qs-fund-details__tabs button:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: -2px; }
.qs-fund-details__body { flex: 1; min-height: 0; overflow: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; }
.qs-fund-details__table { width: 100%; border-collapse: collapse; font-size: 12px; }
.qs-fund-details__table th { position: sticky; top: 0; z-index: 1; padding: 4px 6px; background: var(--qs-bg-card, var(--qs-bg)); color: var(--qs-text-muted); font-weight: 500; text-align: left; white-space: nowrap; }
.qs-fund-details__table td { padding: 4px 6px; border-top: 1px solid var(--qs-border); white-space: nowrap; }
.qs-fund-details__table .num { text-align: right; font-variant-numeric: tabular-nums; }
.qs-fund-details__name { max-width: 180px; overflow: hidden; text-overflow: ellipsis; }
.qs-fund-details__facts { display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: 12px; color: var(--qs-text-secondary); }
.qs-fund-details__facts b { font-weight: 500; color: var(--qs-text); }
.qs-fund-details__note { margin: 0; font-size: 12px; line-height: 1.5; color: var(--qs-text-muted); }
.qs-fund-details__warn { color: var(--qs-error); }
</style>
