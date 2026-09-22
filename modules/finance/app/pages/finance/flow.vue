<script setup lang="ts">
/** The plan as a flow — where the money goes, at a glance. */
definePageMeta({ layout: 'finance' })

import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import { formatCents } from '#finance/utils/money'

const app = useAppStore()
const plan = usePlanStore()

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

    <div class="qf-flow__chart">
      <FinanceSankeyChart />
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
</style>
