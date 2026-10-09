<script setup lang="ts">
import { usePortfolioStore } from '#terminal/stores/portfolio'
import type { PortfolioAsset, PortfolioCashEntry, PortfolioTrade } from '#terminal/types/portfolio'
import { portfolioMoney as money } from '#terminal/utils/portfolio'

const props = defineProps<{ kind: 'asset' | 'trade' | 'cash'; assetId?: string; tradeId?: string; cashEntryId?: string; cashAction?: PortfolioCashEntry['kind']; side?: 'buy' | 'sell'; focusNotes?: boolean }>()
const emit = defineEmits<{ close: [] }>()
const store = usePortfolioStore()
const dialog = ref<HTMLDialogElement | null>(null)
const notesField = ref<HTMLTextAreaElement | null>(null)
const error = ref('')
const revision = store.document.revision
const asset = store.document.assets.find((a) => a.id === props.assetId)
const holding = store.holdings.find((h) => h.asset.id === props.assetId)
const trade = store.document.trades.find((t) => t.id === props.tradeId)
const cashEntry = store.document.cashEntries?.find((c) => c.id === props.cashEntryId)
const initialDate = trade?.date ?? cashEntry?.date ?? new Date().toISOString()
const localTime = (date: string) => {
  const d = new Date(date)
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16)
}
const draft = reactive({
  name: asset?.name ?? '', symbol: asset?.symbol ?? '', source: asset?.source ?? 'manual' as 'manual' | 'market',
  marketName: asset?.marketName ?? '', marketPage: asset?.marketPage ?? 1,
  manualPrice: asset?.manualPrice === null || asset?.manualPrice === undefined ? '' : String(asset.manualPrice),
  assetId: trade?.assetId ?? props.assetId ?? store.document.assets[0]?.id ?? '',
  kind: trade?.kind ?? props.side ?? 'buy' as PortfolioTrade['kind'],
  date: localTime(initialDate), quantity: trade ? String(trade.quantity) : '',
  price: trade ? String(trade.price) : '', fees: trade ? String(trade.fees) : '0', notes: trade?.notes ?? '',
  openingQuantity: holding ? String(holding.quantity) : '', openingPrice: holding ? String(holding.averageCost) : '',
  holdingNotes: asset?.notes ?? '',
  settlesCash: trade ? trade.settlesCash === true : true,
  cashKind: cashEntry?.kind ?? props.cashAction ?? 'balance' as PortfolioCashEntry['kind'],
  cashAmount: cashEntry ? String(cashEntry.amount) : !props.cashAction || props.cashAction === 'balance' ? String(store.cash) : '',
  cashNotes: cashEntry?.notes ?? '',
})
const marketPage = ref(asset?.marketPage ?? 1)
const title = computed(() => props.kind === 'cash' ? cashEntry ? 'Edit cash entry' : 'Manage cash' : props.kind === 'asset' ? asset ? 'Edit holding' : 'Add holding' : trade ? 'Edit trade' : 'Add trade')
const disabled = computed(() => store.busy || store.refreshing)

onMounted(() => {
  store.error = ''
  dialog.value?.showModal()
  if (props.focusNotes) notesField.value?.focus()
  if (draft.source === 'market' && props.kind === 'asset') void store.loadMarket(marketPage.value)
})
watch(() => draft.source, (source) => { if (source === 'market') void store.loadMarket(marketPage.value) })
function selectCoin(event: Event) {
  const coin = store.marketCoins[Number((event.target as HTMLSelectElement).value)]
  if (!coin) return
  Object.assign(draft, { name: coin.name, symbol: coin.symbol, marketName: coin.name, marketPage: coin.page })
}
async function turnPage(direction: number) {
  marketPage.value = Math.max(1, Math.min(100, marketPage.value + direction))
  await store.loadMarket(marketPage.value)
}
function number(raw: string | number, label: string, positive = false): number {
  const value = Number(raw)
  if (!String(raw).trim() || !Number.isFinite(value) || value < 0 || (positive && value === 0)) throw new Error(`Enter a ${positive ? 'positive' : 'non-negative'} ${label}.`)
  return value
}
async function submit() {
  if (disabled.value) return
  error.value = ''
  try {
    let saved: boolean
    const entryDate = draft.date === localTime(initialDate) ? initialDate : new Date(draft.date).toISOString()
    if (props.kind === 'asset') {
      const next: PortfolioAsset = {
        id: asset?.id ?? crypto.randomUUID(), name: draft.name.trim(), symbol: draft.symbol.trim().toUpperCase(),
        source: draft.source, marketName: draft.marketName, marketPage: draft.marketPage,
        manualPrice: String(draft.manualPrice).trim() ? number(draft.manualPrice, 'current price') : null,
        updatedAt: new Date().toISOString(), notes: draft.holdingNotes,
      }
      const quantity = !asset && !String(draft.openingQuantity).trim() ? 0 : number(draft.openingQuantity, 'quantity')
      saved = await store.saveAsset(next, asset || quantity > 0 ? {
        quantity, price: quantity > 0 ? number(draft.openingPrice, 'average cost') : 0,
        date: asset ? undefined : entryDate,
      } : undefined, revision)
    } else if (props.kind === 'cash') {
      saved = await store.saveCashEntry({
        id: cashEntry?.id ?? crypto.randomUUID(), kind: draft.cashKind, date: entryDate,
        amount: number(draft.cashAmount, 'cash amount', draft.cashKind !== 'balance'), notes: draft.cashNotes.trim(),
      }, revision)
    } else {
      saved = await store.saveTrade({
        id: trade?.id ?? crypto.randomUUID(), assetId: draft.assetId, kind: draft.kind,
        date: entryDate, quantity: number(draft.quantity, 'quantity', draft.kind !== 'adjustment'),
        price: number(draft.price, 'price'), fees: draft.kind === 'adjustment' ? 0 : number(draft.fees, 'fees'), notes: draft.notes.trim(),
        settlesCash: (draft.kind === 'buy' || draft.kind === 'sell') && draft.settlesCash,
      }, revision)
    }
    if (saved) { emit('close'); void store.refreshPrices() }
  } catch (e) { error.value = e instanceof Error ? e.message : String(e) }
}
</script>

