<script setup lang="ts">
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { usePortfolioStore } from '#terminal/stores/portfolio'
import { usePortfolioPanel } from '#terminal/composables/usePortfolioPanel'
import { RotateCw, Plus } from 'lucide-vue-next'
import type { PortfolioTrade } from '#terminal/types/portfolio'
import { assetColor, portfolioCashColor, portfolioMoney as money, portfolioNumber as number, portfolioPrice as price } from '#terminal/utils/portfolio'

definePageMeta({ layout: 'terminal' })
const store = usePortfolioStore()
const { view, editor, deleting, selectedHolding, selectedAsset, selectedKind } = usePortfolioPanel()
const manageAsset = computed(() => editor.value && ((editor.value.kind === 'trade' && !editor.value.tradeId) || (editor.value.kind === 'asset' && !!editor.value.assetId)))
const currentOnly = ref(true)
const search = ref('')
const deleteDialog = ref<HTMLDialogElement | null>(null)
const rows = computed(() => store.holdings
  .filter((h) => (!currentOnly.value || h.quantity > 0) && `${h.asset.symbol} ${h.asset.name}`.toLowerCase().includes(search.value.toLowerCase()))
  .sort((a, b) => Number(b.quantity > 0) - Number(a.quantity > 0) || (b.value ?? 0) - (a.value ?? 0) || a.asset.symbol.localeCompare(b.asset.symbol)))
const assetName = (id: string) => store.document.assets.find((a) => a.id === id)?.symbol ?? 'Unknown asset'
const trades = computed(() => [...store.document.trades]
  .filter((t) => (!selectedAsset.value || t.assetId === selectedAsset.value) && (!selectedKind.value || t.kind === selectedKind.value))
  .sort((a, b) => Date.parse(b.date) - Date.parse(a.date) || b.order - a.order))
const cashRows = computed(() => [...store.cashHistory].reverse())
const signed = (value: number | null) => value === null ? '—' : `${value > 0 ? '+' : ''}${money(value)}`
const tone = (value: number | null) => value === null || value === 0 ? '' : value > 0 ? 'is-up' : 'is-down'
const percent = (value: number, cost: number) => cost > 0 ? `${value >= 0 ? '+' : ''}${(value / cost * 100).toFixed(2)}%` : '—'
const displayDate = (value: string) => new Date(value).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
const tradeValue = (trade: PortfolioTrade) => trade.kind === 'adjustment' ? null : trade.quantity * trade.price + (trade.kind === 'sell' ? -trade.fees : trade.fees)
const locked = computed(() => !store.loaded || store.busy || store.loading)
watch([editor, deleting], ([edit, removal]) => { if (edit || removal) store.discardPendingPrices() }, { flush: 'sync', immediate: true })
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
function askDelete(kind: 'asset' | 'trade' | 'cash', id: string, name: string) {
  store.error = ''
  deleting.value = { kind, id, name }
}
watch(deleting, async (target) => {
  if (!target) return
  await nextTick()
  deleteDialog.value?.showModal()
})
function addAsset() { editor.value = { kind: 'trade' } }
function selectSavedHolding(id: string) { selectedHolding.value = store.activeHoldings.some(h => h.asset.id === id) ? id : '' }
async function remove() {
  if (!deleting.value || store.busy) return
  const target = deleting.value
  const ok = target.kind === 'asset' ? await store.removeAsset(target.id) : target.kind === 'cash' ? await store.removeCashEntry(target.id) : await store.removeTrade(target.id)
  if (ok) { deleting.value = null; deleteDialog.value?.close() }
}
watch(() => store.activeHoldings, (holdings) => {
  if (selectedHolding.value && !holdings.some(h => h.asset.id === selectedHolding.value)) selectedHolding.value = ''
})
</script>

