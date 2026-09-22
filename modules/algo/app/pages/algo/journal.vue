<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useJournalStore } from '#algo/stores/journal'
import { useExchangeStore } from '#algo/stores/exchange'
import { useBotsStore } from '#algo/stores/bots'
import { formatCurrency, formatPct, formatDuration, formatDateTime } from '#algo/utils/format'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import type { BotTradeEvent, Trade, DailyPnl } from '#algo/types'

const journalStore = useJournalStore()
const exchangeStore = useExchangeStore()
const botsStore = useBotsStore()

// One calendar: the year of the mode the switch selects (PLAN-QUANTALGO §3.5).
function yearTotal(days: DailyPnl[]): { pnl: number; trades: number } {
  return days.reduce((acc, d) => ({ pnl: acc.pnl + d.pnl, trades: acc.trades + d.trades }), { pnl: 0, trades: 0 })
}
const calendar = computed(() => {
  const days = journalStore.dailyPnl[journalStore.mode]
  return { label: journalStore.mode === 'live' ? 'Live' : 'Paper', days, ...yearTotal(days) }
})

// Selected trade for detail panel
const selectedTrade = ref<Trade | null>(null)
const editingNotes = ref('')
const isSavingNotes = ref(false)
let unlistenTrade: UnlistenFn | null = null

function handleSelect(trade: Trade) {
  if (selectedTrade.value?.id === trade.id) {
    selectedTrade.value = null
    return
  }
  selectedTrade.value = trade
  editingNotes.value = trade.notes ?? ''
}

async function handleUpdateNotes(id: string, notes: string) {
  isSavingNotes.value = true
  try {
    await journalStore.updateNotes(id, notes)
    if (selectedTrade.value?.id === id) {
      selectedTrade.value = { ...selectedTrade.value, notes }
    }
  } catch (err) {
    console.error('Failed to save notes:', err)
  } finally {
    isSavingNotes.value = false
  }
}

async function saveNotes() {
  if (!selectedTrade.value) return
  await handleUpdateNotes(selectedTrade.value.id, editingNotes.value)
}

// Filter changes need no watch here — the store's updateFilters() already
// reloads trades and stats after replacing the filters.

// V3 warm cache: this page stays mounted in the module's cached stage while
// another module is active — listen only while visible, refresh once on return.
// Arming happens in BOTH onMounted and onActivated: async pages mount after the
// stage's activation flush, so onActivated alone would miss the first visit;
// the isListening guard makes the overlap safe. That same late mount can land
// in a stage the user has already left, and there the isListening guard turns
// against us — it would block the real activation and leave the page dead —
// so onMounted arms only inside an active tree.
let isListening = false
// A boolean cannot tell "still listening" from "left and came back" — the
// generation does, so a call that lost the race drops its own handler instead
// of leaking it behind the newer one's.
let listenGen = 0

async function startListening() {
  if (isListening) return
  isListening = true
  const gen = ++listenGen
  await Promise.all([journalStore.loadTrades(), journalStore.loadStats(), journalStore.loadBreakdown(), journalStore.loadDailyPnl(), journalStore.loadFacets()])
  const unlistenFn = await listen<BotTradeEvent>('bot:trade', async () => {
    await journalStore.refresh()
  })
  // The page can be deactivated while the listen() call is in flight.
  if (gen !== listenGen) {
    unlistenFn()
    return
  }
  unlistenTrade = unlistenFn
}

function stopListening() {
  isListening = false
  listenGen++
  unlistenTrade?.()
  unlistenTrade = null
}

onMounted(() => {
  if (inActiveKeepAliveTree()) void startListening()
})
onActivated(startListening)

onDeactivated(stopListening)

onUnmounted(stopListening)
</script>

