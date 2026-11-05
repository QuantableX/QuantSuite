<script setup lang="ts">
/**
 * The plan — the page you actually type into.
 *
 * Three blocks and one line at the bottom:
 *
 *     in       3.000
 *   − saving     500
 *   − fixed    1.400
 *   ───────────────────
 *   = leftover 1.100    ← covers everything irregular
 *
 * Irregular spending is never entered. That is not an omission, it is the
 * model: what is left over is the budget for it, and one number is easier to
 * live by than a hundred receipts.
 */
definePageMeta({ layout: 'finance' })

import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import { formatCents } from '#finance/utils/money'

const app = useAppStore()
const plan = usePlanStore()

const money = (cents: number) => formatCents(cents, app.settings.currency)
</script>

<template>
  <div class="qf-plan">
    <QPageHeading class="qf-plan__heading" title="Monthly plan" />
    <p v-if="plan.error" class="qf-plan__error" role="alert">{{ plan.error }}</p>

    <!-- Side by side, so the whole month and its leftover are on screen at once. -->
    <div class="qf-plan__cols">
      <FinancePlanSection
        kind="income"
        title="Income"
        :total-cents="plan.summary.incomeCents"
      />

      <FinancePlanSection
        kind="saving"
        title="Saving & investing"
        :total-cents="plan.summary.savingCents"
      />

      <FinancePlanSection
        kind="expense"
        title="Fixed costs"
        :total-cents="plan.summary.expenseCents"
      />
    </div>

    <footer class="qf-plan__foot" :class="{ 'is-over': plan.overspent }">
      <div class="qf-plan__leftover">
        <span class="qf-plan__leftover-label">
          {{ plan.overspent ? 'Short by' : 'Leftover' }}
        </span>
        <strong class="qf-num">{{ money(Math.abs(plan.summary.leftoverCents)) }}</strong>
        <span class="qf-plan__per">per month</span>
      </div>
      <p class="qf-plan__note">
        <template v-if="plan.overspent">
          Planned spending exceeds income.
        </template>
        <template v-else>
          Available for variable spending.
        </template>
      </p>
    </footer>
  </div>
</template>

<style scoped>
.qf-plan__heading.qf-plan__heading { margin-bottom: 0; }
.qf-plan__error { margin: 0; color: var(--qf-negative); font-size: 12px; }
.qf-plan {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  min-height: 0;
  padding: var(--qss-page-edge);
  overflow-y: auto;
  container-type: inline-size;
}

/* The columns take the height; each scrolls its own rows, the page does not scroll. */
.qf-plan__cols {
  flex: 1 1 0;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--qss-gap);
}

.qf-plan__foot {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap);
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
  border: 1px solid var(--qf-border);
  border-radius: var(--qss-panel-radius);
  background: var(--qf-bg-raised);
}

/* A row needs about 400px or its name input collapses: two columns next, income above saving. */
@container (max-width: 1180px) {
  .qf-plan__cols {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-template-rows: auto minmax(0, 1fr);
  }
  .qf-plan__cols > :nth-child(3) {
    grid-column: 2;
    grid-row: 1 / span 2;
  }
}

/* One column: one after the other, and the page scrolls. */
@container (max-width: 780px) {
  .qf-plan__cols {
    flex: none;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: none;
  }
  .qf-plan__cols > :nth-child(3) {
    grid-column: auto;
    grid-row: auto;
  }
}

.qf-plan__foot.is-over {
  border-color: color-mix(in srgb, var(--qf-negative) 60%, var(--qf-border));
}

.qf-plan__leftover {
  display: flex;
  align-items: baseline;
  gap: 10px;
  flex-shrink: 0;
}

.qf-plan__leftover-label {
  color: var(--qf-text-secondary);
  font-size: var(--qss-text-label);
  font-weight: 500;
}

.qf-plan__leftover strong {
  color: var(--qf-flow-leftover);
  font-size: var(--qss-text-metric);
  font-weight: 600;
  letter-spacing: -0.02em;
}

.qf-plan__foot.is-over .qf-plan__leftover strong {
  color: var(--qf-negative);
}

.qf-plan__per {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-plan__note {
  margin: 0;
  color: var(--qf-text-muted);
  font-size: var(--qss-text-label);
  line-height: 1.5;
}
</style>