<template>
  <dialog ref="dialog" class="qp-editor" aria-labelledby="qp-editor-title" @cancel="disabled ? $event.preventDefault() : emit('close')">
    <form @submit.prevent="submit">
      <header><h2 id="qp-editor-title">{{ title }}</h2><span>USD</span></header>
      <template v-if="kind === 'asset'">
        <label>Pricing<select v-model="draft.source"><option value="manual">Manual price · any asset</option><option value="market">Live crypto · Terminal market feed</option></select></label>
        <div v-if="draft.source === 'market'" class="qp-editor__market">
          <label>Crypto asset<select :disabled="store.marketLoading" value="" @change="selectCoin"><option value="" disabled>{{ store.marketLoading ? 'Loading market prices…' : 'Choose an asset' }}</option><option v-for="(coin, index) in store.marketCoins" :key="`${coin.symbol}:${coin.name}`" :value="index">{{ coin.symbol }} · {{ coin.name }}</option></select></label>
          <div class="qp-editor__paging"><button type="button" :disabled="store.marketLoading || marketPage === 1" @click="turnPage(-1)">Previous</button><span>Market page {{ marketPage }}</span><button type="button" :disabled="store.marketLoading || marketPage === 100" @click="turnPage(1)">Next</button></div>
          <p v-if="draft.marketName">Selected: {{ draft.symbol }} · {{ draft.marketName }}</p>
          <p v-if="store.quoteError" role="alert">{{ store.quoteError }} <button type="button" @click="store.loadMarket(marketPage)">Retry</button></p>
        </div>
        <div class="qp-editor__grid"><label>Symbol<input v-model="draft.symbol" required maxlength="30" :readonly="draft.source === 'market'" placeholder="BTC, AAPL, VWCE…" /></label><label>Name<input v-model="draft.name" required maxlength="150" placeholder="Asset name" /></label></div>
        <label v-if="draft.source === 'manual'">Current unit price (USD)<input v-model="draft.manualPrice" type="number" min="0" step="any" placeholder="Leave empty until known" /></label>
        <template v-if="!asset">
          <fieldset><legend>Start with what you already hold</legend>
            <div class="qp-editor__grid"><label>Opening quantity<input v-model="draft.openingQuantity" type="number" min="0" step="any" placeholder="0" /></label><label>Average unit cost (USD)<input v-model="draft.openingPrice" type="number" min="0" step="any" :required="Number(draft.openingQuantity) > 0" placeholder="Including purchase fees" /></label></div>
            <label>Held since<input v-model="draft.date" type="datetime-local" required /></label>
            <p>Opening holdings leave cash unchanged. To fund a new purchase from cash, leave quantity empty and add a Buy trade. Avoid counting the same purchase both here and in Trade history.</p>
          </fieldset>
        </template>
        <fieldset v-else><legend>Current position</legend>
          <div class="qp-editor__grid"><label>Current quantity<input v-model="draft.openingQuantity" type="number" min="0" step="any" required /></label><label>Average unit cost (USD)<input v-model="draft.openingPrice" type="number" min="0" step="any" :required="Number(draft.openingQuantity) > 0" /></label></div>
          <p>Changes are saved as an editable position adjustment without moving cash. Earlier trades and realized gains stay intact.</p>
        </fieldset>
        <label>Holding notes<textarea ref="notesField" v-model="draft.holdingNotes" rows="4" maxlength="10000" placeholder="Thesis / theory, entry criteria, risks, exit plan…" /></label>
      </template>
      <template v-else-if="kind === 'cash'">
        <div class="qp-editor__balance"><span>Available cash</span><strong>{{ money(store.cash) }}</strong></div>
        <label>Cash movement<select v-model="draft.cashKind"><option value="deposit">Deposit</option><option value="withdrawal">Withdraw</option><option value="balance">Set balance</option></select></label>
        <div class="qp-editor__grid"><label>{{ draft.cashKind === 'balance' ? 'New balance (USD)' : 'Amount (USD)' }}<input v-model="draft.cashAmount" type="number" min="0" step="any" required /></label><label>Date and time<input v-model="draft.date" type="datetime-local" required /></label></div>
        <label>Notes<textarea v-model="draft.cashNotes" rows="3" maxlength="2000" placeholder="Transfer, withdrawal, balance correction…" /></label>
        <p v-if="draft.cashKind === 'balance'">Sets cash to this amount at the selected time. Later deposits, withdrawals and trades apply to that balance.</p>
      </template>
      <template v-else>
        <div class="qp-editor__grid"><label>Asset<select v-model="draft.assetId" required><option v-for="a in store.document.assets" :key="a.id" :value="a.id">{{ a.symbol }} · {{ a.name }}</option></select></label><label>Entry type<select v-model="draft.kind"><option value="buy">Buy</option><option value="sell">Sell</option><option value="opening">Opening holding</option><option value="adjustment">Position adjustment</option></select></label></div>
        <label>Date and time<input v-model="draft.date" type="datetime-local" required /></label>
        <div class="qp-editor__grid"><label>{{ draft.kind === 'adjustment' ? 'Position quantity' : 'Quantity' }}<input v-model="draft.quantity" type="number" min="0" step="any" required autofocus /></label><label>{{ draft.kind === 'adjustment' ? 'Average unit cost (USD)' : 'Unit price (USD)' }}<input v-model="draft.price" type="number" min="0" step="any" required /></label></div>
        <label v-if="draft.kind !== 'adjustment'">Fees (USD)<input v-model="draft.fees" type="number" min="0" step="any" required /></label>
        <template v-if="draft.kind === 'buy' || draft.kind === 'sell'">
          <label v-if="trade && !trade.settlesCash" class="qp-editor__check"><input v-model="draft.settlesCash" type="checkbox" /> Apply this trade to cash</label>
          <p v-if="draft.settlesCash">{{ draft.kind === 'buy' ? 'Paid from portfolio cash, including fees.' : 'Net sale proceeds return to portfolio cash.' }} Available now: {{ money(store.cash) }}.</p>
          <p v-else>This earlier trade is excluded from cash until you choose to apply it.</p>
        </template>
        <label>Notes<textarea v-model="draft.notes" rows="2" maxlength="2000" /></label>
        <p v-if="draft.kind === 'adjustment'">Sets the position's quantity and average cost without moving cash. Earlier realized gains stay intact; later trades apply to the corrected balance.</p>
        <p v-else>Every entry stays editable. Changes recalculate holdings and gains using average cost. Selling more than you held at that time is rejected.</p>
      </template>
      <p v-if="error || store.error" class="qp-editor__error" role="alert">{{ error || store.error }}</p>
      <footer><button type="button" :disabled="disabled" @click="emit('close')">Cancel</button><button type="submit" class="qp-editor__save" :disabled="disabled">{{ store.busy ? 'Saving…' : 'Save' }}</button></footer>
    </form>
  </dialog>