<template>
  <div class="journal">
    <!-- Loading State -->
    <div v-if="journalStore.isLoading && !journalStore.trades.length" class="journal__loading">
      <p class="text-muted">Loading trades...</p>
    </div>

    <template v-else>
      <!-- The switch picks the mode every number on this page belongs to -->
      <div class="journal__top">
        <div class="journal__stats">
          <AlgoJournalStatsHeader v-if="journalStore.stats" :stats="journalStore.stats" />
        </div>
        <div class="mode-switch" role="tablist" aria-label="Trading mode">
          <button
            class="mode-switch__btn"
            :class="{ active: journalStore.mode === 'paper' }"
            role="tab"
            :aria-selected="journalStore.mode === 'paper'"
            @click="journalStore.setMode('paper')"
          >
            Paper
          </button>
          <button
            class="mode-switch__btn"
            :class="{ active: journalStore.mode === 'live' }"
            role="tab"
            :aria-selected="journalStore.mode === 'live'"
            @click="journalStore.setMode('live')"
          >
            Live
          </button>
        </div>
      </div>

      <!-- Every bot with its own numbers, whatever the switch says -->
      <div v-if="journalStore.breakdown" class="journal__breakdown">
        <AlgoJournalBreakdown :breakdown="journalStore.breakdown" />
      </div>

      <!-- A year of days of the selected mode, full width -->
      <div class="journal__calendar">
        <div class="card calendar-card">
          <div class="calendar-card__head">
            <span class="calendar-card__label">{{ calendar.label }}</span>
            <span class="calendar-card__meta mono">
              <span :class="calendar.pnl > 0 ? 'text-success' : calendar.pnl < 0 ? 'text-error' : ''">{{ formatCurrency(calendar.pnl) }}</span>
              · {{ calendar.trades }} trades · 365 days
            </span>
          </div>
          <AlgoJournalCalendarHeatmap :days="calendar.days" />
        </div>
      </div>

      <!-- Trade Table: its own block, scrolls inside -->
      <div class="journal__table card">
        <AlgoJournalTradeTable
          :trades="journalStore.trades"
          @select="handleSelect"
          @update-notes="handleUpdateNotes"
        />
      </div>

      <!-- Expanded Trade Detail -->
      <Transition name="slide">
        <div v-if="selectedTrade" class="journal__detail card">
          <div class="detail__header">
            <h3 class="detail__title">Trade Detail</h3>
            <button class="btn btn-sm" @click="selectedTrade = null">Close</button>
          </div>

          <div class="detail__grid">
            <div class="detail__field">
              <span class="detail__label">Pair</span>
              <span class="detail__value">{{ selectedTrade.pair }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Side</span>
              <span
                class="detail__value"
                :class="selectedTrade.side === 'long' ? 'text-accent' : 'text-error'"
              >
                {{ selectedTrade.side.toUpperCase() }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Entry Price</span>
              <span class="detail__value mono">{{ formatCurrency(selectedTrade.entry_price) }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Exit Price</span>
              <span class="detail__value mono">
                {{ selectedTrade.exit_price != null ? formatCurrency(selectedTrade.exit_price) : '--' }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Quantity</span>
              <span class="detail__value mono">{{ selectedTrade.quantity }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">PnL</span>
              <span
                class="detail__value mono"
                :class="(selectedTrade.pnl ?? 0) >= 0 ? 'text-success' : 'text-error'"
              >
                {{ selectedTrade.pnl != null ? formatCurrency(selectedTrade.pnl) : '--' }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">PnL %</span>
              <span
                class="detail__value mono"
                :class="(selectedTrade.pnl_pct ?? 0) >= 0 ? 'text-success' : 'text-error'"
              >
                {{ selectedTrade.pnl_pct != null ? formatPct(selectedTrade.pnl_pct) : '--' }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Fee</span>
              <span class="detail__value mono">{{ formatCurrency(selectedTrade.fee) }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Entry Time</span>
              <span class="detail__value">{{ formatDateTime(selectedTrade.entry_time) }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Exit Time</span>
              <span class="detail__value">
                {{ selectedTrade.exit_time ? formatDateTime(selectedTrade.exit_time) : '--' }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Duration</span>
              <span class="detail__value">
                {{
                  selectedTrade.entry_time && selectedTrade.exit_time
                    ? formatDuration(
                        (new Date(selectedTrade.exit_time).getTime() -
                          new Date(selectedTrade.entry_time).getTime()) /
                          1000,
                      )
                    : '--'
                }}
              </span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Exchange</span>
              <span class="detail__value">{{ exchangeStore.label(selectedTrade.exchange) }}</span>
            </div>
            <div class="detail__field">
              <span class="detail__label">Bot</span>
              <span class="detail__value">{{ selectedTrade.bot_id ? botsStore.name(selectedTrade.bot_id) : (selectedTrade.is_backtest ? 'Backtest' : '--') }}</span>
            </div>
          </div>

          <div class="detail__notes">
            <label class="label" for="trade-notes">Notes</label>
            <textarea
              id="trade-notes"
              v-model="editingNotes"
              class="input detail__textarea"
              placeholder="Add notes about this trade..."
              rows="4"
            />
            <button
              class="btn btn-primary detail__save-btn"
              :disabled="isSavingNotes"
              @click="saveNotes"
            >
              {{ isSavingNotes ? 'Saving...' : 'Save Notes' }}
            </button>
          </div>
        </div>
      </Transition>
    </template>
  </div>
</template>

<style scoped>
/* One screen: the page never scrolls. Stats, bot table and calendar keep
   their size; the trade table takes the rest in its own card and scrolls
   inside (user, 2026-09-07). */
.journal {
  height: 100%;
  overflow: hidden;
  padding: 16px 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.journal__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  font-size: 14px;
}

.journal__top {
  display: flex;
  align-items: stretch;
  gap: 12px;
  flex-shrink: 0;
}

.journal__stats {
  flex: 1 1 auto;
  min-width: 0;
}

/* The Paper | Live switch: two segments, one lit. */
.mode-switch {
  display: flex;
  align-self: center;
  flex-shrink: 0;
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  overflow: hidden;
  background: var(--qa-bg-card);
}

.mode-switch__btn {
  width: 76px;
  padding: 8px 0;
  text-align: center;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.02em;
  border: none;
  background: transparent;
  color: var(--qa-text-muted);
  cursor: pointer;
  transition: all var(--qa-transition);
}

.mode-switch__btn + .mode-switch__btn {
  border-left: 1px solid var(--qa-border);
}

.mode-switch__btn:hover {
  color: var(--qa-text);
}

.mode-switch__btn.active {
  background: var(--qa-accent);
  color: var(--qa-bg);
}

.journal__breakdown {
  flex: 0 1 auto;
  min-height: 120px;
  max-height: 30%;
  overflow: auto;
}

/* The one calendar takes the full width — 53 weeks need it. */
.journal__calendar {
  flex-shrink: 0;
}

.calendar-card {
  padding: 10px 14px 12px;
  min-width: 0;
}

.calendar-card__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 8px;
}

.calendar-card__label {
  font-size: 12px;
  font-weight: 600;
  color: var(--qa-text);
}

.calendar-card__meta {
  font-size: 11px;
  color: var(--qa-text-muted);
}


.journal__table {
  flex: 1 1 auto;
  min-height: 120px;
  overflow: auto;
  padding: 0;
}

.empty-state {
  font-size: 13px;
}

/* Detail Panel — below the table, never past the screen */
.journal__detail {
  flex: 0 1 auto;
  min-height: 0;
  max-height: 40%;
  overflow: auto;
}

.detail__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.detail__title {
  font-size: 14px;
  font-weight: 600;
  color: var(--qa-text);
}

.detail__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}

.detail__field {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.detail__label {
  font-size: 11px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-muted);
}

.detail__value {
  font-size: 13px;
  color: var(--qa-text);
  font-weight: 500;
}

.detail__notes {
  border-top: 1px solid var(--qa-border-subtle);
  padding-top: 16px;
}

.detail__textarea {
  resize: vertical;
  min-height: 80px;
  margin-bottom: 8px;
  font-family: inherit;
  line-height: 1.5;
}

.detail__save-btn {
  margin-top: 4px;
}

/* Slide transition */
.slide-enter-active,
.slide-leave-active {
  transition: all 200ms ease;
}

.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
