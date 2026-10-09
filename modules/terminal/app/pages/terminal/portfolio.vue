<script setup lang="ts">
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { usePortfolioStore } from '#terminal/stores/portfolio'
import type { PortfolioTrade } from '#terminal/types/portfolio'
import { assetColor, portfolioMoney as money, portfolioNumber as number, portfolioPrice as price } from '#terminal/utils/portfolio'

definePageMeta({ layout: 'terminal' })
const store = usePortfolioStore()
const view = ref<'holdings' | 'history'>('holdings')
const currentOnly = ref(true)
const search = ref('')
const editor = ref<{ kind: 'asset' | 'trade'; assetId?: string; tradeId?: string; side?: 'buy' | 'sell'; focusNotes?: boolean } | null>(null)
const deleting = ref<{ kind: 'asset' | 'trade'; id: string; name: string } | null>(null)
const deleteDialog = ref<HTMLDialogElement | null>(null)
const selectedAsset = ref('')
const selectedKind = ref('')
const rows = computed(() => store.holdings
  .filter((h) => (!currentOnly.value || h.quantity > 0) && `${h.asset.symbol} ${h.asset.name}`.toLowerCase().includes(search.value.toLowerCase()))
  .sort((a, b) => Number(b.quantity > 0) - Number(a.quantity > 0) || (b.value ?? 0) - (a.value ?? 0) || a.asset.symbol.localeCompare(b.asset.symbol)))
const assetName = (id: string) => store.document.assets.find((a) => a.id === id)?.symbol ?? 'Unknown asset'
const trades = computed(() => [...store.document.trades]
  .filter((t) => (!selectedAsset.value || t.assetId === selectedAsset.value) && (!selectedKind.value || t.kind === selectedKind.value))
  .sort((a, b) => Date.parse(b.date) - Date.parse(a.date) || b.order - a.order))
const signed = (value: number | null) => value === null ? '—' : `${value > 0 ? '+' : ''}${money(value)}`
const tone = (value: number | null) => value === null || value === 0 ? '' : value > 0 ? 'is-up' : 'is-down'
const percent = (value: number, cost: number) => cost > 0 ? `${value >= 0 ? '+' : ''}${(value / cost * 100).toFixed(2)}%` : '—'
const displayDate = (value: string) => new Date(value).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
const tradeValue = (trade: PortfolioTrade) => trade.kind === 'adjustment' ? null : trade.quantity * trade.price + (trade.kind === 'sell' ? -trade.fees : trade.fees)
const locked = computed(() => !store.loaded || store.busy || store.loading || store.refreshing)
let timer: ReturnType<typeof setInterval> | undefined
async function start() {
  if (timer) return
  timer = setInterval(() => { if (!editor.value && !deleting.value) void store.refreshPrices() }, 60000)
  await store.load()
  if (timer && !editor.value) await store.refreshPrices()
}
function stop() { clearInterval(timer); timer = undefined }
onMounted(() => { if (inActiveKeepAliveTree()) void start() })
onActivated(() => void start())
onDeactivated(stop)
onBeforeUnmount(stop)
async function askDelete(kind: 'asset' | 'trade', id: string, name: string) {
  store.error = ''
  deleting.value = { kind, id, name }
  await nextTick()
  deleteDialog.value?.showModal()
}
async function remove() {
  if (!deleting.value || store.busy) return
  const target = deleting.value
  const ok = target.kind === 'asset' ? await store.removeAsset(target.id) : await store.removeTrade(target.id)
  if (ok) { deleting.value = null; deleteDialog.value?.close() }
}
function historyFor(id: string) { selectedAsset.value = id; view.value = 'history' }
</script>

