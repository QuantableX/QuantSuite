<script setup lang="ts">
import { useAppStore } from '#finance/stores/app'
import { useFundsStore } from '#finance/stores/funds'
import { formatCents } from '#finance/utils/money'

const app = useAppStore()
const store = useFundsStore()
const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
</script>

<template>
  <div class="qff-left">
    <button class="qf-btn qff-left__create" :disabled="store.busy || !store.loaded" @click="store.openEditor({ kind: 'fund' })">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14 M5 12h14" /></svg>
      New fund
    </button>

    <section class="qff-left__total">
      <span class="qff-left__label">Total current value</span>
      <strong>{{ money(store.totals.value) }}</strong>
      <small>Across {{ store.funds.length }} {{ store.funds.length === 1 ? 'fund' : 'funds' }}</small>
    </section>
    <dl class="qff-left__figures">
      <div><dt>Net contributed</dt><dd>{{ money(store.totals.input) }}</dd></div>
      <div><dt>Value change</dt><dd :class="store.totals.gain < 0 ? 'qf-neg' : store.totals.gain > 0 ? 'qf-pos' : ''">{{ money(store.totals.gain) }}</dd></div>
    </dl>

    <section class="qff-left__funds">
      <header><span class="qff-left__label">Funds</span><span>{{ store.funds.length }}</span></header>
      <p v-if="!store.funds.length">Your funds will appear here. Start with a reserve, an investment pot or a savings goal.</p>
      <nav v-else aria-label="Funds">
        <button v-for="fund in store.funds" :key="fund.id" :aria-pressed="fund.id === store.selectedId" :class="{ 'is-active': fund.id === store.selectedId }" @click="store.selectedId = fund.id">
          <span>{{ fund.name }}</span>
          <strong>{{ money(fund.currentValueCents) }}</strong>
          <small v-if="fund.targetCents">{{ Math.min(100, Math.round(fund.currentValueCents / fund.targetCents * 100)) }}% of target</small>
        </button>
      </nav>
    </section>
  </div>
</template>

<style scoped>
.qff-left { display: flex; flex-direction: column; gap: 14px; padding: 12px 10px; }
.qff-left__create { justify-content: center; width: 100%; }
.qff-left__total { display: flex; flex-direction: column; gap: 4px; padding: 12px; border: 1px solid var(--qf-border); border-radius: var(--qf-radius-lg); }
.qff-left__label { color: var(--qf-text-muted); font-size: 10px; font-weight: 700; letter-spacing: .07em; text-transform: uppercase; }
.qff-left__total strong { font-size: 22px; font-weight: 650; letter-spacing: -.02em; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.qff-left small { font-size: 10px; color: var(--qf-text-muted); }
.qff-left__figures { display: flex; flex-direction: column; gap: 8px; margin: 0; padding: 0 3px; font-size: 11px; }
.qff-left__figures div { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 8px; }
.qff-left__figures dt { color: var(--qf-text-muted); }
.qff-left__figures dd { margin: 0; font-variant-numeric: tabular-nums; }
.qff-left__funds { border-top: 1px solid var(--qf-border-subtle); padding-top: 14px; }
.qff-left__funds header { display: flex; justify-content: space-between; align-items: center; padding: 0 3px 10px; font-size: 10px; color: var(--qf-text-muted); }
.qff-left__funds p { margin: 0; padding: 4px 3px; color: var(--qf-text-muted); font-size: 11px; line-height: 1.7; }
.qff-left nav { display: flex; flex-direction: column; gap: 4px; }
.qff-left nav button { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; width: 100%; padding: 10px; text-align: left; border: 1px solid transparent; border-radius: var(--qf-radius); background: transparent; color: var(--qf-text-secondary); cursor: pointer; overflow-wrap: anywhere; }
.qff-left nav button:hover { background: var(--qf-bg-hover); }
.qff-left nav button.is-active { background: var(--qf-bg-card); border-color: var(--qf-border-subtle); color: var(--qf-text); }
.qff-left nav button > span { font-size: 12px; }
.qff-left nav strong { font-size: 14px; font-weight: 600; font-variant-numeric: tabular-nums; }
.qff-left button:disabled { opacity: .5; cursor: default; }
</style>
