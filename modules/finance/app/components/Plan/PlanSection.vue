<script setup lang="ts">
/**
 * One block of the plan — money in, money set aside, or a fixed cost.
 *
 * Everything is edited in place. There is no dialog and no save button: type
 * a name, type an amount, done. That is the whole point of the module — the
 * entry has to be blunt enough that keeping the plan current is not a chore.
 *
 * A field commits on blur and on Enter. Escape puts the old value back.
 */
import { usePlanStore } from '#finance/stores/plan'
import { useAppStore } from '#finance/stores/app'
import type { Cadence, Item, ItemKind } from '#finance/types'
import { formatAmount, formatCents, parseCents } from '#finance/utils/money'

const props = defineProps<{
  kind: ItemKind
  title: string
  hint?: string
  /** Sum of this block, in cents — always positive; the sign is the block. */
  totalCents: number
}>()

const plan = usePlanStore()
const app = useAppStore()
const router = useRouter()

function openFund(item: Item, action?: string) {
  if (item.fundId) void router.push({ path: '/finance/funds', query: { fund: item.fundId, ...(action ? { action } : {}) } })
}

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)

const rows = computed(() => plan.of(props.kind))

const CADENCES: Array<{ value: Cadence; label: string }> = [
  { value: 1, label: 'monthly' },
  { value: 3, label: 'quarterly' },
  { value: 6, label: 'twice a year' },
  { value: 12, label: 'yearly' },
]

/** Which row has its extra line open (target, already saved, note). */
const expanded = ref<string | null>(null)

function toggle(id: string) {
  expanded.value = expanded.value === id ? null : id
}

// ── Editing ───────────────────────────────────────────────────────────────

function commitName(item: Item, event: Event) {
  const value = (event.target as HTMLInputElement).value.trim()
  if (value === item.name) return
  void plan.update(item.id, { name: value })
}

function amountText(cents: number): string {
  return cents ? (cents / 100).toFixed(2) : ''
}

function commitAmount(item: Item, event: Event) {
  const el = event.target as HTMLInputElement
  // An unreadable or empty field means zero, not "keep the old figure" — a
  // silently retained amount is how a plan drifts away from reality.
  const cents = Math.abs(parseCents(el.value) ?? 0)
  if (cents === item.amountCents) {
    el.value = amountText(cents)
    return
  }
  void plan.update(item.id, { amountCents: cents })
}

function commitTarget(item: Item, event: Event) {
  const cents = parseCents((event.target as HTMLInputElement).value)
  void plan.update(item.id, { targetCents: cents && cents > 0 ? Math.abs(cents) : null })
}

function revert(event: KeyboardEvent, value: string) {
  const el = event.target as HTMLInputElement
  el.value = value
  el.blur()
}

async function addRow() {
  const created = await plan.add(props.kind)
  await nextTick()
  // Land the caret in the new row's name so a second line is one keystroke
  // away rather than a hunt for the field.
  const el = document.querySelector<HTMLInputElement>(`[data-row="${created.id}"] .qf-pl__name`)
  el?.focus()
  el?.select()
}
</script>

