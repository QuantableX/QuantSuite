<script setup lang="ts">
import { ArrowLeft, Pencil, Plus, Wallet } from 'lucide-vue-next'
import { usePortfolioStore } from '#terminal/stores/portfolio'
import { usePortfolioPanel } from '#terminal/composables/usePortfolioPanel'
import { assetColor, portfolioMoney as money, portfolioNumber as number, portfolioPrice as price } from '#terminal/utils/portfolio'

const store = usePortfolioStore()
const { selectedHolding, editor, deleting, view, historyFor } = usePortfolioPanel()
const holding = computed(() => store.holdings.find(h => h.asset.id === selectedHolding.value))
const locked = computed(() => !store.loaded || store.loading || store.busy || !!editor.value || !!deleting.value)
const recent = computed(() => [
  ...store.document.trades.map(t => ({ id: t.id, source: 'trade' as const, date: t.date, order: t.order, label: `${t.kind === 'adjustment' ? 'Adjusted' : t.kind === 'opening' ? 'Opening' : t.kind === 'buy' ? 'Bought' : 'Sold'} ${store.document.assets.find(a => a.id === t.assetId)?.symbol ?? 'asset'}`, detail: `${number(t.quantity)} units` })),
  ...(store.document.cashEntries ?? []).map(c => ({ id: c.id, source: 'cash' as const, date: c.date, order: c.order, label: c.kind === 'balance' ? 'Cash balance set' : c.kind === 'deposit' ? 'Cash deposited' : 'Cash withdrawn', detail: money(c.amount) })),
].sort((a, b) => Date.parse(b.date) - Date.parse(a.date) || b.order - a.order).slice(0, 5))
const date = (value: string) => new Date(value).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
const signed = (value: number | null) => value === null ? '—' : `${value > 0 ? '+' : ''}${money(value)}`
const tone = (value: number | null) => value === null || value === 0 ? '' : value > 0 ? 'is-up' : 'is-down'
</script>

<template>
  <div class="qp-panel" aria-label="Portfolio details">
    <section class="qp-panel-cash">
      <div class="qp-panel-label"><Wallet :size="13" /><span>Available cash</span><span>USD</span></div>
      <strong class="qp-panel-value">{{ money(store.cash) }}</strong>
      <p>{{ store.totals.value > 0 ? `${(store.cash / store.totals.value * 100).toFixed(1)}% of portfolio` : 'Ready for your next investment' }}</p>
      <div class="qp-panel-actions"><button :disabled="locked" aria-label="Deposit cash" @click="editor = { kind: 'cash', cashAction: 'deposit' }"><Plus :size="12" />Deposit</button><button :disabled="locked || store.cash <= 0" aria-label="Withdraw cash" @click="editor = { kind: 'cash', cashAction: 'withdrawal' }">Withdraw</button></div>
      <div class="qp-panel-links"><button :disabled="locked" aria-label="Set cash balance" @click="editor = { kind: 'cash', cashAction: 'balance' }">Set balance</button><button @click="view = 'cash'">Cash history</button></div>
    </section>
    <section v-if="holding" class="qp-panel-position">
      <button class="qp-panel-back" @click="selectedHolding = ''"><ArrowLeft :size="12" />Portfolio overview</button>
      <div class="qp-panel-asset"><i :style="{ background: assetColor(holding.asset.id) }" /><h2>{{ holding.asset.symbol }}</h2><span v-if="!holding.quantity">Closed</span></div>
      <p class="qp-panel-name">{{ holding.asset.name }}</p>
      <strong class="qp-panel-value">{{ money(holding.value) }}</strong>
      <dl><div><dt>Quantity</dt><dd>{{ number(holding.quantity) }}</dd></div><div><dt>Average cost</dt><dd>{{ price(holding.averageCost) }}</dd></div><div><dt>Current price</dt><dd>{{ price(holding.price) }}</dd></div><div><dt>Unrealized</dt><dd :class="tone(holding.unrealized)">{{ signed(holding.unrealized) }}</dd></div><div><dt>Realized</dt><dd :class="tone(holding.realized)">{{ signed(holding.realized) }}</dd></div></dl>
      <div class="qp-panel-actions"><button :disabled="locked" :aria-label="`Manage ${holding.asset.symbol} holding`" @click="editor = { kind: 'trade', assetId: holding.asset.id }"><Pencil :size="13" />Manage holding</button></div>
      <div class="qp-panel-links"><button @click="historyFor(holding.asset.id)">Trade history</button></div>
      <div class="qp-panel-note-head"><h3>Holding notes</h3><button :disabled="locked" :aria-label="`Edit ${holding.asset.symbol} notes`" @click="editor = { kind: 'asset', assetId: holding.asset.id, focusNotes: true }"><Pencil :size="12" /></button></div>
      <p class="qp-panel-notes">{{ holding.asset.notes?.trim() || 'Keep your thesis, entry criteria and exit plan here.' }}</p>
      <button class="qp-panel-delete" :disabled="locked" @click="store.error = ''; deleting = { kind: 'asset', id: holding.asset.id, name: holding.asset.name }">Delete asset…</button>
    </section>
    <template v-else>
      <section class="qp-panel-guide"><h3>{{ store.document.assets.length ? 'Holding details' : 'Build your portfolio' }}</h3><p>{{ store.document.assets.length ? 'Select a holding or an allocation slice to view notes, record a trade or edit the position.' : 'Add the investments you already own and set your available cash. New purchases can then use that cash.' }}</p></section>
      <section class="qp-panel-recent"><h3>Recent activity</h3><p v-if="!recent.length">Your trades and cash movements will appear here.</p><button v-for="entry in recent" :key="`${entry.source}:${entry.id}`" class="qp-panel-activity" :disabled="locked" :aria-label="`Edit ${entry.label}`" @click="editor = entry.source === 'trade' ? { kind: 'trade', tradeId: entry.id } : { kind: 'cash', cashEntryId: entry.id }"><span>{{ entry.label }}<small>{{ entry.detail }}</small></span><time :datetime="entry.date">{{ date(entry.date) }}</time></button></section>
    </template>
  </div>