<template>
  <div class="qp-page">
    <header class="qp-heading"><div><h1>Portfolio</h1><p>Your investments and available cash · USD</p></div><div class="qp-actions"><button :disabled="locked" @click="editor = { kind: 'asset' }">Add existing holding</button><button class="qp-primary" :disabled="locked" @click="addAsset"><Plus :size="14" />Add asset</button></div></header>
    <div v-if="store.error && !editor && !deleting" class="qp-alert" role="alert">{{ store.error }} <button :disabled="store.busy || store.loading" @click="store.load()">Reload</button></div>
    <p v-if="store.loading && !store.loaded" class="qp-empty" role="status">Loading portfolio…</p>
    <template v-else-if="store.loaded">
      <div class="qp-stats">
        <div><span>{{ store.totals.missing ? 'Priced portfolio value' : 'Portfolio value' }}</span><strong>{{ money(store.totals.value) }}</strong><small>{{ store.totals.count }} {{ store.totals.count === 1 ? 'investment' : 'investments' }} · {{ money(store.totals.cost) }} cost basis</small></div>
        <div><span>Unrealized gain</span><strong :class="tone(store.totals.unrealized)">{{ signed(store.totals.unrealized) }}</strong><small>{{ store.totals.unrealized === null ? 'Current prices missing' : percent(store.totals.unrealized, store.totals.cost) }}</small></div>
        <div><span>Total gain</span><strong :class="tone(store.totals.totalGain)">{{ signed(store.totals.totalGain) }}</strong><small>{{ signed(store.totals.realized) }} realized · after fees</small></div>
      </div>
      <p v-if="store.quoteError" class="qp-alert" role="status">{{ store.quoteError }}</p>
      <p v-if="store.totals.missing" class="qp-alert" role="status">{{ store.totals.missing }} holding(s) have no current price. They are excluded from the priced value and pie; total gains stay unavailable until priced.</p>
      <nav class="qp-tabs" aria-label="Portfolio views"><button :aria-pressed="view === 'holdings'" @click="view = 'holdings'">Holdings</button><button :aria-pressed="view === 'history'" @click="view = 'history'">Trade history <span>{{ store.document.trades.length }}</span></button><button :aria-pressed="view === 'cash'" @click="view = 'cash'">Cash history</button></nav>
      <template v-if="view === 'holdings'">
        <div class="qp-dashboard">
          <TerminalPortfolioCharts :holdings="store.holdings" :cash="store.cash" :history="store.history" @select="selectedHolding = $event" @cash="!locked && (editor = { kind: 'cash' })" />
          <section class="qp-holdings-panel" aria-label="Holdings panel">
          <div class="qp-holdings-head"><h2>Holdings <span>{{ rows.length }}</span></h2><label class="qp-search"><span class="sr-only">Find a holding</span><input v-model="search" type="search" placeholder="Find a holding…" /></label><label class="qp-checkbox"><input v-model="currentOnly" type="checkbox" /> Current holdings only</label><button class="qp-refresh" :disabled="locked || store.refreshing || !store.activeHoldings.some(h => h.asset.source === 'market')" :aria-label="store.refreshing ? 'Refreshing prices' : 'Refresh prices'" title="Refresh market prices" @click="store.refreshPrices()"><RotateCw :size="14" /></button></div>
          <div class="qp-table-wrap qp-holdings-scroll" :class="{ 'qp-holdings-scroll--empty': !rows.length }" tabindex="0" role="region" aria-label="Scrollable holdings"><table class="qp-table qp-holdings-table" aria-label="Portfolio holdings"><colgroup><col style="width: 20%" /><col style="width: 9%" /><col style="width: 12%" /><col style="width: 12%" /><col style="width: 14%" /><col style="width: 14%" /><col style="width: 19%" /></colgroup><thead><tr><th>Asset</th><th>Quantity</th><th>Avg. cost</th><th>Current price</th><th>Value / share</th><th>Unrealized</th><th><span class="sr-only">Actions</span></th></tr></thead><tbody>
            <tr class="qp-cash-row"><td><div class="qp-asset"><i :style="{ background: portfolioCashColor }" /><span><strong>Cash</strong><small>USD · Available to invest</small></span></div></td><td>—</td><td>—</td><td>—</td><td>{{ money(store.cash) }}<small>{{ store.totals.value > 0 ? `${(store.cash / store.totals.value * 100).toFixed(1)}%` : '—' }}</small></td><td>—</td><td><div class="qp-row-actions"><button :disabled="locked" aria-label="Manage cash" @click="editor = { kind: 'cash', cashAction: 'balance' }">Manage</button></div></td></tr>
            <tr v-for="h in rows" :key="h.asset.id" class="qp-holding-row" :class="{ 'is-selected': selectedHolding === h.asset.id }" @click="selectedHolding = h.asset.id">
              <td><div class="qp-asset"><i :style="{ background: assetColor(h.asset.id) }" /><span><button class="qp-asset-select" :aria-label="`View ${h.asset.symbol} details`" :aria-pressed="selectedHolding === h.asset.id" @click="selectedHolding = h.asset.id">{{ h.asset.symbol }}</button><small :title="h.asset.name">{{ h.asset.name }}{{ h.quantity === 0 ? ' · Closed' : '' }}</small></span></div></td>
              <td :title="number(h.quantity)">{{ number(h.quantity) }}</td><td :title="price(h.averageCost)">{{ price(h.averageCost) }}</td><td :title="h.asset.source === 'manual' ? displayDate(h.asset.updatedAt) : store.document.quotes[h.asset.id] ? `Checked ${displayDate(store.document.quotes[h.asset.id]!.checkedAt)}` : 'Not priced yet'">{{ price(h.price) }}<small>{{ h.asset.source === 'manual' ? 'Manual' : 'Market' }}</small></td><td :title="money(h.value)">{{ money(h.value) }}<small>{{ h.value !== null && store.totals.value > 0 ? `${(h.value / store.totals.value * 100).toFixed(1)}%` : '—' }}</small></td><td :class="tone(h.unrealized)" :title="signed(h.unrealized)">{{ signed(h.unrealized) }}<small>{{ h.unrealized === null ? '—' : percent(h.unrealized, h.cost) }}</small></td><td><div class="qp-row-actions"><button :disabled="locked" :aria-label="`Manage ${h.asset.symbol}`" @click="editor = { kind: 'trade', assetId: h.asset.id }">Manage</button></div></td>
            </tr>
          </tbody></table></div>
          <div v-if="!rows.length" class="qp-holdings-empty"><div class="qp-no-rows"><template v-if="!store.document.assets.length"><strong>Start with what you own</strong><p>Add your existing investments, then set your available cash.<br />For a new purchase, choose Add asset.</p><button class="qp-primary" :disabled="locked" @click="editor = { kind: 'asset' }">Add your first holding</button><button :disabled="locked" @click="editor = { kind: 'cash', cashAction: 'balance' }">Set available cash</button></template><template v-else-if="currentOnly && !search.trim() && !store.activeHoldings.length"><strong>No active holdings</strong><p>Your completed trades remain in history.</p><button class="qp-primary" :disabled="locked" @click="addAsset">Add an asset</button></template><template v-else><strong>No matching holdings</strong><p>Try another search or include closed positions.</p><button @click="search = ''; currentOnly = false">Show all holdings</button></template></div></div>
          </section>
        </div>
      </template>
      <section v-else-if="view === 'history'" class="qp-history-panel" aria-label="Trade history panel">
        <div class="qp-history-head"><p>Opening balances, trades and position adjustments remain editable. Corrections recalculate all later holdings and gains.</p><div class="qp-actions"><label>Asset<select v-model="selectedAsset"><option value="">All assets</option><option v-for="a in store.document.assets" :key="a.id" :value="a.id">{{ a.symbol }} · {{ a.name }}</option></select></label><label>Entry type<select v-model="selectedKind"><option value="">All entries</option><option value="opening">Opening</option><option value="buy">Buy</option><option value="sell">Sell</option><option value="adjustment">Position adjustment</option></select></label></div></div>
        <div class="qp-table-wrap" tabindex="0" role="region" aria-label="Scrollable trade history"><table class="qp-table" aria-label="Portfolio trade history"><thead><tr><th>Date</th><th>Asset</th><th>Type</th><th>Quantity</th><th>Price</th><th>Fees</th><th>Net amount</th><th>Cash change</th><th>Notes</th><th>Actions</th></tr></thead><tbody>
          <tr v-for="t in trades" :key="t.id"><td>{{ displayDate(t.date) }}</td><td>{{ assetName(t.assetId) }}</td><td><span class="qp-kind" :class="t.kind">{{ t.kind }}</span></td><td>{{ number(t.quantity) }}<small v-if="t.kind === 'adjustment'">Position total</small></td><td>{{ price(t.price) }}<small v-if="t.kind === 'adjustment'">Average cost</small></td><td>{{ t.kind === 'adjustment' ? '—' : money(t.fees) }}</td><td>{{ money(tradeValue(t)) }}</td><td>{{ t.settlesCash ? signed((tradeValue(t) ?? 0) * (t.kind === 'buy' ? -1 : 1)) : '—' }}</td><td class="qp-note">{{ t.notes || '—' }}</td><td><div class="qp-row-actions"><button :disabled="locked" :aria-label="`Edit ${assetName(t.assetId)} ${t.kind}`" @click="editor = { kind: 'trade', tradeId: t.id }">Edit</button><button :disabled="locked" :aria-label="`Delete ${assetName(t.assetId)} ${t.kind}`" @click="askDelete('trade', t.id, `${assetName(t.assetId)} ${t.kind}`)">Delete</button></div></td></tr>
          <tr v-if="!trades.length"><td colspan="10" class="qp-no-rows">No trades match these filters.</td></tr>
        </tbody></table></div>
      </section>
      <section v-else class="qp-history-panel" aria-label="Cash history panel">
        <div class="qp-history-head"><div><strong>Cash · {{ money(store.cash) }}</strong><p>Deposits, withdrawals and trades in one running balance.</p></div><div class="qp-actions"><button :disabled="locked" @click="editor = { kind: 'cash', cashAction: 'deposit' }">Deposit</button><button :disabled="locked" @click="editor = { kind: 'cash', cashAction: 'withdrawal' }">Withdraw</button><button :disabled="locked" @click="editor = { kind: 'cash', cashAction: 'balance' }">Set balance</button></div></div>
        <div class="qp-table-wrap qp-cash-scroll" tabindex="0" role="region" aria-label="Scrollable cash history"><table class="qp-table qp-cash-table" aria-label="Portfolio cash history"><colgroup><col style="width: 20%" /><col style="width: 15%" /><col style="width: 14%" /><col style="width: 14%" /><col style="width: 23%" /><col style="width: 14%" /></colgroup><thead><tr><th>Date</th><th>Type</th><th>Cash change</th><th>Balance after</th><th>Notes</th><th>Actions</th></tr></thead><tbody>
          <tr v-for="c in cashRows" :key="`${c.source}:${c.id}`"><td>{{ displayDate(c.date) }}</td><td><span class="qp-kind">{{ c.kind === 'balance' ? 'Balance set' : c.kind }}{{ c.source === 'trade' ? ` · ${assetName(c.assetId)}` : '' }}</span></td><td :class="tone(c.amount)">{{ signed(c.amount) }}</td><td>{{ money(c.balance) }}</td><td class="qp-note">{{ c.notes || '—' }}</td><td><div class="qp-row-actions"><button :disabled="locked" :aria-label="`Edit cash ${c.kind}`" @click="editor = c.source === 'trade' ? { kind: 'trade', tradeId: c.id } : { kind: 'cash', cashEntryId: c.id }">Edit</button><button :disabled="locked" :aria-label="`Delete cash ${c.kind}`" @click="askDelete(c.source, c.id, c.source === 'trade' ? `${assetName(c.assetId)} ${c.kind}` : `cash ${c.kind}`)">Delete</button></div></td></tr>
          <tr v-if="!cashRows.length"><td colspan="6" class="qp-no-rows">Set your current cash balance or add a deposit to get started.</td></tr>
        </tbody></table></div>
      </section>
    </template>
    <TerminalPortfolioTradeTicket v-if="editor && manageAsset" :key="JSON.stringify(editor)" :asset-id="editor.assetId" :initial-tab="editor.kind === 'asset' ? 'edit' : editor.side" :focus-notes="editor.focusNotes" @saved="selectSavedHolding" @close="editor = null" />
    <TerminalPortfolioEditor v-else-if="editor" :key="JSON.stringify(editor)" v-bind="editor" @saved="selectSavedHolding" @new-asset="editor = { kind: 'trade', side: 'buy' }" @close="editor = null" />
    <dialog ref="deleteDialog" class="qp-delete" aria-labelledby="qp-delete-title" @cancel="store.busy ? $event.preventDefault() : (deleting = null)"><h2 id="qp-delete-title">Delete {{ deleting?.name }}?</h2><p>{{ deleting?.kind === 'asset' ? 'This removes the asset and its trades, undoing their cash movements.' : 'This removes the entry and recalculates cash, holdings and gains.' }} Deletion is rejected if a later entry would exceed the available cash or holdings.</p><p v-if="store.error" class="qp-alert" role="alert">{{ store.error }}</p><footer><button :disabled="store.busy" @click="deleting = null; deleteDialog?.close()">Cancel</button><button :disabled="store.busy" @click="remove">Delete</button></footer></dialog>
  </div>
