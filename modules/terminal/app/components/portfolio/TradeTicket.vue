<script setup lang="ts">
import { usePortfolioStore } from '#terminal/stores/portfolio'
import type { PortfolioAsset } from '#terminal/types/portfolio'
import { calculateCash, calculateHoldings, portfolioTradeAmounts, portfolioMoney as money, portfolioNumber as number } from '#terminal/utils/portfolio'

const props = defineProps<{ assetId?: string; initialTab?: 'buy' | 'sell' | 'edit'; focusNotes?: boolean }>()
const emit = defineEmits<{ close: []; saved: [assetId: string] }>()
const store = usePortfolioStore()
const dialog = ref<HTMLDialogElement | null>(null)
const amountField = ref<HTMLInputElement | null>(null)
const searchField = ref<HTMLInputElement | null>(null)
let revision = store.document.revision
const editing = ref(props.initialTab === 'edit')
const side = ref<'buy' | 'sell'>(props.initialTab === 'sell' ? 'sell' : 'buy')
const mode = ref<'usd' | 'units'>(side.value === 'sell' ? 'units' : 'usd')
const assetId = ref(props.assetId ?? '')
const newAsset = ref<PortfolioAsset | null>(null)
const manual = ref(false)
const picker = ref(!props.assetId)
const search = ref('')
const coins = ref<typeof store.marketCoins>([])
const marketPage = ref(0)
const error = ref('')
const funding = ref(false)
const deposit = ref('')
const localTime = (value: string) => { const d = new Date(value); return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16) }
const initialDate = localTime(new Date().toISOString())
const draft = reactive({ amount: '', price: '', fees: '0', date: initialDate, notes: '', symbol: '', name: '' })
const selected = computed(() => store.document.assets.find(a => a.id === assetId.value) ?? newAsset.value)
const symbol = computed(() => manual.value ? draft.symbol.trim().toUpperCase() : selected.value?.symbol ?? '')
const customDate = computed(() => draft.date !== initialDate)
const asOf = computed(() => customDate.value ? Date.parse(draft.date) : Infinity)
const holding = computed(() => calculateHoldings(store.document, undefined, asOf.value).find(h => h.asset.id === assetId.value))
const availableCash = computed(() => calculateCash(store.document, asOf.value))
const amount = computed(() => [draft.amount, draft.price, draft.fees].some(v => !String(v).trim()) ? null : portfolioTradeAmounts(side.value, mode.value, Number(draft.amount), Number(draft.price), Number(draft.fees)))
const cashAfter = computed(() => {
  if (!amount.value) return null
  const value = availableCash.value + (side.value === 'buy' ? -amount.value.cash : amount.value.cash)
  return value < 0 && value >= -1e-8 ? 0 : value
})
const insufficient = computed(() => cashAfter.value !== null && cashAfter.value < -1e-8)
const oversell = computed(() => side.value === 'sell' && amount.value && amount.value.quantity > (holding.value?.quantity ?? 0) * (1 + 1e-10))
const busy = computed(() => store.busy || store.loading)
const ready = computed(() => selected.value && amount.value && !insufficient.value && !oversell.value && (!manual.value || (symbol.value && draft.name.trim())))
const matches = (symbol: string, name: string) => `${symbol} ${name}`.toLowerCase().includes(search.value.trim().toLowerCase())
const existing = computed(() => store.activeHoldings.filter(h => matches(h.asset.symbol, h.asset.name)))
const marketResults = computed(() => coins.value.filter(c => matches(c.symbol, c.name) && !store.activeHoldings.some(h => h.asset.source === 'market' && h.asset.symbol === c.symbol && h.asset.marketName === c.name)).slice(0, 12))
const entryDate = () => customDate.value ? new Date(draft.date).toISOString() : new Date().toISOString()
async function loadMore() {
  if (store.marketLoading) return
  const page = marketPage.value + 1
  await store.loadMarket(page)
  if (store.quoteError) return
  for (const coin of store.marketCoins) if (!coins.value.some(c => c.symbol === coin.symbol && c.name === coin.name)) coins.value.push(coin)
  marketPage.value = page
}
async function focusAmount() { await nextTick(); if (!editing.value) amountField.value?.focus() }
function chooseExisting(id: string) {
  assetId.value = id; newAsset.value = null; manual.value = false; picker.value = false; draft.amount = ''; error.value = ''
  const price = store.holdings.find(h => h.asset.id === id)?.price
  draft.price = price === null || price === undefined ? '' : String(price)
  void focusAmount()
}
function chooseCoin(coin: typeof store.marketCoins[number]) {
  assetId.value = ''; manual.value = false; picker.value = false; draft.amount = ''; draft.price = String(coin.price); error.value = ''
  newAsset.value = { id: crypto.randomUUID(), symbol: coin.symbol, name: coin.name, source: 'market', marketName: coin.name, marketPage: coin.page, manualPrice: null, updatedAt: new Date().toISOString() }
  void focusAmount()
}
function chooseManual() {
  assetId.value = ''; manual.value = true; picker.value = false; draft.amount = ''; draft.price = ''; draft.symbol = search.value.trim().toUpperCase().slice(0, 30); draft.name = ''; error.value = ''
  newAsset.value = { id: crypto.randomUUID(), symbol: '', name: '', source: 'manual', marketName: '', marketPage: 1, manualPrice: null, updatedAt: new Date().toISOString() }
}
function changeSide(value: 'buy' | 'sell') {
  if (value !== side.value) { side.value = value; mode.value = value === 'buy' ? 'usd' : 'units'; draft.amount = ''; funding.value = false; error.value = '' }
  editing.value = false
  void focusAmount()
}
function changeMode(value: 'usd' | 'units') {
  if (value === mode.value) return
  draft.amount = amount.value ? String(value === 'usd' ? amount.value.cash : amount.value.quantity) : ''
  mode.value = value
}
function portion(fraction: number) {
  mode.value = side.value === 'sell' ? 'units' : 'usd'
  draft.amount = String((side.value === 'sell' ? holding.value?.quantity ?? 0 : availableCash.value) * fraction)
  void focusAmount()
}
async function changeAsset() {
  picker.value = true; editing.value = false; search.value = ''; funding.value = false
  await nextTick(); searchField.value?.focus()
  if (!marketPage.value && side.value === 'buy') void loadMore()
}
function showFunding() { funding.value = !funding.value; deposit.value = insufficient.value ? String(Math.ceil(-cashAfter.value! * 100) / 100) : '' }
async function addCash() {
  const value = Number(deposit.value)
  if (busy.value) return
  if (!String(deposit.value).trim() || !Number.isFinite(value) || value <= 0) { error.value = 'Enter a positive deposit amount.'; return }
  error.value = ''
  try {
    if (await store.saveCashEntry({ id: crypto.randomUUID(), kind: 'deposit', date: entryDate(), amount: value, notes: 'Deposit before purchase' }, revision)) {
      revision = store.document.revision; funding.value = false
    }
  } catch (e) { error.value = String(e) }
}
async function submit() {
  if (busy.value || editing.value || picker.value || funding.value || !ready.value || !amount.value || !selected.value) return
  error.value = ''
  try {
    const date = entryDate(), price = Number(draft.price), fees = Number(draft.fees), notes = draft.notes.trim()
    const asset = manual.value ? { ...selected.value, symbol: symbol.value, name: draft.name.trim(), manualPrice: price } : selected.value
    const saved = newAsset.value
      ? await store.saveAsset(asset, { kind: 'buy', quantity: amount.value.quantity, price, fees, date, notes }, revision)
      : await store.saveTrade({ id: crypto.randomUUID(), assetId: asset.id, kind: side.value, quantity: amount.value.quantity, price, fees, date, notes, settlesCash: true }, revision)
    if (saved) { emit('saved', asset.id); emit('close'); void store.refreshPrices() }
  } catch (e) { error.value = e instanceof Error ? e.message : String(e) }
}
onMounted(() => {
  store.error = ''; dialog.value?.showModal()
  if (props.assetId) chooseExisting(props.assetId)
  else { searchField.value?.focus(); if (side.value === 'buy') void loadMore() }
})
</script>

