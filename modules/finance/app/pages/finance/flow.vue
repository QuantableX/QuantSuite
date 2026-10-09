<script setup lang="ts">
/** The plan as a flow — where the money goes, at a glance. */
definePageMeta({ layout: 'finance' })

import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import { formatCents } from '#finance/utils/money'
import { flowPieSlices } from '#finance/utils/pie'

const app = useAppStore()
const plan = usePlanStore()
const router = useRouter()
const slices = computed(() => flowPieSlices(plan.sankey))
function openFund(id: string) {
  void router.push({ path: '/finance/funds', query: { fund: id } })
}

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
</script>

<template>
  <div class="qf-flow">
    <div class="qf-flow__stats">
      <QMetric label="Income" :value="money(plan.summary.incomeCents)" tone="up" />
      <QMetric label="Saving" :value="money(plan.summary.savingCents)" />
      <QMetric label="Fixed" :value="money(plan.summary.expenseCents)" tone="down" />
      <QMetric
        :label="plan.overspent ? 'Short by' : 'Leftover'"
        :value="money(Math.abs(plan.summary.leftoverCents))"
        :tone="plan.overspent ? 'down' : undefined"
      />
      <QMetric
        v-if="plan.summary.savingsRate !== null"
        label="Savings rate"
        :value="`${plan.summary.savingsRate}%`"
      />
    </div>

    <div class="qf-flow__toolbar">
      <span>Monthly budget</span>
      <div class="qf-flow__switch" role="group" aria-label="Chart type">
        <button type="button" :aria-pressed="app.flowChartMode !== 'pie'" @click="app.flowChartMode = 'sankey'">Sankey</button>
        <button type="button" :aria-pressed="app.flowChartMode === 'pie'" @click="app.flowChartMode = 'pie'">Pie chart</button>
      </div>
    </div>
    <div class="qf-flow__chart" :class="{ 'qf-flow__chart--pie': app.flowChartMode === 'pie' }">
      <template v-if="app.flowChartMode === 'pie'">
        <p v-if="plan.overspent" class="qf-flow__deficit" role="status">
          Short by {{ money(Math.abs(plan.summary.leftoverCents)) }}. Shares are based on total planned saving and fixed costs, which exceed income.
        </p>
        <FinancePieChart
          :slices="slices"
          title="Budget allocation"
          :description="plan.overspent ? 'Share of planned saving and fixed costs.' : 'How your monthly income is split between saving, fixed costs and leftover.'"
          :total-label="plan.overspent ? 'Total planned' : 'Total allocated'"
          empty-text="Nothing planned yet. Fill in the Plan to see your allocation."
          @select="openFund"
        />
      </template>
      <FinanceSankeyChart v-else />
    </div>
  </div>
</template>

<style scoped>
.qf-flow {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  padding: 14px 18px 18px;
}

.qf-flow__stats {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--qf-border-subtle);
}

.qf-flow__chart {
  flex: 1;
  min-height: 0;
  padding-top: 8px;
}
.qf-flow__toolbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding: 12px 0; }
.qf-flow__toolbar > span { font-size: 11px; color: var(--qf-text-muted); }
.qf-flow__switch { display: flex; gap: 3px; padding: 3px; border: 1px solid var(--qf-border-subtle); border-radius: var(--qf-radius); background: var(--qf-bg-raised); }
.qf-flow__switch button { padding: 5px 12px; border: 0; border-radius: 4px; background: transparent; color: var(--qf-text-secondary); font-size: 12px; cursor: pointer; }
.qf-flow__switch button[aria-pressed='true'] { background: var(--qf-bg-hover); color: var(--qf-text); }
.qf-flow__switch button:focus-visible { outline: 2px solid var(--qf-accent); outline-offset: 1px; }
.qf-flow__chart--pie { overflow: auto; padding: 16px; border: 1px solid var(--qf-border-subtle); border-radius: var(--qf-radius-lg); background: var(--qf-bg-raised); }
.qf-flow__deficit { margin: 0 0 16px; padding: 10px 12px; border-left: 3px solid var(--qf-negative); background: var(--qf-bg-card); color: var(--qf-text-secondary); font-size: 12px; }
</style>