<template>
  <div class="qp-page">
    <header class="qp-heading"><div><h1>Portfolio</h1><p>Your holdings, trades and performance · USD</p></div><div class="qp-actions"><button :disabled="locked" @click="editor = { kind: 'asset' }">Add holding</button><button :disabled="locked || !store.document.assets.length" @click="editor = { kind: 'trade' }">Add trade</button><button :disabled="locked || !store.document.assets.some(a => a.source === 'market')" @click="store.refreshPrices()">{{ store.refreshing ? 'Refreshing…' : 'Refresh prices' }}</button></div></header>
    <div v-if="store.error && !editor && !deleting" class="qp-alert" role="alert">{{ store.error }} <button :disabled="store.busy || store.loading" @click="store.load()">Reload</button></div>
    <p v-if="store.loading && !store.loaded" class="qp-empty" role="status">Loading portfolio…</p>
    <template v-else-if="store.loaded">
      <div class="qp-stats">
        <div><span>{{ store.totals.missing ? 'Priced holdings value' : 'Holdings value' }}</span><strong>{{ money(store.totals.value) }}</strong><small>{{ store.totals.count }} current {{ store.totals.count === 1 ? 'holding' : 'holdings' }}</small></div>
        <div><span>Remaining cost</span><strong>{{ money(store.totals.cost) }}</strong><small>Average cost including buy fees</small></div>
        <div><span>Unrealized gain</span><strong :class="tone(store.totals.unrealized)">{{ signed(store.totals.unrealized) }}</strong><small>{{ store.totals.unrealized === null ? 'Current prices missing' : percent(store.totals.unrealized, store.totals.cost) }}</small></div>
        <div><span>Realized gain</span><strong :class="tone(store.totals.realized)">{{ signed(store.totals.realized) }}</strong><small>Closed quantities, after fees</small></div>
        <div><span>Total gain</span><strong :class="tone(store.totals.totalGain)">{{ signed(store.totals.totalGain) }}</strong><small>Realized + unrealized</small></div>
      </div>
      <p v-if="store.quoteError" class="qp-alert" role="status">{{ store.quoteError }}</p>
      <p v-if="store.totals.missing" class="qp-alert" role="status">{{ store.totals.missing }} holding(s) have no current price. They are excluded from the priced value and pie; total gains stay unavailable until priced.</p>
      <nav class="qp-tabs" aria-label="Portfolio views"><button :aria-pressed="view === 'holdings'" @click="view = 'holdings'">Current holdings</button><button :aria-pressed="view === 'history'" @click="view = 'history'">Trade history <span>{{ store.document.trades.length }}</span></button></nav>
      <template v-if="view === 'holdings'">
        <div v-if="!store.document.assets.length" class="qp-empty"><h2>Start with your current holdings</h2><p>Add an asset, its quantity and average cost. You can enter older trades later or record new buys and sells.</p><button @click="editor = { kind: 'asset' }">Add your first holding</button></div>
        <div v-else class="qp-dashboard">
          <TerminalPortfolioCharts :holdings="store.holdings" :history="store.history" @select="!locked && (editor = { kind: 'asset', assetId: $event })" />
          <section class="qp-holdings-panel" aria-label="Holdings panel">
          <div class="qp-holdings-head"><h2>Holdings</h2><label class="qp-search"><span class="sr-only">Find a holding</span><input v-model="search" type="search" placeholder="Symbol or name" /></label><label class="qp-checkbox"><input v-model="currentOnly" type="checkbox" /> Current holdings only</label></div>
          <div class="qp-table-wrap" tabindex="0" role="region" aria-label="Scrollable holdings"><table class="qp-table" aria-label="Portfolio holdings"><thead><tr><th>Asset</th><th>Quantity</th><th>Avg. cost</th><th>Current price</th><th>Value / share</th><th>Unrealized</th><th>Actions</th></tr></thead><tbody>
            <tr v-for="h in rows" :key="h.asset.id"><td><div class="qp-asset"><i :style="{ background: assetColor(h.asset.id) }" /><span><strong>{{ h.asset.symbol }}</strong><small>{{ h.asset.name }}{{ h.quantity === 0 ? ' · Closed / no holding' : '' }}</small><button class="qp-holding-note" :disabled="locked" :aria-label="`Notes for ${h.asset.symbol}`" :title="h.asset.notes || 'Add holding notes'" @click="editor = { kind: 'asset', assetId: h.asset.id, focusNotes: true }">{{ h.asset.notes?.trim() || 'Add notes' }}</button></span></div></td><td>{{ number(h.quantity) }}</td><td>{{ price(h.averageCost) }}</td><td :title="h.asset.source === 'manual' ? displayDate(h.asset.updatedAt) : store.document.quotes[h.asset.id] ? `Checked ${displayDate(store.document.quotes[h.asset.id]!.checkedAt)}` : 'Not priced yet'">{{ price(h.price) }}<small>{{ h.asset.source === 'manual' ? 'Manual' : 'Market snapshot' }}</small></td><td>{{ money(h.value) }}<small>{{ h.value !== null && store.totals.value > 0 ? `${(h.value / store.totals.value * 100).toFixed(1)}%` : '—' }}</small></td><td :class="tone(h.unrealized)">{{ signed(h.unrealized) }}<small>{{ h.unrealized === null ? '—' : percent(h.unrealized, h.cost) }}</small></td><td><div class="qp-row-actions"><button :disabled="locked" :aria-label="`Edit ${h.asset.symbol} holding`" @click="editor = { kind: 'asset', assetId: h.asset.id }">Edit</button><button :disabled="locked" :aria-label="`Trade ${h.asset.symbol}`" @click="editor = { kind: 'trade', assetId: h.asset.id }">Trade</button><button :aria-label="`${h.asset.symbol} trade history`" @click="historyFor(h.asset.id)">History</button><button :disabled="locked" :aria-label="`Delete ${h.asset.symbol} asset`" @click="askDelete('asset', h.asset.id, h.asset.name)">Delete</button></div></td></tr>
            <tr v-if="!rows.length"><td colspan="7" class="qp-no-rows">No matching current holdings. Clear the search or turn off “Current holdings only” to see closed and newly added assets.</td></tr>
          </tbody></table></div>
          </section>
        </div>
        <p v-if="store.document.assets.length" class="qp-caption">USD · Cached crypto quotes and manual prices · Sale proceeds are not automatically added as cash.</p>
      </template>
      <section v-else class="qp-history-panel" aria-label="Trade history panel">
        <div class="qp-history-head"><p>Opening balances, trades and position adjustments remain editable. Corrections recalculate all later holdings and gains.</p><div class="qp-actions"><label>Asset<select v-model="selectedAsset"><option value="">All assets</option><option v-for="a in store.document.assets" :key="a.id" :value="a.id">{{ a.symbol }} · {{ a.name }}</option></select></label><label>Entry type<select v-model="selectedKind"><option value="">All entries</option><option value="opening">Opening</option><option value="buy">Buy</option><option value="sell">Sell</option><option value="adjustment">Position adjustment</option></select></label></div></div>
        <div class="qp-table-wrap" tabindex="0" role="region" aria-label="Scrollable trade history"><table class="qp-table" aria-label="Portfolio trade history"><thead><tr><th>Date</th><th>Asset</th><th>Type</th><th>Quantity</th><th>Price</th><th>Fees</th><th>Net amount</th><th>Notes</th><th>Actions</th></tr></thead><tbody>
          <tr v-for="t in trades" :key="t.id"><td>{{ displayDate(t.date) }}</td><td>{{ assetName(t.assetId) }}</td><td><span class="qp-kind" :class="t.kind">{{ t.kind }}</span></td><td>{{ number(t.quantity) }}<small v-if="t.kind === 'adjustment'">Position total</small></td><td>{{ price(t.price) }}<small v-if="t.kind === 'adjustment'">Average cost</small></td><td>{{ t.kind === 'adjustment' ? '—' : money(t.fees) }}</td><td>{{ money(tradeValue(t)) }}</td><td class="qp-note">{{ t.notes || '—' }}</td><td><div class="qp-row-actions"><button :disabled="locked" :aria-label="`Edit ${assetName(t.assetId)} ${t.kind}`" @click="editor = { kind: 'trade', tradeId: t.id }">Edit</button><button :disabled="locked" :aria-label="`Delete ${assetName(t.assetId)} ${t.kind}`" @click="askDelete('trade', t.id, `${assetName(t.assetId)} ${t.kind}`)">Delete</button></div></td></tr>
          <tr v-if="!trades.length"><td colspan="9" class="qp-no-rows">No trades match these filters.</td></tr>
        </tbody></table></div>
      </section>
    </template>
    <TerminalPortfolioEditor v-if="editor" v-bind="editor" @close="editor = null" />
    <dialog ref="deleteDialog" class="qp-delete" aria-labelledby="qp-delete-title" @cancel="store.busy ? $event.preventDefault() : (deleting = null)"><h2 id="qp-delete-title">Delete {{ deleting?.name }}?</h2><p>{{ deleting?.kind === 'asset' ? 'This removes the asset and all of its trade entries. Other holdings stay unchanged.' : 'Holdings and gains will be recalculated. Deletion is rejected if a later sale would exceed the remaining quantity.' }}</p><p v-if="store.error" class="qp-alert" role="alert">{{ store.error }}</p><footer><button :disabled="store.busy" @click="deleting = null; deleteDialog?.close()">Cancel</button><button :disabled="store.busy" @click="remove">Delete</button></footer></dialog>
  </div>
