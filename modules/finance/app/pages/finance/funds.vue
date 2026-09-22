<script setup lang="ts">
import { useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#finance/stores/app'
import { useFundsStore } from '#finance/stores/funds'
import { formatAmount, formatCents } from '#finance/utils/money'
import { parseFundCents } from '#finance/utils/fund-money'
import type { Fund, FundEntryKind, FundPlan } from '#finance/types/funds'

definePageMeta({ layout: 'finance' })
const app = useAppStore()
const store = useFundsStore()
const route = useRoute()
const router = useRouter()
const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
const editable = (cents: number) => formatAmount(cents, app.settings.locale)
const localToday = () => {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
const today = ref(localToday())
function refresh() { today.value = localToday(); void store.load() }
onMounted(refresh)
onActivated(refresh)
useTauriEvent('finance:funds-changed', () => void store.load())
const displayDate = (date: string) => new Intl.DateTimeFormat(app.settings.locale, { dateStyle: 'medium' }).format(new Date(`${date}T12:00:00`))
type Mode = 'fund' | 'entry' | 'plan' | 'delete'
const dialog = ref<HTMLDialogElement | null>(null)
const mode = ref<Mode>('fund')
const editingId = ref<string | null>(null)
const fundId = ref('')
const validation = ref('')
const deleting = ref<{ kind: 'fund' | 'entry' | 'plan'; id: string; name: string } | null>(null)
const draft = reactive({ name: '', date: '', amount: '', opening: '', value: '', target: '', notes: '', kind: 'deposit' as FundEntryKind, every: 1, active: true })
const title = computed(() => mode.value === 'fund' ? `${editingId.value ? 'Edit' : 'New'} fund`
  : mode.value === 'plan' ? `${editingId.value ? 'Edit' : 'New'} savings plan`
    : mode.value === 'delete' ? `Delete ${deleting.value?.kind}?`
      : ({ deposit: 'Add deposit', withdrawal: 'Add withdrawal', valuation: 'Update current value' })[draft.kind])

function open(nextMode: Mode) {
  mode.value = nextMode
  validation.value = ''
  store.error = ''
  today.value = localToday()
  dialog.value?.showModal()
}
function editFund(fund?: Fund) {
  editingId.value = fund?.id ?? null
  Object.assign(draft, { name: fund?.name ?? '', date: fund?.openedOn ?? localToday(), opening: editable(fund?.openingCents ?? 0), value: editable(fund?.openingValueCents ?? 0), target: fund?.targetCents ? editable(fund.targetCents) : '', notes: fund?.notes ?? '' })
  open('fund')
}
function addEntry(kind: FundEntryKind) {
  if (!store.selected) return
  fundId.value = store.selected.id
  Object.assign(draft, { date: localToday(), kind, amount: kind === 'valuation' ? editable(store.selected.currentValueCents) : '', notes: '' })
  open('entry')
}
function editPlan(plan?: FundPlan) {
  if (!store.selected) return
  fundId.value = store.selected.id
  editingId.value = plan?.id ?? null
  Object.assign(draft, { name: plan?.name ?? 'Savings plan', date: plan?.nextOn ?? localToday(), amount: plan ? editable(plan.amountCents) : '', every: plan?.everyMonths ?? 1, active: plan?.isActive ?? true })
  open('plan')
}
function askDelete(kind: 'fund' | 'entry' | 'plan', id: string, name: string) {
  deleting.value = { kind, id, name }
  open('delete')
}
function cents(raw: string, label: string, positive = false) {
  // Reject text and fractional cents instead of silently converting them to another amount.
  const value = parseFundCents(raw)
  if (value === null || !Number.isSafeInteger(value) || value < (positive ? 1 : 0)) throw new Error(`Enter a ${positive ? 'positive' : 'non-negative'} ${label}.`)
  return value
}
async function submit() {
  if (store.busy) return
  validation.value = ''
  try {
    let saved = false
    if (mode.value === 'fund') {
      saved = await store.saveFund({ name: draft.name.trim(), openedOn: draft.date, openingCents: cents(draft.opening, 'opening input'), openingValueCents: cents(draft.value, 'opening value'), targetCents: draft.target.trim() ? cents(draft.target, 'target', true) : null, notes: draft.notes.trim() }, editingId.value)
    } else if (mode.value === 'entry') {
      saved = await store.addEntry({ fundId: fundId.value, kind: draft.kind, amountCents: cents(draft.amount, 'amount', draft.kind !== 'valuation'), occurredOn: draft.date, notes: draft.notes.trim() })
    } else if (mode.value === 'plan') {
      saved = await store.savePlan({ fundId: fundId.value, name: draft.name.trim(), amountCents: cents(draft.amount, 'amount', true), everyMonths: draft.every, nextOn: draft.date, isActive: draft.active }, editingId.value)
    } else if (deleting.value) {
      saved = await store.remove(deleting.value.kind, deleting.value.id)
    }
    if (saved) dialog.value?.close()
  } catch (e) { validation.value = e instanceof Error ? e.message : String(e) }
}
// Sidebar controls use this page's existing editor so every action shares one form.
watch(() => store.editorRequest, async (request) => {
  if (!request) return
  store.editorRequest = null
  await nextTick()
  if (request.kind === 'fund') {
    const fund = store.funds.find((f) => f.id === request.id)
    if (!request.id || fund) editFund(fund)
  } else if (request.kind === 'plan') {
    const fund = store.funds.find((f) => f.id === request.fundId)
    if (!fund) return
    store.selectedId = fund.id
    const plan = fund.plans.find((p) => p.id === request.id)
    if (!request.id || plan) editPlan(plan)
  } else {
    askDelete(request.recordKind, request.id, request.name)
  }
}, { immediate: true, flush: 'post' })
onBeforeUnmount(() => { store.editorRequest = null })

// Consume navigation requests only after the shared fund data and dialog exist.
watch([() => route.query.fund, () => route.query.action, () => store.loading, dialog], async () => {
  if (store.loading || !store.loaded || !dialog.value || typeof route.query.fund !== 'string') return
  const fund = store.funds.find((f) => f.id === route.query.fund)
  if (!fund) return
  store.selectedId = fund.id
  const action = route.query.action
  // Clear the request so sidebar selection and later refreshes stay independent.
  const { fund: _fund, action: _action, ...query } = route.query
  await router.replace({ query })
  if (action === 'deposit' || action === 'withdrawal' || action === 'valuation') addEntry(action)
  else if (action === 'edit') editFund(fund)
  else if (action === 'delete') askDelete('fund', fund.id, fund.name)
}, { flush: 'post' })
</script>

<template>
  <div class="qfunds">
    <div v-if="store.error" class="qfunds-error" role="alert">{{ store.error }} <button class="qf-btn" :disabled="store.busy || store.loading" @click="refresh">Retry</button></div>
    <p v-if="store.loading && !store.loaded" class="qfunds-empty" role="status">Loading funds…</p>
    <QEmptyState v-else-if="store.loaded && !store.funds.length" title="No funds yet" description="Add a fund and its opening balance.">
      <button class="qf-btn" @click="editFund()">New fund</button>
    </QEmptyState>

    <section v-else-if="store.selected" class="qfunds-detail">
        <header class="qfunds-section-head">
          <div><h2>{{ store.selected.name }}</h2><p v-if="store.selected.notes">{{ store.selected.notes }}</p></div>
          <div class="qfunds-actions"><button class="qf-btn" :disabled="store.busy" @click="editFund(store.selected)">Edit fund</button><button class="qfunds-text-btn" :disabled="store.busy" @click="askDelete('fund', store.selected.id, store.selected.name)">Delete</button></div>
        </header>

        <div class="qfunds-balance">
          <span>Current value</span><strong>{{ money(store.selected.currentValueCents) }}</strong>
          <small>As of {{ displayDate(store.selected.valueAsOf) }} · manually tracked</small>
        </div>
        <div class="qfunds-metrics">
          <div><span>Total input</span><strong>{{ money(store.selected.depositedCents) }}</strong></div>
          <div><span>Total output</span><strong>{{ money(store.selected.withdrawnCents) }}</strong></div>
          <div><span>Net contributed</span><strong>{{ money(store.selected.netInputCents) }}</strong></div>
          <div><span>Value change</span><strong :class="store.selected.gainCents < 0 ? 'qf-neg' : 'qf-pos'">{{ money(store.selected.gainCents) }}</strong></div>
        </div>
        <div class="qfunds-actions qfunds-entry-actions">
          <button class="qf-btn" :disabled="store.busy" @click="addEntry('deposit')">+ Deposit</button>
          <button class="qf-btn" :disabled="store.busy" @click="addEntry('withdrawal')">− Withdrawal</button>
          <button class="qf-btn" :disabled="store.busy" @click="addEntry('valuation')">Update value</button>
        </div>

        <section class="qfunds-section">
          <header class="qfunds-section-head"><h3>History</h3><span class="qfunds-hint">{{ store.selected.entries.length }} entries</span></header>
          <div class="qfunds-table-scroll"><table class="qfunds-history">
            <thead><tr><th>Date</th><th>Entry</th><th>Note</th><th class="qfunds-number">Amount</th><th><span class="qfunds-sr">Actions</span></th></tr></thead>
            <tbody>
              <tr v-for="entry in store.selected.entries" :key="entry.id">
                <td>{{ displayDate(entry.occurredOn) }}</td>
                <td>{{ entry.kind === 'valuation' ? 'Value update' : entry.kind === 'deposit' ? 'Deposit' : 'Withdrawal' }}<small v-if="entry.planId">Savings plan</small></td>
                <td class="qfunds-note">{{ entry.notes || '—' }}</td>
                <td class="qfunds-number" :class="{ 'qf-pos': entry.kind === 'deposit', 'qf-neg': entry.kind === 'withdrawal' }">{{ entry.kind === 'deposit' ? '+' : entry.kind === 'withdrawal' ? '−' : '' }}{{ money(entry.amountCents) }}</td>
                <td><button class="qfunds-text-btn" :disabled="store.busy" :aria-label="`Delete ${entry.kind} on ${entry.occurredOn}`" @click="askDelete('entry', entry.id, `${entry.kind} · ${money(entry.amountCents)}`)">Delete</button></td>
              </tr>
              <tr class="qfunds-opening"><td>{{ displayDate(store.selected.openedOn) }}</td><td>Opening balance</td><td>Input {{ money(store.selected.openingCents) }}</td><td class="qfunds-number">{{ money(store.selected.openingValueCents) }}</td><td /></tr>
            </tbody>
          </table></div>
        </section>
    </section>

    <dialog ref="dialog" class="qfunds-dialog" aria-labelledby="fund-dialog-title" @cancel="store.busy && $event.preventDefault()">
      <form @submit.prevent="submit">
        <h2 id="fund-dialog-title">{{ title }}</h2>
        <template v-if="mode === 'delete'">
          <p>Delete {{ deleting?.name }}?</p>
          <p class="qfunds-hint">{{ deleting?.kind === 'fund' ? 'This removes the fund, its history and all savings plans.' : deleting?.kind === 'plan' ? 'Booked deposits stay in the fund history.' : 'The balance will be recalculated. Savings plan dates stay unchanged.' }}</p>
        </template>
        <template v-else>
          <label v-if="mode !== 'entry'">Name<input v-model="draft.name" class="qf-input" required maxlength="150" autofocus :placeholder="mode === 'fund' ? 'e.g. Emergency Fund' : 'e.g. Monthly investment'" /></label>
          <label>{{ mode === 'fund' ? 'Opening date' : mode === 'plan' ? 'Next deposit' : 'Date' }}<input v-model="draft.date" class="qf-input" type="date" required min="1900-01-01" :max="mode === 'plan' ? '9998-12-31' : today" /></label>
          <template v-if="mode === 'fund'">
            <div class="qfunds-form-grid">
              <label>Contributed at opening ({{ app.settings.currency }})<input v-model="draft.opening" class="qf-input" inputmode="decimal" required /></label>
              <label>Value at opening ({{ app.settings.currency }})<input v-model="draft.value" class="qf-input" inputmode="decimal" required /></label>
            </div>
            <p class="qfunds-hint">For an existing investment, enter the money you contributed and its value on the opening date. Add later value changes with “Update value”.</p>
            <label>Target (optional, {{ app.settings.currency }})<input v-model="draft.target" class="qf-input" inputmode="decimal" placeholder="No target" /></label>
          </template>
          <template v-else>
            <label>{{ mode === 'entry' && draft.kind === 'valuation' ? 'Current value' : 'Amount' }} ({{ app.settings.currency }})<input v-model="draft.amount" class="qf-input" inputmode="decimal" required autofocus /></label>
            <p v-if="mode === 'entry' && draft.kind === 'valuation'" class="qfunds-hint">Sets the value on this date, including movements already recorded for that day. Later movements adjust it; earlier movements only change net contributions.</p>
          </template>
          <template v-if="mode === 'plan'">
            <label>Repeat<select v-model.number="draft.every" class="qf-select"><option :value="1">Monthly</option><option :value="3">Quarterly</option><option :value="6">Half-yearly</option><option :value="12">Yearly</option></select></label>
            <label class="qfunds-checkbox"><input v-model="draft.active" type="checkbox" /> Active</label>
            <p class="qfunds-hint">Due deposits appear here for confirmation. Pausing keeps the next due date; edit it to skip a period.</p>
          </template>
          <label v-else>Notes (optional)<textarea v-model="draft.notes" class="qf-input" rows="2" maxlength="2000" /></label>
        </template>
        <p v-if="validation || store.error" class="qfunds-error" role="alert">{{ validation || store.error }}</p>
        <footer class="qfunds-actions"><button type="button" class="qf-btn" :disabled="store.busy" @click="dialog?.close()">Cancel</button><button type="submit" class="qf-btn" :disabled="store.busy">{{ store.busy ? 'Saving…' : mode === 'delete' ? 'Delete' : 'Save' }}</button></footer>
      </form>
    </dialog>
  </div>
</template>

<style scoped>
.qfunds { height: 100%; overflow: auto; padding: 24px; container-type: inline-size; }
.qfunds h2, .qfunds h3, .qfunds p { margin: 0; }
.qfunds h2 { font-size: 20px; font-weight: 600; }
.qfunds h3 { font-size: 13px; font-weight: 650; }
.qfunds-section-head { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; }
.qfunds-section-head p { color: var(--qf-text-muted); margin-top: 5px; font-size: 12px; }
.qfunds-detail { min-width: 0; max-width: 1040px; margin: 0 auto; padding-bottom: 24px; }
.qfunds-balance { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 28px 0 22px; }
.qfunds-balance > span { color: var(--qf-text-secondary); font-size: 12px; }
.qfunds-balance > strong { font-size: 34px; font-weight: 600; font-variant-numeric: tabular-nums; letter-spacing: -.025em; overflow-wrap: anywhere; }
.qfunds-balance small { font-size: 10px; color: var(--qf-text-muted); }
.qfunds-metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 16px; padding: 16px; border: 1px solid var(--qf-border-subtle); border-radius: var(--qf-radius-lg); background: var(--qf-bg-raised); }
.qfunds-metrics > div { display: flex; flex-direction: column; gap: 5px; }
.qfunds-metrics span { color: var(--qf-text-muted); font-size: 11px; }
.qfunds-metrics strong { font-weight: 500; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.qfunds-actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.qfunds-entry-actions { margin-top: 18px; }
.qfunds-section { border-top: 1px solid var(--qf-border-subtle); margin-top: 28px; padding-top: 16px; }
.qfunds .qfunds-hint { color: var(--qf-text-muted); font-size: 11px; line-height: 1.6; margin: 8px 0; }
.qfunds-text-btn { background: transparent; border: 0; color: var(--qf-text-muted); padding: 4px; font-size: 11px; cursor: pointer; }
.qfunds-text-btn:hover { color: var(--qf-text); }
.qfunds button:disabled { opacity: .5; cursor: default; }
.qfunds-table-scroll { overflow-x: auto; margin-top: 12px; }
.qfunds-history { width: 100%; border-collapse: collapse; font-size: 12px; }
.qfunds-history th { text-align: left; color: var(--qf-text-muted); font-size: 10px; font-weight: 500; }
.qfunds-history th, .qfunds-history td { padding: 10px 8px; border-bottom: 1px solid var(--qf-border-subtle); }
.qfunds-history small { display: block; color: var(--qf-text-muted); font-size: 10px; }
.qfunds-history .qfunds-number { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.qfunds-note { max-width: 260px; overflow-wrap: anywhere; }
.qfunds-opening { color: var(--qf-text-muted); }
.qfunds-empty { display: flex; flex-direction: column; align-items: center; gap: 16px; max-width: 480px; margin: 0 auto; padding: 80px 12px; text-align: center; color: var(--qf-text-secondary); }
.qfunds-empty p { font-size: 12px; line-height: 1.7; }
.qfunds-error { border: 1px solid var(--qf-negative); border-radius: 6px; padding: 10px 12px; margin: 12px 0; color: var(--qf-text); overflow-wrap: anywhere; }
.qfunds-dialog { width: min(520px, calc(100% - 32px)); max-height: calc(100% - 40px); margin: auto; padding: 24px; border: 1px solid var(--qf-border); border-radius: 12px; background: var(--qf-bg-raised); color: var(--qf-text); overflow: auto; }
.qfunds-dialog::backdrop { background: #0009; }
.qfunds-dialog form { display: flex; flex-direction: column; gap: 16px; }
.qfunds-dialog label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--qf-text-secondary); }
.qfunds-dialog .qfunds-checkbox { flex-direction: row; align-items: center; }
.qfunds-dialog footer { justify-content: flex-end; }
.qfunds-form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.qfunds-sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
@container (max-width: 650px) {
  .qfunds-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .qfunds-balance > strong { font-size: 28px; }
}
</style>
