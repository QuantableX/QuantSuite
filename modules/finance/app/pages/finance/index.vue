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

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
</script>

<template>
  <div class="qf-plan">
    <div class="qf-plan__scroll">
      <div class="qf-plan__sheet">
        <QPageHeading title="Monthly plan" />
        <p v-if="plan.error" class="qf-plan__error" role="alert">{{ plan.error }}</p>
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
    </div>
  </div>
</template>

<style scoped>
.qf-plan__error { color: var(--qf-negative); font-size: 12px; }
.qf-plan {
  height: 100%;
  min-height: 0;
}

.qf-plan__scroll {
  height: 100%;
  overflow-y: auto;
}

.qf-plan__sheet {
  display: flex;
  flex-direction: column;
  gap: 26px;
  width: min(820px, 100%);
  margin: 0 auto;
  padding: 20px 18px 32px;
}

.qf-plan__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  padding: 16px 18px;
  border: 1px solid var(--qf-border);
  border-radius: var(--qf-radius-lg);
  background: var(--qf-bg-raised);
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
  color: var(--qf-text-muted);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qf-plan__leftover strong {
  color: var(--qf-flow-leftover);
  font-size: 28px;
  font-weight: 700;
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
  font-size: 12px;
  line-height: 1.5;
  text-align: right;
}
</style>