<template>
  <dialog ref="dialog" class="qp-ticket" :class="{ 'qp-ticket--editing': editing }" aria-labelledby="qp-ticket-title" @cancel="busy ? $event.preventDefault() : emit('close')">
    <header><div><h2 id="qp-ticket-title">{{ picker ? 'Add asset' : `Manage ${symbol || 'asset'}` }}</h2><small>{{ picker ? 'Choose an asset for your portfolio' : manual ? 'New asset' : selected?.name }}</small></div><button v-if="!picker && !props.assetId" type="button" :disabled="busy" @click="changeAsset">Change</button></header>
    <div v-if="!picker" class="qp-ticket-sides" :class="{ 'qp-ticket-sides--manage': !newAsset }" role="group" aria-label="Manage holding">
      <button type="button" :disabled="busy" :aria-pressed="!editing && side === 'buy'" @click="changeSide('buy')">Buy</button>
      <button type="button" :disabled="busy || !!newAsset || !holding?.quantity" :aria-pressed="!editing && side === 'sell'" @click="changeSide('sell')">Sell</button>
      <button v-if="!newAsset" type="button" :disabled="busy" :aria-pressed="editing" @click="editing = true">Edit holding</button>
    </div>
    <TerminalPortfolioEditor v-if="editing && assetId" kind="asset" :asset-id="assetId" :focus-notes="focusNotes" embedded @saved="emit('saved', $event)" @close="emit('close')" />
    <form v-show="!editing" @submit.prevent="submit">
      <fieldset :disabled="busy">
        <template v-if="picker">
          <label>Find an asset<input ref="searchField" v-model="search" type="search" placeholder="Name or symbol…" /></label>
          <div class="qp-ticket-results">
            <h3 v-if="existing.length">Current holdings</h3>
            <button v-for="h in existing" :key="h.asset.id" type="button" :aria-label="`Choose ${h.asset.symbol} holding`" @click="chooseExisting(h.asset.id)"><span><strong>{{ h.asset.symbol }}</strong><small>{{ h.asset.name }}</small></span><small>{{ number(h.quantity) }} held</small></button>
            <h3 v-if="side === 'buy'">Crypto</h3>
            <button v-for="coin in side === 'buy' ? marketResults : []" :key="`${coin.symbol}:${coin.name}`" type="button" :aria-label="`Choose ${coin.name}`" @click="chooseCoin(coin)"><span><strong>{{ coin.symbol }}</strong><small>{{ coin.name }}</small></span><small>{{ money(coin.price) }}</small></button>
            <p v-if="!existing.length && !marketResults.length && !store.marketLoading">No matching assets in the loaded list.</p>
          </div>
          <p v-if="store.quoteError" role="status">{{ store.quoteError }}</p>
          <div v-if="side === 'buy'" class="qp-ticket-picker-actions"><button type="button" :disabled="store.marketLoading || marketPage >= 100" @click="loadMore">{{ store.marketLoading ? 'Loading…' : store.quoteError ? 'Retry prices' : 'Load more crypto' }}</button><button type="button" @click="chooseManual">Add another asset…</button></div>
        </template>
        <template v-else>
          <div v-if="manual" class="qp-ticket-grid"><label>Symbol<input v-model="draft.symbol" required maxlength="30" placeholder="AAPL" /></label><label>Name<input v-model="draft.name" required maxlength="150" placeholder="Apple" /></label></div>
          <div class="qp-ticket-amount-head"><label for="qp-ticket-amount">{{ mode === 'units' ? 'Quantity' : side === 'buy' ? 'Spend (USD)' : 'Receive (USD)' }}</label><div><button type="button" :aria-pressed="mode === 'usd'" @click="changeMode('usd')">USD</button><button type="button" :aria-pressed="mode === 'units'" @click="changeMode('units')">Units</button></div></div>
          <input id="qp-ticket-amount" ref="amountField" v-model="draft.amount" class="qp-ticket-amount" type="number" min="0" step="any" required placeholder="0" />
          <div class="qp-ticket-portions"><span>{{ side === 'buy' ? `${money(availableCash)} available` : `${number(holding?.quantity ?? 0)} ${symbol} available` }}</span><button v-for="part in [0.25, 0.5, 1]" :key="part" type="button" @click="portion(part)">{{ part === 1 ? side === 'sell' ? 'Sell all' : 'Max' : `${part * 100}%` }}</button></div>
          <label>Price per unit (USD)<input v-model="draft.price" type="number" min="0" step="any" required placeholder="Enter price" :title="manual ? 'Your trade price per unit' : 'Prefilled from the latest stored price. Change it to your trade price.'" /></label>
          <div class="qp-ticket-preview" aria-live="polite"><div><span>{{ side === 'buy' ? 'You buy' : 'You sell' }}</span><strong>{{ amount ? number(amount.quantity) : '—' }} {{ symbol }}</strong></div><div><span>{{ side === 'buy' ? 'Total paid' : 'Net received' }}</span><strong>{{ money(amount?.cash ?? null) }}</strong></div><div><span>{{ customDate ? 'Cash after this entry' : 'Cash remaining' }}</span><strong :class="{ 'qp-ticket-error': insufficient }">{{ money(cashAfter) }}</strong></div></div>
          <p v-if="oversell" class="qp-ticket-error" role="status">You only hold {{ number(holding?.quantity ?? 0) }} {{ symbol }}. Use Sell all to close the position.</p>
          <p v-if="!amount && String(draft.amount).trim() && String(draft.price).trim()" class="qp-ticket-error" role="status">Enter a positive amount and a valid price. A USD purchase must cover its fees.</p>
          <div v-if="side === 'buy'" class="qp-ticket-funding"><p v-if="insufficient" class="qp-ticket-error">Add {{ money(-cashAfter!) }} cash to cover this purchase.</p><button type="button" @click="showFunding">{{ funding ? 'Cancel deposit' : 'Add cash' }}</button><div v-if="funding"><label>Deposit amount (USD)<input v-model="deposit" type="number" min="0" step="any" @keydown.enter.prevent="addCash" /></label><p>This saves a cash deposit{{ customDate ? ' at the selected trade date' : '' }}. Your trade stays open.</p><button type="button" @click="addCash">Save deposit</button></div></div>
          <details class="qp-ticket-details"><summary>Fees, date &amp; notes</summary><div class="qp-ticket-grid"><label>Fees (USD)<input v-model="draft.fees" type="number" min="0" step="any" required /></label><label>Date and time<input v-model="draft.date" type="datetime-local" required /></label></div><label>Trade notes<textarea v-model="draft.notes" rows="2" maxlength="2000" /></label></details>
        </template>
      </fieldset>
      <p v-if="error || store.error" class="qp-ticket-error" role="alert">{{ error || store.error }}</p>
      <footer><button type="button" :disabled="busy" @click="emit('close')">Cancel</button><button v-if="!picker" type="submit" class="qp-ticket-save" :disabled="busy || funding || !ready">{{ store.busy ? 'Saving…' : side === 'buy' ? `Save buy${symbol ? ` · ${symbol}` : ''}` : `Save sell${symbol ? ` · ${symbol}` : ''}` }}</button></footer>
    </form>
  </dialog>
