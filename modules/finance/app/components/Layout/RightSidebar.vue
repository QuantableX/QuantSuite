<script setup lang="ts">
/**
 * The right panel: saving targets and when they are reached.
 *
 * Two kinds, rendered by **one** loop so they cannot drift apart visually:
 *
 *   * **from the plan** — a saving line that carries a number to reach. Its
 *     rate is that line's monthly amount, so it is edited on the Plan page and
 *     carries a `plan` badge here; two places to change one figure drift.
 *   * **standalone** — not funded by the plan and part of no total. It reads
 *     exactly like a plan target until the edit button is pressed, which turns
 *     the same card into its own fields.
 */
import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import type { GoalProgress } from '#finance/types'
import { formatAmount, formatCents, parseCents } from '#finance/utils/money'

const app = useAppStore()
const plan = usePlanStore()

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)

/** The standalone target currently in edit mode, if any. */
const editing = ref<string | null>(null)

function pct(saved: number, target: number): number {
  return target > 0 ? Math.min(100, Math.round((saved / target) * 100)) : 0
}

/** `2028-03` → `March 2028`. Built from the parts; a bare date string parses
 * as UTC midnight and can read as the previous month. */
function monthLabel(period: string | null): string {
  if (!period) return '—'
  const [y, m] = period.split('-').map(Number)
  return new Date(y ?? 1970, (m ?? 1) - 1, 1).toLocaleDateString(app.settings.locale, {
    month: 'long',
    year: 'numeric',
  })
}

/**
 * The value an amount field shows.
 *
 * Formatted the way the figure above it is, so the same number does not read
 * as `10.00` in the field and `10,00 €` two lines up. `parseCents` takes
 * either separator back. Zero stays empty so the placeholder shows.
 */
function amountText(cents: number): string {
  return cents ? formatAmount(cents, app.settings.locale) : ''
}

function commit(goal: GoalProgress, field: 'targetCents' | 'savedCents' | 'monthlyCents', event: Event) {
  const cents = Math.abs(parseCents((event.target as HTMLInputElement).value) ?? 0)
  void plan.updateTarget(goal.id, { [field]: cents })
}

function commitName(goal: GoalProgress, event: Event) {
  const value = (event.target as HTMLInputElement).value.trim()
  if (value === goal.name) return
  void plan.updateTarget(goal.id, { name: value })
}

async function removeTarget(id: string) {
  editing.value = null
  await plan.removeTarget(id)
}

// ── Adding ────────────────────────────────────────────────────────────────

const adding = ref(false)
const newName = ref('')
const newInput = ref<HTMLInputElement | null>(null)

// The header button is far from the field it opens; land the caret there so
// the next thing is typing, not aiming.
watch(adding, (open) => {
  if (open) nextTick(() => newInput.value?.focus())
})

async function addTarget() {
  const name = newName.value.trim()
  if (!name) return
  await plan.addTarget(name)
  newName.value = ''
  adding.value = false
}

/** 24×24 stroke path, per PLAN-V3 §3 — no emoji, no glyph characters. */
const ICON_TRASH = 'M4 7h16 M9 7V5h6v2 M6 7l1 13h10l1-13'
</script>

<template>
  <div class="qf-rs">
    <header class="qf-rs__head">
      <span>Targets</span>
      <button class="qf-rs__head-add" aria-label="Add a target" title="Add a target" @click="adding = true">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
          <path d="M12 5v14 M5 12h14" />
        </svg>
      </button>
    </header>

    <div class="qf-rs__body">
      <article v-for="goal in plan.goals" :key="goal.id" class="qf-rs__goal">
        <div class="qf-rs__goal-head">
          <span class="qf-dot" :data-color="goal.color" />

          <input
            v-if="editing === goal.id"
            class="qf-input qf-rs__name-input"
            :value="goal.name"
            placeholder="Target name"
            @blur="commitName(goal, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
          />
          <span v-else class="qf-rs__goal-name">{{ goal.name || 'Untitled' }}</span>

          <!-- Where a plan target carries its badge, a standalone one carries
               the control that turns this same card into its own form. -->
          <span v-if="goal.source === 'plan'" class="qf-rs__badge" title="Funded by a saving line in the plan">
            plan
          </span>
          <template v-else-if="editing === goal.id">
            <button class="qf-rs__act qf-rs__act--danger" aria-label="Delete this target" title="Delete" @click="removeTarget(goal.id)">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path :d="ICON_TRASH" />
              </svg>
            </button>
            <button class="qf-rs__badge qf-rs__badge--btn" aria-label="Done editing" @click="editing = null">
              done
            </button>
          </template>
          <button
            v-else
            class="qf-rs__badge qf-rs__badge--btn"
            aria-label="Edit this target"
            @click="editing = goal.id"
          >
            edit
          </button>
        </div>

        <div class="qf-rs__amounts">
          <strong class="qf-num">{{ money(goal.savedCents) }}</strong>
          <span class="qf-num">of {{ money(goal.targetCents) }}</span>
        </div>

        <div class="qf-rs__bar">
          <span class="qf-rs__fill" :style="{ width: `${pct(goal.savedCents, goal.targetCents)}%` }" />
        </div>

        <div v-if="editing === goal.id" class="qf-rs__fields">
          <label>
            <span>Saved</span>
            <input
              class="qf-input qf-num"
              :value="amountText(goal.savedCents)"
              inputmode="decimal"
              placeholder="0,00"
              @blur="commit(goal, 'savedCents', $event)"
              @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            />
          </label>
          <label>
            <span>Target</span>
            <input
              class="qf-input qf-num"
              :value="amountText(goal.targetCents)"
              inputmode="decimal"
              placeholder="0,00"
              @blur="commit(goal, 'targetCents', $event)"
              @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            />
          </label>
          <label>
            <span>Per month</span>
            <input
              class="qf-input qf-num"
              :value="amountText(goal.monthlyCents)"
              inputmode="decimal"
              placeholder="0,00"
              @blur="commit(goal, 'monthlyCents', $event)"
              @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            />
          </label>
        </div>

        <dl v-else class="qf-rs__meta">
          <dt>Rate</dt>
          <dd class="qf-num">{{ money(goal.monthlyCents) }} / month</dd>
          <dt>Done</dt>
          <dd v-if="goal.monthsLeft === null" class="qf-rs__never">
            {{ goal.source === 'plan' ? 'Never — nothing is going in' : 'Set a monthly amount' }}
          </dd>
          <dd v-else-if="goal.monthsLeft === 0">Reached</dd>
          <dd v-else>
            {{ monthLabel(goal.reachedOn) }}
            <span class="qf-rs__months">({{ goal.monthsLeft }} mo)</span>
          </dd>
        </dl>
      </article>

      <div v-if="adding" class="qf-rs__new">
        <input
          ref="newInput"
          v-model="newName"
          class="qf-input"
          placeholder="What are you saving for?"
          @keydown.enter.prevent="addTarget"
          @keydown.esc="adding = false"
        />
        <button class="qf-btn" @click="addTarget">Add</button>
      </div>

      <p v-if="!plan.goals.length && !adding" class="qf-rs__empty">
        No targets yet. Add one with +.
      </p>
    </div>
  </div>