</template>

<style scoped>
.qp-editor { width: min(580px, calc(100% - 32px)); max-height: calc(100% - 40px); margin: auto; padding: 24px; border: 1px solid var(--border); border-radius: 12px; color: var(--text-primary); background: var(--surface-1); overflow: auto; }
.qp-editor::backdrop { background: #0009; }
.qp-editor form { display: flex; flex-direction: column; gap: 16px; }
.qp-editor header, .qp-editor footer, .qp-editor__paging { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.qp-editor h2 { font-size: 18px; font-weight: 600; margin: 0; }
.qp-editor label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--text-secondary); }
.qp-editor input, .qp-editor select, .qp-editor textarea { width: 100%; min-width: 0; padding: 8px 10px; color: var(--text-primary); background: var(--surface-2); border: 1px solid var(--border); border-radius: 6px; }
.qp-editor button { padding: 7px 12px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface-2); color: var(--text-primary); cursor: pointer; font-size: 12px; }
.qp-editor button:disabled { opacity: .5; cursor: default; }
.qp-editor__grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.qp-editor p, .qp-editor header span, .qp-editor__paging { font-size: 11px; color: var(--text-secondary); line-height: 1.6; margin: 0; }
.qp-editor fieldset { display: flex; flex-direction: column; gap: 12px; padding: 14px; border: 1px solid var(--border); border-radius: 8px; }
.qp-editor legend { padding: 0 5px; font-size: 12px; }
.qp-editor__market { display: flex; flex-direction: column; gap: 8px; }
.qp-editor .qp-editor__error { color: var(--negative); }
.qp-editor footer { justify-content: flex-end; }
.qp-editor .qp-editor__save { background: var(--accent); color: white; }
.qp-editor__balance { display: flex; justify-content: space-between; align-items: center; padding: 14px; border: 1px solid var(--border); border-radius: 8px; font-size: 12px; }
.qp-editor__balance strong { font-size: 20px; }
.qp-editor .qp-editor__check { flex-direction: row; align-items: center; }
.qp-editor__check input { width: auto; }
</style>