</template>

<style scoped>
.qp-ticket { width: min(440px, calc(100% - 32px)); max-height: calc(100% - 32px); margin: auto; padding: 22px; border: 1px solid var(--border); border-radius: 12px; color: var(--text-primary); background: var(--surface-1); overflow: auto; }
.qp-ticket--editing { width: min(580px, calc(100% - 32px)); }
.qp-ticket > header { margin-bottom: 14px; }
.qp-ticket > .qp-ticket-sides { margin-bottom: 14px; }
.qp-ticket .qp-ticket-sides--manage { grid-template-columns: 1fr 1fr 1.4fr; }
.qp-ticket::backdrop { background: #0009; }.qp-ticket form, .qp-ticket fieldset { display: flex; flex-direction: column; gap: 10px; }.qp-ticket fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
.qp-ticket header, .qp-ticket footer, .qp-ticket-amount-head, .qp-ticket-portions, .qp-ticket-preview > div, .qp-ticket-picker-actions { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
.qp-ticket h2 { margin: 0; font-size: 19px; font-weight: 600; overflow-wrap: anywhere; }.qp-ticket header > div { min-width: 0; overflow-wrap: anywhere; }.qp-ticket header button { flex-shrink: 0; }
.qp-ticket h3 { font-size: 11px; font-weight: 500; color: var(--text-secondary); margin: 8px 0; }
.qp-ticket label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--text-secondary); }
.qp-ticket input, .qp-ticket textarea { width: 100%; min-width: 0; padding: 9px 10px; color: var(--text-primary); background: var(--surface-2); border: 1px solid var(--border); border-radius: 6px; }
.qp-ticket button { padding: 7px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface-2); color: var(--text-primary); cursor: pointer; font-size: 12px; }.qp-ticket button:hover { background: var(--surface-3); }.qp-ticket button:disabled { opacity: .45; cursor: default; }
.qp-ticket small, .qp-ticket p { font-size: 10px; color: var(--text-secondary); line-height: 1.5; margin: 0; }.qp-ticket small { display: block; }
.qp-ticket-sides, .qp-ticket-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }.qp-ticket-sides button[aria-pressed='true'] { border-color: var(--accent); background: var(--surface-3); font-weight: 600; }
.qp-ticket-amount-head { margin-bottom: -8px; }.qp-ticket-amount-head button { padding: 3px 7px; font-size: 10px; background: transparent; border-color: transparent; }.qp-ticket-amount-head button[aria-pressed='true'] { background: var(--surface-3); }
.qp-ticket .qp-ticket-amount { font-size: 26px; padding: 12px; font-variant-numeric: tabular-nums; }.qp-ticket-portions { gap: 4px; margin-top: -6px; }.qp-ticket-portions span { flex: 1; font-size: 10px; color: var(--text-secondary); min-width: 0; overflow-wrap: anywhere; }.qp-ticket-portions button { padding: 4px 7px; font-size: 10px; }
.qp-ticket-preview { display: flex; flex-direction: column; gap: 8px; background: var(--surface-2); border-radius: 8px; padding: 12px; font-size: 11px; }.qp-ticket-preview span { color: var(--text-secondary); }.qp-ticket-preview strong { font-weight: 500; font-variant-numeric: tabular-nums; text-align: right; overflow-wrap: anywhere; min-width: 0; }
.qp-ticket-details summary { cursor: pointer; font-size: 11px; color: var(--text-secondary); }.qp-ticket-details > div, .qp-ticket-details > label { margin-top: 12px; }
.qp-ticket-funding > button { padding: 0; background: transparent; border: 0; font-size: 10px; color: var(--text-secondary); }.qp-ticket-funding > div { display: flex; flex-direction: column; gap: 8px; padding: 12px; margin-top: 8px; border: 1px solid var(--border); border-radius: 6px; }
.qp-ticket .qp-ticket-error { color: var(--negative); overflow-wrap: anywhere; }.qp-ticket-results { max-height: 300px; overflow-y: auto; }.qp-ticket-results > button { display: flex; justify-content: space-between; align-items: center; gap: 12px; width: 100%; padding: 9px 8px; text-align: left; background: transparent; border: 0; }.qp-ticket-results span { min-width: 0; overflow-wrap: anywhere; }.qp-ticket-results strong { font-weight: 500; }.qp-ticket-picker-actions button { font-size: 10px; }
.qp-ticket footer { position: sticky; bottom: -22px; background: var(--surface-1); border-top: 1px solid var(--border); padding: 12px 0 0; justify-content: flex-end; }.qp-ticket .qp-ticket-save { flex: 1; background: var(--accent); border-color: var(--accent); color: var(--qss-accent-ink, white); }
.qp-ticket button.qp-ticket-save:hover { background: var(--accent-hover, var(--accent)); }
</style>