<template>
  <section class="qf-pl">
    <header class="qf-pl__head">
      <div class="qf-pl__title">
        <h2>{{ title }}</h2>
        <span v-if="hint" class="qf-pl__hint">{{ hint }}</span>
      </div>
      <span class="qf-pl__total qf-num">{{ money(totalCents) }}</span>
    </header>

    <div class="qf-pl__rows">
      <div
        v-for="item in rows"
        :key="item.id"
        class="qf-pl__row"
        :class="{ 'is-off': !item.isActive }"
        :data-row="item.id"
      >
        <div class="qf-pl__main">
          <button
            class="qf-dot qf-pl__swatch"
            :data-color="item.color"
            :title="item.isActive ? 'Pause this line' : 'Resume this line'"
            :disabled="!!item.fundId && item.fundPlanCount === 0"
            @click="plan.update(item.id, { isActive: !item.isActive })"
          />

          <input
            class="qf-input qf-pl__name"
            :value="item.name"
            placeholder="What is it?"
            @blur="commitName(item, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            @keydown.esc="revert($event, item.name)"
          />

          <input
            v-if="!item.fundId || item.fundPlanCount <= 1"
            class="qf-input qf-num qf-pl__amount"
            :value="amountText(item.amountCents)"
            inputmode="decimal"
            placeholder="0,00"
            @blur="commitAmount(item, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            @keydown.esc="revert($event, amountText(item.amountCents))"
          />
          <button v-else class="qf-pl__amount qf-pl__fund-link qf-num" @click="openFund(item)">{{ money(item.monthlyCents) }} / mo</button>

          <select
            v-if="!item.fundId || item.fundPlanCount <= 1"
            class="qf-select qf-pl__every"
            :value="item.everyMonths"
            :disabled="!!item.fundId && item.fundPlanCount === 0"
            @change="plan.update(item.id, { everyMonths: Number(($event.target as HTMLSelectElement).value) })"
          >
            <option v-for="c in CADENCES" :key="c.value" :value="c.value">{{ c.label }}</option>
          </select>
          <button v-else class="qf-pl__every qf-pl__fund-link" @click="openFund(item)">{{ item.fundPlanCount }} savings plans</button>

          <!-- The per-month figure is the crate's, never recomputed here. -->
          <span class="qf-pl__monthly qf-num" :title="'per month'">
            {{ item.everyMonths === 1 ? '' : formatAmount(item.monthlyCents, app.settings.locale) }}
          </span>

          <button class="qf-pl__more" :aria-expanded="expanded === item.id" @click="toggle(item.id)">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="m6 9 6 6 6-6" />
            </svg>
          </button>

          <button class="qf-pl__del" :aria-label="item.fundId ? 'Delete fund' : 'Delete this line'" @click="item.fundId ? openFund(item, 'delete') : plan.remove(item.id)">×</button>
        </div>

        <div v-if="item.fundId" class="qf-pl__fund">
          <button class="qf-pl__fund-link" @click="openFund(item)">Fund · {{ money(item.savedCents) }}</button>
          <button class="qf-pl__fund-link" @click="openFund(item, 'deposit')">+ Deposit</button>
          <button class="qf-pl__fund-link" @click="openFund(item, 'withdrawal')">− Withdrawal</button>
          <button class="qf-pl__fund-link" @click="openFund(item, 'edit')">Edit fund</button>
        </div>

        <div v-if="expanded === item.id" class="qf-pl__extra">
          <!-- A saving line with a target IS a goal; there is no second object. -->
          <template v-if="kind === 'saving'">
            <label class="qf-pl__field">
              <span>Target</span>
              <input
                class="qf-input qf-num"
                :value="item.targetCents ? (item.targetCents / 100).toFixed(2) : ''"
                inputmode="decimal"
                placeholder="none"
                @blur="commitTarget(item, $event)"
                @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
              />
            </label>
            <div class="qf-pl__field"><span>Current fund value</span><button class="qf-pl__fund-link qf-num" @click="openFund(item, 'valuation')">{{ money(item.savedCents) }} · Update value</button></div>
          </template>

          <label class="qf-pl__field qf-pl__field--wide">
            <span>Note</span>
            <input
              class="qf-input"
              :value="item.notes ?? ''"
              placeholder="optional"
              @blur="plan.update(item.id, { notes: ($event.target as HTMLInputElement).value.trim() || null })"
              @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            />
          </label>
        </div>
      </div>

      <button class="qf-pl__add" @click="addRow">+ Add line</button>
    </div>
  </section>
</template>