</template>

<style scoped>
.qf-rs {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.qf-rs__head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  /* Less right padding than left: the button's own box carries the rest, so
     its glyph lands on the same optical margin as the panel's content. */
  padding: 8px 8px 8px 14px;
  border-bottom: 1px solid var(--qf-border);
  color: var(--qf-text-secondary);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.qf-rs__head-add {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--qf-radius);
  background: transparent;
  color: var(--qf-text-muted);
  cursor: pointer;
}

.qf-rs__head-add:hover {
  background: var(--qf-bg-hover);
  color: var(--qf-text);
}

.qf-rs__body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 12px;
  overflow-y: auto;
  overflow-x: hidden;
}

.qf-rs__goal {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.qf-rs__goal-head {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
}

/* The static name and the input share one box — same padding, same border
   width, same metrics. Without the transparent border and the padding here the
   title jumps sideways by 7px the moment edit mode opens. */
.qf-rs__goal-name,
.qf-rs__name-input {
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--qf-radius);
  color: var(--qf-text);
  font-family: inherit;
  font-size: 13px;
  font-weight: 600;
  line-height: 1.5;
}

.qf-rs__goal-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qf-rs__name-input {
  background: transparent;
}

.qf-rs__name-input:hover,
.qf-rs__name-input:focus {
  border-color: var(--qf-border);
  background: var(--qf-bg-input);
}

/* `PLAN` and `EDIT` are the same pill in the same corner — four letters each,
   so the two kinds of card line up rather than one carrying a word and the
   other an icon of a different weight. */
.qf-rs__badge {
  flex-shrink: 0;
  /* PLAN, EDIT and DONE are four letters each but not four equal widths, and
     the cards stack — so the box is fixed and the word is centred in it,
     otherwise the pills stagger down the panel. */
  min-width: 42px;
  padding: 1px 6px;
  text-align: center;
  border: 1px solid var(--qf-border);
  border-radius: 999px;
  background: transparent;
  color: var(--qf-text-muted);
  font-family: inherit;
  font-size: 9px;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.qf-rs__badge--btn {
  cursor: pointer;
  transition: border-color 120ms ease, color 120ms ease;
}

.qf-rs__badge--btn:hover {
  border-color: var(--qf-accent);
  color: var(--qf-text);
}

.qf-rs__act {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qf-text-muted);
  cursor: pointer;
}

.qf-rs__act:hover {
  background: var(--qf-bg-card);
  color: var(--qf-text);
}

.qf-rs__act--danger:hover {
  color: var(--qf-negative);
}

.qf-rs__amounts {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.qf-rs__amounts strong {
  color: var(--qf-text);
  font-size: 15px;
}

.qf-rs__amounts span {
  color: var(--qf-text-muted);
  font-size: 11px;
}

.qf-rs__bar {
  height: 5px;
  border-radius: 3px;
  background: var(--qf-bg-card);
  overflow: hidden;
}

.qf-rs__fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--qf-flow-saving);
  transition: width 200ms ease;
}

.qf-rs__fields {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
}

.qf-rs__fields label {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.qf-rs__fields span {
  color: var(--qf-text-muted);
  font-size: 10px;
}

.qf-rs__fields .qf-input {
  padding: 4px 6px;
  font-size: 12px;
}

.qf-rs__meta {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px 10px;
  margin: 0;
  font-size: 11px;
}

.qf-rs__meta dt {
  color: var(--qf-text-muted);
}

.qf-rs__meta dd {
  margin: 0;
  color: var(--qf-text-secondary);
}

.qf-rs__months {
  color: var(--qf-text-muted);
}

.qf-rs__never {
  color: var(--qf-negative);
}

.qf-rs__new {
  display: flex;
  gap: 6px;
}

.qf-rs__empty {
  margin: 0;
  color: var(--qf-text-muted);
  font-size: 12px;
  line-height: 1.55;
}
</style>
