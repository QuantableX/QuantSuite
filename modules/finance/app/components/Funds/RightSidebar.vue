<script setup lang="ts">
import { useAppStore } from '#finance/stores/app'
import { useFundsStore } from '#finance/stores/funds'
import { formatCents } from '#finance/utils/money'
import type { FundPlan } from '#finance/types/funds'

const app = useAppStore()
const store = useFundsStore()
const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
const displayDate = (date: string) => new Intl.DateTimeFormat(app.settings.locale, { dateStyle: 'medium' }).format(new Date(`${date}T12:00:00`))
const cadence = (months: number) => ({ 1: 'Monthly', 3: 'Quarterly', 6: 'Half-yearly', 12: 'Yearly' })[months] ?? `${months} months`
const due = (plan: FundPlan) => plan.isActive && new Date(`${plan.nextOn}T00:00:00`) <= new Date()
const progress = computed(() => store.selected?.targetCents ? Math.min(100, store.selected.currentValueCents / store.selected.targetCents * 100) : 0)
const remaining = computed(() => Math.max(0, (store.selected?.targetCents ?? 0) - (store.selected?.currentValueCents ?? 0)))

function editPlan(plan?: FundPlan) {
  if (store.selected) store.openEditor({ kind: 'plan', fundId: store.selected.id, id: plan?.id })
}
async function togglePlan(plan: FundPlan) {
  await store.savePlan({ fundId: plan.fundId, name: plan.name, amountCents: plan.amountCents, everyMonths: plan.everyMonths, nextOn: plan.nextOn, isActive: !plan.isActive }, plan.id)
}
</script>

<template>
  <div class="qff-right">
    <section>
      <header class="qff-right__head"><h2>Target</h2><button v-if="store.selected" class="qff-right__text" :disabled="store.busy" @click="store.openEditor({ kind: 'fund', id: store.selected.id })">{{ store.selected.targetCents ? 'Edit' : 'Set target' }}</button></header>
      <div v-if="store.selected?.targetCents" class="qff-right__target">
        <span class="qff-right__name">{{ store.selected.name }}</span>
        <strong>{{ money(store.selected.currentValueCents) }}</strong>
        <small>of {{ money(store.selected.targetCents) }}</small>
        <progress :value="progress" max="100" :aria-label="`Progress towards ${money(store.selected.targetCents)}`" />
        <div class="qff-right__progress"><span>{{ Math.round(progress) }}%</span><span>{{ remaining ? `${money(remaining)} to go` : 'Target reached' }}</span></div>
      </div>
      <p v-else class="qff-right__hint">{{ store.selected ? 'Add an optional target to see how close this fund is to your goal.' : 'Select a fund to follow its target and planned contributions.' }}</p>
    </section>

    <section class="qff-right__plans">
      <header class="qff-right__head">
        <h2>Savings plans</h2>
        <button class="qff-right__add" aria-label="New savings plan" title="New savings plan" :disabled="store.busy || !store.selected" @click="editPlan()">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14 M5 12h14" /></svg>
        </button>
      </header>
      <template v-if="store.selected">
        <p v-if="!store.selected.plans.length" class="qff-right__hint">Set up a regular contribution for {{ store.selected.name }}.</p>
        <button v-if="!store.selected.plans.length" class="qf-btn qff-right__wide" :disabled="store.busy" @click="editPlan()">Add savings plan</button>
        <article v-for="plan in store.selected.plans" :key="plan.id" class="qff-right__plan" :class="{ 'is-due': due(plan) }">
          <span class="qff-right__name">{{ plan.name }}</span>
          <div class="qff-right__amount"><strong>{{ money(plan.amountCents) }}</strong><small>{{ cadence(plan.everyMonths) }}</small></div>
          <p class="qff-right__date">{{ !plan.isActive ? 'Paused · next' : due(plan) ? 'Due' : 'Next' }} {{ displayDate(plan.nextOn) }}</p>
          <button v-if="due(plan)" class="qf-btn qff-right__wide" :disabled="store.busy" @click="store.bookPlan(plan.id, plan.nextOn)">Book deposit</button>
          <div class="qff-right__actions">
            <button class="qff-right__text" :disabled="store.busy" @click="togglePlan(plan)">{{ plan.isActive ? 'Pause' : 'Resume' }}</button>
            <button class="qff-right__text" :disabled="store.busy" @click="editPlan(plan)">Edit</button>
            <button class="qff-right__text" :disabled="store.busy" :aria-label="`Delete ${plan.name}`" @click="store.openEditor({ kind: 'delete', recordKind: 'plan', id: plan.id, name: plan.name })">Delete</button>
          </div>
        </article>
        <p class="qff-right__hint">Active savings plans appear in Saving &amp; investing and the cash-flow chart. Book each deposit once it arrives to update the fund balance.</p>
      </template>
      <p v-else class="qff-right__hint">Monthly, quarterly or yearly contributions. Due deposits will appear here for confirmation.</p>
    </section>
  </div>
</template>

<style scoped>
.qff-right { display: flex; flex-direction: column; gap: 20px; padding: 12px 10px; }
.qff-right__head { display: flex; justify-content: space-between; align-items: center; gap: 8px; min-height: 24px; margin-bottom: 8px; padding: 0 2px; }
.qff-right__head h2 { margin: 0; color: var(--qf-text-muted); font-size: 10px; font-weight: 700; letter-spacing: .07em; text-transform: uppercase; }
.qff-right__hint { color: var(--qf-text-muted); font-size: 11px; line-height: 1.7; margin: 6px 2px; }
.qff-right__target, .qff-right__plan { display: flex; flex-direction: column; gap: 6px; border: 1px solid var(--qf-border-subtle); border-radius: var(--qf-radius-lg); padding: 12px; }
.qff-right__target > strong { font-size: 20px; font-weight: 600; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.qff-right__name { color: var(--qf-text-secondary); font-size: 12px; overflow-wrap: anywhere; }
.qff-right small { color: var(--qf-text-muted); font-size: 10px; }
.qff-right progress { appearance: none; border: 0; border-radius: 3px; width: 100%; height: 4px; margin-top: 4px; overflow: hidden; background: var(--qf-bg-card); }
.qff-right progress::-webkit-progress-bar { background: var(--qf-bg-card); }
.qff-right progress::-webkit-progress-value { background: var(--qf-flow-saving); }
.qff-right__progress { display: flex; justify-content: space-between; flex-wrap: wrap; gap: 4px; color: var(--qf-text-muted); font-size: 10px; }
.qff-right__plans { border-top: 1px solid var(--qf-border-subtle); padding-top: 12px; }
.qff-right__text, .qff-right__add { border: none; background: transparent; color: var(--qf-text-muted); padding: 3px; cursor: pointer; }
.qff-right__text { font-size: 11px; }
.qff-right__add { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--qf-radius); }
.qff-right__text:hover, .qff-right__add:hover { color: var(--qf-text); background: var(--qf-bg-hover); }
.qff-right__plan { margin-bottom: 8px; }
.qff-right__plan.is-due { border-left: 2px solid var(--qf-flow-saving); }
.qff-right__amount { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 4px; }
.qff-right__amount strong { font-size: 15px; font-weight: 600; font-variant-numeric: tabular-nums; }
.qff-right__date { margin: 0; color: var(--qf-text-muted); font-size: 10px; }
.qff-right__actions { display: flex; gap: 8px; }
.qff-right__wide { width: 100%; justify-content: center; font-size: 11px; margin-top: 4px; }
.qff-right button:disabled { opacity: .5; cursor: default; }
</style>