</template>

<style scoped>
.qp-page { box-sizing: border-box; padding: 16px 20px; height: 100%; min-height: 0; overflow: hidden; container-type: size; container-name: portfolio; display: flex; flex-direction: column; gap: 12px; }
.qp-page > .qp-heading, .qp-page > .qp-stats, .qp-page > .qp-tabs { flex-shrink: 0; }
.qp-dashboard { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(220px, .7fr) minmax(0, 2fr); gap: 14px; }
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
.qp-stats { display: grid; grid-template-columns: repeat(3, minmax(130px, 1fr)); gap: 10px; overflow-x: auto; }
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
.qp-holdings-scroll, .qp-cash-scroll { scrollbar-gutter: stable; }
.qp-holdings-scroll--empty { flex: 0 0 auto; border-bottom: 1px solid var(--border); }
.qp-holdings-empty { display: flex; flex: 1; min-height: 0; overflow: auto; padding: 20px; text-align: center; color: var(--text-secondary); }
.qp-holdings-empty > .qp-no-rows { width: 100%; max-width: 380px; margin: auto; }
.qp-holdings-table { table-layout: fixed; min-width: 800px; }
.qp-cash-table { table-layout: fixed; min-width: 820px; }
.qp-holdings-table td, .qp-cash-table td { overflow: hidden; text-overflow: ellipsis; }
.qp-table th, .qp-table td { padding: 12px 10px; text-align: right; white-space: nowrap; border-bottom: 1px solid var(--border); }
.qp-table th { position: sticky; top: 0; z-index: 1; font-size: 10px; color: var(--text-secondary); font-weight: 500; background: var(--surface-1); }
.qp-holdings-table th { height: 40px; box-sizing: border-box; z-index: 2; }
.qp-cash-row td { position: sticky; top: 40px; z-index: 1; background: var(--surface-1); }
.qp-holdings-table th { z-index: 4; }
.qp-holdings-table th:first-child { left: 0; z-index: 5; }.qp-holdings-table th:last-child { right: 0; z-index: 5; }
.qp-holdings-table td:first-child:not([colspan]), .qp-holdings-table td:last-child:not([colspan]) { position: sticky; z-index: 1; background: var(--surface-1); }
.qp-holdings-table td:first-child:not([colspan]) { left: 0; }.qp-holdings-table td:last-child:not([colspan]) { right: 0; }
.qp-cash-row td { z-index: 2; }.qp-holdings-table .qp-cash-row td:first-child, .qp-holdings-table .qp-cash-row td:last-child { z-index: 3; }
.qp-table th:first-child, .qp-table td:first-child, .qp-table td:last-child { text-align: left; }
.qp-table tr:last-child td { border-bottom: 0; }
.qp-table small { display: block; font-size: 10px; color: var(--text-secondary); margin-top: 4px; }
.qp-asset { display: flex; align-items: center; gap: 8px; }
.qp-asset i { width: 9px; height: 9px; border-radius: 3px; flex-shrink: 0; }
.qp-asset strong { font-size: 12px; font-weight: 600; }
.qp-asset span { min-width: 0; }
.qp-asset small { max-width: 180px; overflow: hidden; text-overflow: ellipsis; }
.qp-holdings-table .qp-asset > span { flex: 1; }
.qp-holdings-table .qp-asset small { max-width: 100%; }
.qp-row-actions { display: flex; flex-wrap: nowrap; gap: 4px; }
.qp-row-actions button { padding: 4px 6px; font-size: 10px; background: transparent; border-color: transparent; }
.qp-holdings-table .qp-row-actions button { padding: 6px 12px; font-size: 11px; background: var(--surface-2); border-color: var(--border); }
.qp-table .qp-no-rows { text-align: center; padding: 28px 12px; color: var(--text-secondary); white-space: normal; }
.qp-kind { text-transform: capitalize; font-size: 10px; padding: 3px 6px; border: 1px solid var(--border); border-radius: 4px; }
.qp-kind.buy { color: var(--positive); }.qp-kind.sell { color: var(--negative); }
.qp-table .qp-note { white-space: normal; min-width: 100px; max-width: 240px; overflow-wrap: anywhere; text-align: left; }
.qp-empty { flex: 1; min-height: 0; overflow: auto; text-align: center; padding: 30px 20px; color: var(--text-secondary); }
.qp-alert { flex-shrink: 0; max-height: 56px; overflow: auto; padding: 8px 12px; margin: 0; border-left: 3px solid var(--warning); background: var(--surface-1); color: var(--text-secondary); font-size: 12px; overflow-wrap: anywhere; }
.qp-page .is-up { color: var(--positive); }.qp-page .is-down { color: var(--negative); }
.qp-delete { width: min(450px, calc(100% - 32px)); padding: 24px; margin: auto; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-1); color: var(--text-primary); }
.qp-delete::backdrop { background: #0009; }
.qp-delete h2 { font-size: 17px; margin: 0 0 12px; }.qp-delete p { font-size: 12px; line-height: 1.6; margin-bottom: 16px; }
.qp-delete footer { display: flex; justify-content: flex-end; gap: 8px; }
@container portfolio (max-width: 660px) { .qp-dashboard { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, .8fr) minmax(0, 1.2fr); } .qp-heading p { display: none; } .qp-heading h1 { font-size: 17px; } .qp-heading .qp-actions { gap: 4px; } .qp-heading button { padding: 6px 8px; } }
@container portfolio (max-height: 620px) { .qp-stats > div { padding: 8px 10px; gap: 3px; } .qp-stats strong { font-size: 17px; } .qp-heading p { display: none; } .qp-holdings-head, .qp-history-head { padding: 8px 10px; gap: 8px; } .qp-history-head p { display: none; } }