</template>

<style scoped>
.qp-page { box-sizing: border-box; padding: 16px 20px; height: 100%; min-height: 0; overflow: hidden; container-type: size; container-name: portfolio; display: flex; flex-direction: column; gap: 12px; }
.qp-page > .qp-heading, .qp-page > .qp-stats, .qp-page > .qp-tabs { flex-shrink: 0; }
.qp-dashboard { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(240px, .85fr) minmax(0, 2fr); gap: 14px; }
.qp-holdings-panel, .qp-history-panel { min-height: 0; min-width: 0; display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-1); }
.qp-history-panel { flex: 1; }
.qp-holdings-head, .qp-history-head { padding: 12px 14px; flex-shrink: 0; }
.qp-holdings-head h2 { margin: 0; font-size: 13px; font-weight: 600; }
.qp-heading, .qp-holdings-head, .qp-history-head { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; }
.qp-heading h1 { margin: 0; font-size: 20px; font-weight: 600; }
.qp-heading p, .qp-history-head p { margin: 5px 0 0; font-size: 12px; color: var(--text-secondary); }
.qp-actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.qp-page button { padding: 7px 11px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface-2); color: var(--text-primary); font-size: 12px; cursor: pointer; }
.qp-page button:hover { background: var(--surface-3); }
.qp-page button:disabled { opacity: .45; cursor: default; }
.qp-stats { display: grid; grid-template-columns: repeat(5, minmax(130px, 1fr)); gap: 10px; overflow-x: auto; }
.qp-stats > div { display: flex; flex-direction: column; gap: 5px; padding: 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-1); min-width: 0; }
.qp-stats span { font-size: 10px; color: var(--text-secondary); text-transform: uppercase; letter-spacing: .04em; }
.qp-stats strong { font-size: 19px; font-weight: 600; font-variant-numeric: tabular-nums; white-space: nowrap; }
.qp-stats small { font-size: 10px; color: var(--text-secondary); }
.qp-tabs { display: flex; gap: 4px; border-bottom: 1px solid var(--border); padding-bottom: 8px; }
.qp-tabs button { background: transparent; border-color: transparent; }
.qp-tabs button[aria-pressed='true'] { background: var(--surface-2); border-color: var(--border); }
.qp-tabs span { color: var(--text-secondary); margin-left: 6px; }
.qp-page label { font-size: 11px; color: var(--text-secondary); }
.qp-page input, .qp-page select { min-width: 0; padding: 7px 9px; color: var(--text-primary); background: var(--surface-1); border: 1px solid var(--border); border-radius: 5px; font-size: 12px; }
.qp-search { display: flex; align-items: center; gap: 8px; flex: 1; max-width: 200px; }
.qp-search input { width: 100%; }
.qp-checkbox { display: flex; align-items: center; gap: 7px; }
.qp-history-head label { display: flex; align-items: center; gap: 6px; }
.qp-table-wrap { flex: 1; min-height: 0; overflow: auto; overscroll-behavior: contain; border-top: 1px solid var(--border); }
.qp-table-wrap:focus-visible { outline: 1px solid var(--text-secondary); outline-offset: -2px; }
.qp-table { width: 100%; border-collapse: collapse; font-size: 12px; font-variant-numeric: tabular-nums; }
.qp-table th, .qp-table td { padding: 12px 10px; text-align: right; white-space: nowrap; border-bottom: 1px solid var(--border); }
.qp-table th { position: sticky; top: 0; z-index: 1; font-size: 10px; color: var(--text-secondary); font-weight: 500; background: var(--surface-1); }
.qp-table th:first-child, .qp-table td:first-child, .qp-table td:last-child { text-align: left; }
.qp-table tr:last-child td { border-bottom: 0; }
.qp-table small { display: block; font-size: 10px; color: var(--text-secondary); margin-top: 4px; }
.qp-asset { display: flex; align-items: center; gap: 8px; }
.qp-asset i { width: 9px; height: 9px; border-radius: 3px; flex-shrink: 0; }
.qp-asset strong { font-size: 12px; font-weight: 600; }
.qp-asset span { min-width: 0; }
.qp-asset small { max-width: 180px; overflow: hidden; text-overflow: ellipsis; }
.qp-page .qp-holding-note { display: block; max-width: 180px; padding: 0; margin-top: 5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; font-size: 10px; color: var(--text-secondary); background: transparent; border: 0; border-radius: 0; }
.qp-page .qp-holding-note:hover { color: var(--text-primary); text-decoration: underline; }
.qp-row-actions { display: flex; flex-wrap: nowrap; gap: 4px; }
.qp-row-actions button { padding: 4px 6px; font-size: 10px; background: transparent; border-color: transparent; }
.qp-table .qp-no-rows { text-align: center; padding: 28px 12px; color: var(--text-secondary); white-space: normal; }
.qp-kind { text-transform: capitalize; font-size: 10px; padding: 3px 6px; border: 1px solid var(--border); border-radius: 4px; }
.qp-kind.buy { color: var(--positive); }.qp-kind.sell { color: var(--negative); }
.qp-table .qp-note { white-space: normal; min-width: 100px; max-width: 240px; overflow-wrap: anywhere; text-align: left; }
.qp-empty { flex: 1; min-height: 0; overflow: auto; text-align: center; padding: 30px 20px; color: var(--text-secondary); }
.qp-empty h2 { font-size: 17px; font-weight: 600; color: var(--text-primary); }
.qp-empty p { max-width: 490px; margin: 12px auto 20px; font-size: 13px; line-height: 1.7; }
.qp-alert { flex-shrink: 0; max-height: 56px; overflow: auto; padding: 8px 12px; margin: 0; border-left: 3px solid var(--warning); background: var(--surface-1); color: var(--text-secondary); font-size: 12px; overflow-wrap: anywhere; }
.qp-caption { flex-shrink: 0; font-size: 10px; color: var(--text-secondary); line-height: 1.4; margin: 0; }
.qp-page .is-up { color: var(--positive); }.qp-page .is-down { color: var(--negative); }
.qp-delete { width: min(450px, calc(100% - 32px)); padding: 24px; margin: auto; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-1); color: var(--text-primary); }
.qp-delete::backdrop { background: #0009; }
.qp-delete h2 { font-size: 17px; margin: 0 0 12px; }.qp-delete p { font-size: 12px; line-height: 1.6; margin-bottom: 16px; }
.qp-delete footer { display: flex; justify-content: flex-end; gap: 8px; }
@container portfolio (max-width: 660px) { .qp-dashboard { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, .8fr) minmax(0, 1.2fr); } .qp-heading p { display: none; } .qp-heading h1 { font-size: 17px; } .qp-heading .qp-actions { gap: 4px; } .qp-heading button { padding: 6px 8px; } }
@container portfolio (max-height: 620px) { .qp-stats > div { padding: 8px 10px; gap: 3px; } .qp-stats strong { font-size: 17px; } .qp-stats small { display: none; } .qp-heading p, .qp-caption { display: none; } .qp-holdings-head, .qp-history-head { padding: 8px 10px; gap: 8px; } .qp-history-head p { display: none; } }
</style>