<style scoped>
.qf-pl__fund { display: flex; flex-wrap: wrap; gap: 12px; padding: 0 6px 8px 25px; }
.qf-pl__fund-link { border: 0; background: transparent; padding: 3px 0; color: var(--qf-text-muted); font-size: 11px; cursor: pointer; text-align: left; }
.qf-pl__fund-link:hover { color: var(--qf-text); }
.qf-pl__main:focus-within .qf-pl__more, .qf-pl__main:focus-within .qf-pl__del { opacity: 1; }
.qf-pl {
  display: flex;
  flex-direction: column;
  gap: 6px;
  container-type: inline-size;
}

.qf-pl__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--qf-border-subtle);
}

.qf-pl__title {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.qf-pl__title h2 {
  margin: 0;
  color: var(--qf-text);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.qf-pl__hint {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-pl__total {
  color: var(--qf-text);
  font-size: 15px;
  font-weight: 600;
}

.qf-pl__rows {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.qf-pl__row {
  border-radius: var(--qf-radius);
}

.qf-pl__row.is-off {
  opacity: 0.45;
}

.qf-pl__main {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 6px;
  border-radius: var(--qf-radius);
}

.qf-pl__main:hover {
  background: var(--qf-bg-hover);
}

.qf-pl__swatch {
  flex-shrink: 0;
  width: 11px;
  height: 11px;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.qf-pl__swatch.is-on {
  outline: 2px solid var(--qf-accent);
  outline-offset: 1px;
}

/* Borderless until touched: the list reads as a list, not as a form. */
.qf-pl__name,
.qf-pl__amount {
  border-color: transparent;
  background: transparent;
}

.qf-pl__name:hover,
.qf-pl__amount:hover,
.qf-pl__name:focus,
.qf-pl__amount:focus {
  border-color: var(--qf-border);
  background: var(--qf-bg-input);
}

.qf-pl__name {
  flex: 1;
  min-width: 0;
}

.qf-pl__amount {
  flex: 0 0 118px;
  min-width: 0;
}

.qf-pl__every {
  flex: 0 0 118px;
  min-width: 0;
  border-color: transparent;
  background: transparent;
  color: var(--qf-text-muted);
  font-size: 12px;
}

.qf-pl__every:hover,
.qf-pl__every:focus {
  border-color: var(--qf-border);
  background: var(--qf-bg-input);
  color: var(--qf-text);
}

.qf-pl__monthly {
  flex: 0 0 78px;
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-pl__more,
.qf-pl__del {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qf-text-muted);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
}

.qf-pl__main:hover .qf-pl__more,
.qf-pl__main:hover .qf-pl__del,
.qf-pl__more[aria-expanded='true'] {
  opacity: 1;
}

.qf-pl__more:hover,
.qf-pl__del:hover {
  background: var(--qf-bg-card);
  color: var(--qf-text);
}

.qf-pl__del:hover {
  color: var(--qf-negative);
}

.qf-pl__extra {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin: 2px 0 6px;
  padding: 10px 12px;
  border: 1px solid var(--qf-border-subtle);
  border-radius: var(--qf-radius);
  background: var(--qf-bg-raised);
}

.qf-pl__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.qf-pl__field--wide {
  flex: 1 1 220px;
}

.qf-pl__field > span {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-pl__field .qf-input {
  width: 140px;
}

.qf-pl__field--wide .qf-input {
  width: 100%;
}

.qf-pl__add {
  align-self: flex-start;
  margin-top: 2px;
  padding: 5px 8px;
  border: none;
  border-radius: var(--qf-radius);
  background: transparent;
  color: var(--qf-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.qf-pl__add:hover {
  background: var(--qf-bg-hover);
  color: var(--qf-text);
}

@container (max-width: 600px) {
  .qf-pl__monthly { display: none; }
  .qf-pl__amount { flex-basis: 96px; }
  .qf-pl__every { flex-basis: 108px; }
  .qf-pl__title { flex-wrap: wrap; gap: 4px 10px; }
}
</style>
