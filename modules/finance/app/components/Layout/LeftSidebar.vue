<script setup lang="ts">
/**
 * The sidebar: the four numbers the plan produces, stacked.
 *
 * The leftover is the big one, because it is the figure you actually live by —
 * everything irregular comes out of it.
 */
import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import { formatCents } from '#finance/utils/money'

const app = useAppStore()
const plan = usePlanStore()

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)

/** How much of the income each block takes, for the little bars. */
function share(cents: number): number {
  const income = plan.summary.incomeCents
  return income > 0 ? Math.min(100, Math.round((cents / income) * 100)) : 0
}
</script>

<template>
  <div class="qf-ls">
    <section class="qf-ls__lead" :class="{ 'is-over': plan.overspent }">
      <span class="qf-ls__lead-label">{{ plan.overspent ? 'Short by' : 'Leftover' }}</span>
      <strong class="qf-num">{{ money(Math.abs(plan.summary.leftoverCents)) }}</strong>
      <span class="qf-ls__lead-per">per month</span>
    </section>

    <section class="qf-ls__block">
      <div class="qf-ls__row">
        <span class="qf-dot" data-color="income" />
        <span class="qf-ls__name">Income</span>
        <span class="qf-num">{{ money(plan.summary.incomeCents) }}</span>
      </div>

      <div class="qf-ls__row">
        <span class="qf-dot" data-color="saving" />
        <span class="qf-ls__name">Saving</span>
        <span class="qf-num">{{ money(plan.summary.savingCents) }}</span>
      </div>
      <div class="qf-ls__bar">
        <span class="qf-ls__fill" data-color="saving" :style="{ width: `${share(plan.summary.savingCents)}%` }" />
      </div>

      <div class="qf-ls__row">
        <span class="qf-dot" data-color="expense" />
        <span class="qf-ls__name">Fixed</span>
        <span class="qf-num">{{ money(plan.summary.expenseCents) }}</span>
      </div>
      <div class="qf-ls__bar">
        <span class="qf-ls__fill" data-color="expense" :style="{ width: `${share(plan.summary.expenseCents)}%` }" />
      </div>
    </section>

    <section v-if="plan.summary.savingsRate !== null" class="qf-ls__rate">
      <span>Savings rate</span>
      <strong>{{ plan.summary.savingsRate }}%</strong>
    </section>
  </div>
</template>

<style scoped>
.qf-ls {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 0;
  padding: 12px 10px;
  overflow-y: auto;
}

.qf-ls__lead {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px;
  border: 1px solid var(--qf-border);
  border-radius: var(--qf-radius-lg);
  background: var(--qf-bg-raised);
}

.qf-ls__lead.is-over {
  border-color: color-mix(in srgb, var(--qf-negative) 60%, var(--qf-border));
}

.qf-ls__lead-label {
  color: var(--qf-text-muted);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
}

.qf-ls__lead strong {
  color: var(--qf-flow-leftover);
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.qf-ls__lead.is-over strong {
  color: var(--qf-negative);
}

.qf-ls__lead-per {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-ls__block {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.qf-ls__row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 2px 2px;
  font-size: 12px;
}

.qf-ls__name {
  flex: 1;
  min-width: 0;
  color: var(--qf-text-secondary);
}

.qf-ls__bar {
  height: 4px;
  margin: 0 2px 6px;
  border-radius: 2px;
  background: var(--qf-bg-card);
  overflow: hidden;
}

.qf-ls__fill {
  --qf-c: var(--qf-flow-expense);
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--qf-c);
  transition: width 160ms ease;
}

.qf-ls__fill[data-color='saving'] { --qf-c: var(--qf-flow-saving); }
.qf-ls__fill[data-color='expense'] { --qf-c: var(--qf-flow-expense); }

.qf-ls__rate {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 8px 10px;
  border: 1px solid var(--qf-border-subtle);
  border-radius: var(--qf-radius);
  color: var(--qf-text-secondary);
  font-size: 12px;
}

.qf-ls__rate strong {
  color: var(--qf-text);
  font-size: 15px;
}
</style>