</template>

<style scoped>
.qp-panel { display: flex; flex-direction: column; font-size: 12px; min-width: 0; min-height: 0; overflow-y: auto; overscroll-behavior: contain; }
.qp-panel section { flex-shrink: 0; }
.qp-panel section { padding: 16px 14px; border-bottom: 1px solid var(--border); }
.qp-panel-label { display: flex; align-items: center; gap: 6px; color: var(--text-secondary); font-size: 11px; }
.qp-panel-label span:last-child { margin-left: auto; font-size: 10px; }
.qp-panel-value { display: block; font-size: 24px; line-height: 1.3; font-weight: 600; margin-top: 9px; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
.qp-panel p { color: var(--text-secondary); font-size: 11px; line-height: 1.65; margin: 5px 0 12px; overflow-wrap: anywhere; }
.qp-panel button { display: inline-flex; align-items: center; justify-content: center; gap: 4px; padding: 7px 8px; font-size: 11px; color: var(--text-primary); background: var(--surface-2); border: 1px solid var(--border); border-radius: 5px; cursor: pointer; }
.qp-panel button:hover { background: var(--surface-3); }
.qp-panel button:disabled { opacity: .45; cursor: default; }
.qp-panel-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-top: 12px; }
.qp-panel-actions > button:only-child { grid-column: 1 / -1; }
.qp-panel-links { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px; margin-top: 6px; }
.qp-panel-links button, .qp-panel .qp-panel-back, .qp-panel-note-head button, .qp-panel .qp-panel-delete { padding: 5px 0; background: transparent; border: 0; color: var(--text-secondary); }
.qp-panel .qp-panel-back { margin: -4px 0 14px; font-size: 10px; }
.qp-panel h2, .qp-panel h3 { margin: 0; font-weight: 600; }
.qp-panel h2 { font-size: 18px; overflow-wrap: anywhere; min-width: 0; }
.qp-panel h3 { font-size: 12px; }
.qp-panel-asset { display: flex; align-items: center; gap: 8px; }
.qp-panel-asset i { width: 10px; height: 10px; border-radius: 3px; flex-shrink: 0; }
.qp-panel-asset > span { font-size: 10px; color: var(--text-secondary); }
.qp-panel dl { margin: 14px 0; display: flex; flex-direction: column; gap: 10px; font-size: 11px; }
.qp-panel dl > div { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 8px; }
.qp-panel dt { color: var(--text-secondary); }.qp-panel dd { margin: 0; overflow-wrap: anywhere; min-width: 0; font-variant-numeric: tabular-nums; }
.qp-panel-note-head { display: flex; align-items: center; justify-content: space-between; margin-top: 22px; }
.qp-panel .qp-panel-notes { white-space: pre-wrap; max-height: 220px; overflow: auto; }
.qp-panel .qp-panel-delete { margin-top: 16px; font-size: 10px; }.qp-panel .qp-panel-delete:hover { color: var(--negative); }
.qp-panel .qp-panel-activity { width: 100%; justify-content: space-between; align-items: flex-start; gap: 8px; padding: 12px 0; background: transparent; border: 0; border-bottom: 1px solid var(--border); border-radius: 0; text-align: left; }
.qp-panel-activity span { min-width: 0; overflow-wrap: anywhere; }.qp-panel-activity small { display: block; margin-top: 4px; color: var(--text-secondary); }.qp-panel-activity time { flex-shrink: 0; font-size: 10px; color: var(--text-secondary); }
.qp-panel .is-up { color: var(--positive); }.qp-panel .is-down { color: var(--negative); }
</style>