.qp-page .qp-primary { display: inline-flex; align-items: center; justify-content: center; gap: 6px; background: var(--accent); border-color: var(--accent); color: var(--qss-accent-ink, white); }
.qp-page .qp-primary:hover { filter: brightness(1.12); }
.qp-stats > div:first-child strong { font-size: 25px; }
.qp-holdings-head h2 span { margin-left: 6px; font-weight: 400; font-size: 11px; color: var(--text-secondary); }
.qp-page .qp-refresh { display: inline-flex; padding: 6px; background: transparent; }
.qp-holding-row { cursor: pointer; }.qp-holdings-table .qp-holding-row:hover td { background: var(--surface-2); }.qp-holdings-table .qp-holding-row.is-selected td { background: var(--surface-2); }
.qp-holding-row.is-selected td:first-child { box-shadow: inset 2px 0 var(--accent); }
.qp-page .qp-asset-select { max-width: 100%; overflow: hidden; text-overflow: ellipsis; padding: 0; border: 0; border-radius: 0; background: transparent; font-weight: 600; text-align: left; }
.qp-no-rows strong { display: block; color: var(--text-primary); font-size: 15px; font-weight: 500; margin: 10px 0; }.qp-no-rows p { line-height: 1.8; margin: 8px 0 18px; font-size: 12px; }.qp-no-rows button { margin: 3px; }
</style>
