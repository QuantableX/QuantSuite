<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useBotsStore } from '#algo/stores/bots'
import { useStrategiesStore } from '#algo/stores/strategies'
import { useExchangeStore } from '#algo/stores/exchange'
import { formatCurrency, formatPnl, formatTime } from '#algo/utils/format'
import type { LogEntry, TradeStats } from '#algo/types'

const router = useRouter()
const botsStore = useBotsStore()
const strategiesStore = useStrategiesStore()
const exchangeStore = useExchangeStore()

const showCreate = ref(false)

// Paper and live side by side (PLAN-QUANTALGO §3.5) — never added up.
const pnlRows = computed(() => [
  { label: 'Today', paper: botsStore.pnlPaper.today, live: botsStore.pnlLive.today },
  { label: '7 days', paper: botsStore.pnlPaper.week, live: botsStore.pnlLive.week },
  { label: '30 days', paper: botsStore.pnlPaper.month, live: botsStore.pnlLive.month },
  { label: 'All time', paper: botsStore.pnlPaper.all, live: botsStore.pnlLive.all },
])

function botStats(id: string): TradeStats | null {
  return botsStore.statsByBot[id] ?? null
}

function winRate(stats: TradeStats | null): string {
  if (!stats || stats.total_trades === 0) return '--'
  return `${stats.win_rate.toFixed(0)}%`
}

const recentLogs = computed(() => botsStore.recentLogs.slice(-40).reverse())

function logLevelClass(level: LogEntry['level']): string {
  switch (level) {
    case 'trade': return 'text-accent'
    case 'warn': return 'text-warning'
    case 'error': return 'text-error'
    default: return ''
  }
}

function strategyName(id: string): string {
  return strategiesStore.strategies.find((s) => s.id === id)?.name ?? id.slice(0, 8)
}

function navigateTo(path: string) {
  router.push(path)
}

// "Stop all trading" (PLAN-AUTO-EXECUTION §2.6): stops every bot, disarms
// Manual execution, cancels QuantSuite's open orders — never sells.
interface KillReport { on: boolean; bots_stopped: number; systems_disarmed: number; orders_cancelled: string[]; errors: string[]; left_on_exchange: string[] }
const killOn = ref(false)
const killConfirm = ref(false)
const killBusy = ref(false)
const killReport = ref<KillReport | null>(null)
const killError = ref<string | null>(null)

async function loadKillSwitch() {
  try {
    killOn.value = await invoke<boolean>('plugin:algo|trading_kill_switch_status')
    notices.value = await invoke<{ imported: { from: string; at: string } | null; moved: { at: string } | null }>('plugin:algo|trading_notices')
  } catch (e) {
    killError.value = String(e)
  }
}

// Migration (PLAN-MIGRATION §2.2): an import arrived here, or this machine moved away.
const notices = ref<{ imported: { from: string; at: string } | null; moved: { at: string } | null }>({ imported: null, moved: null })

async function dismissImported() {
  await invoke('plugin:algo|trading_notice_dismiss')
  notices.value = { ...notices.value, imported: null }
}

async function setKillSwitch(on: boolean) {
  killBusy.value = true
  killError.value = null
  try {
    const report = await invoke<KillReport>('plugin:algo|trading_kill_switch', { on })
    killOn.value = report.on
    killReport.value = on ? report : null
    killConfirm.value = false
    await botsStore.load()
  } catch (e) {
    killError.value = String(e)
  } finally {
    killBusy.value = false
  }
}

// Heartbeats: the feed's last tick per running bot; the age ticks while
// the page is shown and stops with it (warm cache).
const heartbeats = ref<Record<string, string>>({})
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null
let unlistenBeat: UnlistenFn | null = null

function beatAge(id: string): string {
  const at = heartbeats.value[id]
  if (!at) return ''
  const s = Math.max(0, Math.round((now.value - Date.parse(at)) / 1000))
  return s < 120 ? `${s}s` : `${Math.round(s / 60)}m`
}

onActivated(async () => {
  void botsStore.load()
  void loadKillSwitch()
  try {
    heartbeats.value = await invoke<Record<string, string>>('plugin:algo|bot_heartbeats')
  } catch { /* the column fills with the next tick */ }
  unlistenBeat = await listen<{ bot_id: string; at: string }>('bot:heartbeat', (e) => {
    heartbeats.value = { ...heartbeats.value, [e.payload.bot_id]: e.payload.at }
  })
  clock = setInterval(() => { now.value = Date.now() }, 5000)
})

onDeactivated(() => {
  if (clock) clearInterval(clock)
  clock = null
  unlistenBeat?.()
  unlistenBeat = null
})
</script>

<template>
  <div class="dashboard">
    <QPageHeading class="dashboard__heading" title="Trading overview">
      <div class="kill">
        <template v-if="killOn">
          <span class="kill__on">Stop all trading is on</span>
          <button class="btn btn-sm" :disabled="killBusy" @click="setKillSwitch(false)">Allow trading again</button>
        </template>
        <template v-else-if="killConfirm">
          <span class="kill__ask">Stop every bot, disarm Manual execution and cancel open orders? Nothing is sold.</span>
          <button class="btn btn-sm" :disabled="killBusy" @click="killConfirm = false">Cancel</button>
          <button class="btn btn-sm kill__btn" :disabled="killBusy" @click="setKillSwitch(true)">{{ killBusy ? 'Stopping…' : 'Stop all trading' }}</button>
        </template>
        <button v-else class="btn btn-sm kill__btn" @click="killConfirm = true">Stop all trading</button>
      </div>
    </QPageHeading>
    <div v-if="notices.moved" class="kill__report text-warning">
      This installation moved to another machine on {{ notices.moved.at.slice(0, 10) }}: trading stays off here. Turn "Stop all trading" off only if you mean to trade from this machine again.
    </div>
    <div v-if="notices.imported" class="kill__report">
      Imported from {{ notices.imported.from }} on {{ notices.imported.at.slice(0, 10) }}: bots are stopped and Manual execution is disarmed. Arm trading on this machine when ready.
      <button class="btn btn-sm" @click="dismissImported">OK</button>
    </div>
    <div v-if="killError" class="kill__report text-error">{{ killError }}</div>
    <div v-if="killReport" class="kill__report">
      {{ killReport.bots_stopped }} bot(s) stopped, {{ killReport.systems_disarmed }} system(s) disarmed, {{ killReport.orders_cancelled.length }} order(s) cancelled.
      Nothing was sold{{ killReport.left_on_exchange.length ? `; still held: ${killReport.left_on_exchange.join(', ')}` : '' }}.
      <span v-if="killReport.errors.length" class="text-warning"> {{ killReport.errors.join('; ') }}</span>
    </div>
    <div class="dashboard__top">
      <!-- Bots at a glance -->
      <div class="card card--bots">
        <div class="card__head">
          <h3 class="card__title">Bots</h3>
          <div class="card__buttons">
            <button class="btn btn-sm btn-primary" @click="showCreate = true">New bot</button>
            <button class="btn btn-sm" @click="navigateTo('/algo/bots')">All bots</button>
          </div>
        </div>
        <div class="bots-glance">
          <div class="stat-strip">
            <div class="stat">
              <span class="stat__value">{{ botsStore.runningCount }}</span>
              <span class="stat__label">running</span>
            </div>
            <div class="stat">
              <span class="stat__value">{{ botsStore.liveRunningCount }}</span>
              <span class="stat__label">live</span>
            </div>
            <div class="stat">
              <span class="stat__value">{{ botsStore.list.length }}</span>
              <span class="stat__label">total</span>
            </div>
          </div>
          <div class="kv-grid">
            <div class="kv">
              <span class="kv__label">Equity · paper</span>
              <span class="kv__value mono">{{ formatCurrency(botsStore.equityByMode.paper) }}</span>
            </div>
            <div class="kv">
              <span class="kv__label">Equity · live</span>
              <span class="kv__value mono">{{ formatCurrency(botsStore.equityByMode.live) }}</span>
            </div>
            <div class="kv">
              <span class="kv__label">Open · paper</span>
              <span class="kv__value mono">{{ botsStore.openByMode.paper }}</span>
            </div>
            <div class="kv">
              <span class="kv__label">Open · live</span>
              <span class="kv__value mono">{{ botsStore.openByMode.live }}</span>
            </div>
            <div class="kv">
              <span class="kv__label">Exchanges</span>
              <span class="kv__value mono">{{ exchangeStore.exchanges.length }}</span>
            </div>
            <div class="kv">
              <span class="kv__label">Strategies</span>
              <span class="kv__value mono">{{ strategiesStore.strategies.length }}</span>
            </div>
          </div>
        </div>
        <div class="exposure-mini">
          <span class="kv__label">Exposure per venue</span>
          <AlgoExchangeExposurePanel compact />
        </div>
        <div class="exposure-mini">
          <AlgoExchangePerpPositions />
        </div>
      </div>

      <!-- Realised PnL, paper and live apart -->
      <div class="card card--pnl">
        <div class="card__head">
          <h3 class="card__title">Profit &amp; Loss</h3>
          <span class="card__hint">Realised</span>
        </div>
        <table class="pnl-table">
          <thead>
            <tr>
              <th></th>
              <th class="num">Paper</th>
              <th class="num">Live</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in pnlRows" :key="row.label">
              <td class="pnl-table__label">{{ row.label }}</td>
              <td class="num mono" :class="formatPnl(row.paper).class">{{ formatPnl(row.paper).text }}</td>
              <td class="num mono" :class="formatPnl(row.live).class">{{ formatPnl(row.live).text }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Every bot, with its own numbers -->
    <div class="card card--list">
      <div class="card__head">
        <h3 class="card__title">
          Every bot
          <span v-if="botsStore.list.length" class="pill">{{ botsStore.list.length }}</span>
        </h3>
        <button class="btn btn-sm" @click="navigateTo('/algo/bots')">Manage</button>
      </div>
      <div v-if="botsStore.list.length" class="bots-table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>Bot</th>
              <th>Strategy</th>
              <th>Pair</th>
              <th>Timeframe</th>
              <th>Mode</th>
              <th>Status</th>
              <th class="num">Equity</th>
              <th class="num">Open</th>
              <th class="num">Trades</th>
              <th class="num">Win rate</th>
              <th class="num">PnL</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="bot in botsStore.list" :key="bot.id" class="bot-row" @click="navigateTo('/algo/bots')">
              <td class="bot-name">{{ bot.name }}</td>
              <td>{{ strategyName(bot.strategy_id) }}</td>
              <td class="mono">{{ bot.pair }}</td>
              <td class="mono">{{ bot.timeframe }}</td>
              <td><span class="mode-tag" :class="`mode-tag--${bot.trading_mode}`">{{ bot.trading_mode }}</span></td>
              <td>
                <span class="dot" :class="`dot--${bot.status}`" />
                {{ bot.status }}
                <span v-if="bot.status === 'running' && beatAge(bot.id)" class="beat" :title="`Last feed tick ${heartbeats[bot.id]}`">· {{ beatAge(bot.id) }}</span>
              </td>
              <td class="num mono">{{ bot.status === 'running' ? formatCurrency(bot.equity) : '--' }}</td>
              <td class="num mono">{{ bot.status === 'running' ? bot.open_positions : '--' }}</td>
              <td class="num mono">{{ botStats(bot.id)?.total_trades ?? 0 }}</td>
              <td class="num mono">{{ winRate(botStats(bot.id)) }}</td>
              <td class="num mono" :class="formatPnl(botsStore.pnlByBot[bot.id] ?? 0).class">
                {{ formatPnl(botsStore.pnlByBot[bot.id] ?? 0).text }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <QEmptyState v-else class="dashboard__empty" compact title="No bots yet">
        <button class="btn btn-primary" @click="showCreate = true">New bot</button>
      </QEmptyState>
    </div>

    <!-- What just happened -->
    <div class="card card--list">
      <div class="card__head">
        <h3 class="card__title">Recent activity</h3>
        <div class="card__buttons">
          <button class="btn btn-sm" @click="navigateTo('/algo/backtest')">Backtest</button>
          <button class="btn btn-sm" @click="navigateTo('/algo/strategies')">Strategies</button>
        </div>
      </div>
      <div v-if="recentLogs.length" class="activity-list">
        <div v-for="(log, idx) in recentLogs" :key="idx" class="activity-entry">
          <span class="activity-entry__time text-muted">{{ formatTime(log.timestamp) }}</span>
          <span v-if="log.bot_id" class="activity-entry__bot">{{ botsStore.name(log.bot_id) }}</span>
          <span class="activity-entry__msg" :class="logLevelClass(log.level)">{{ log.message }}</span>
        </div>
      </div>
      <p v-else class="empty-state text-muted">No recent activity</p>
    </div>

    <AlgoBotCreateModal :visible="showCreate" @close="showCreate = false" @created="showCreate = false" />
  </div>
</template>

<style scoped>
/* The glance row takes what it needs, the two lists share the rest and scroll
   inside their panels. The page only scrolls once a short window hits the floors. */
.dashboard {
  container: algo-overview / inline-size;
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

.dashboard .dashboard__heading {
  flex-shrink: 0;
  margin-bottom: 0;
}

.dashboard .dashboard__empty {
  flex: 1;
  min-height: 0;
  padding: var(--qss-gap-tight);
}

.exposure-mini {
  margin-top: var(--qss-gap-tight);
  padding-top: var(--qss-gap-tight);
  border-top: 1px solid var(--qa-border-subtle);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.exposure-mini:empty {
  display: none;
}

.kill {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
}

.kill__btn {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 45%, var(--qa-border));
}

.kill__on {
  font-size: var(--qss-text-label);
  font-weight: 600;
  color: var(--qa-error);
}

.kill__ask {
  font-size: var(--qss-text-label);
  color: var(--qa-text-secondary);
}

.kill__report {
  font-size: var(--qss-text-label);
  color: var(--qa-text-secondary);
  flex-shrink: 0;
}

.beat {
  color: var(--qa-text-muted);
  font-size: 11px;
}

.dashboard__top {
  flex: 0 1 auto;
  min-height: 0;
  max-height: 42%;
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
  grid-auto-rows: minmax(0, 1fr);
  gap: var(--qss-gap);
}

.dashboard .card {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  overflow: auto;
}

.dashboard .card--list {
  flex: 1 1 0;
  min-height: 176px;
  overflow: hidden;
}

.card__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap-tight);
  min-height: var(--qss-ctl-h);
  margin-bottom: var(--qss-gap-tight);
  flex-shrink: 0;
}

.card__title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  margin: 0;
}

.card__hint {
  font-size: var(--qss-text-meta);
  color: var(--qa-text-muted);
}

.card__buttons {
  display: flex;
  gap: 6px;
}

/* Bots card: the counts beside their details */
.bots-glance {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: var(--qss-gap) 28px;
}

.stat-strip {
  display: flex;
  gap: 24px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.stat__value {
  font-size: var(--qss-text-metric);
  line-height: 1.1;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  letter-spacing: -.02em;
  color: var(--qa-text);
}

.stat__label {
  font-size: var(--qss-text-meta);
  color: var(--qa-text-muted);
}

.stat__label::first-letter {
  text-transform: uppercase;
}

.kv-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 24px;
}

.kv {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--qss-gap-tight);
  font-size: var(--qss-text-label);
}

.kv__label {
  color: var(--qa-text-muted);
  font-size: var(--qss-text-label);
  white-space: nowrap;
}

.kv__value {
  color: var(--qa-text);
  font-weight: 500;
}

/* PnL card */
.pnl-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}

.pnl-table th {
  padding: 0 0 4px;
  font-size: var(--qss-text-meta);
  font-weight: 500;
  color: var(--qa-text-muted);
  text-align: left;
}

.pnl-table th.num,
.pnl-table td.num {
  text-align: right;
}

.pnl-table td {
  height: 26px;
  padding: 0;
  border-top: 1px solid var(--qa-border-subtle);
}

.pnl-table__label {
  color: var(--qa-text-secondary);
}

/* Bot table */
.bots-table-wrap {
  flex: 1 1 0;
  min-height: 0;
  overflow: auto;
}

.bots-table-wrap .table th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qss-bg-raised);
}

.bots-table-wrap .table td {
  white-space: nowrap;
}

.table th.num,
.table td.num {
  text-align: right;
  white-space: nowrap;
}

.bot-row {
  cursor: pointer;
}

.bot-name {
  font-weight: 600;
  color: var(--qa-text);
  white-space: nowrap;
}

.mode-tag {
  display: inline-block;
  padding: 0 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  text-transform: capitalize;
  color: var(--qa-text-secondary);
  border: 1px solid var(--qa-border-subtle);
}

.mode-tag--live {
  color: var(--qa-text);
  border-color: var(--qa-border);
}

.dot {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  margin-right: 6px;
  background: var(--qa-text-muted);
}

.dot--running {
  background: var(--qa-success);
}

.dot--error {
  background: var(--qa-error);
}

/* Activity */
.activity-list {
  flex: 1 1 0;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.activity-entry {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 4px 0;
  border-bottom: 1px solid var(--qa-border-subtle);
  font-size: var(--qss-text-label);
}

.activity-entry:last-child {
  border-bottom: none;
}

.activity-entry__time {
  font-size: 11px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', monospace;
  flex-shrink: 0;
}

.activity-entry__bot {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--qa-text-secondary);
}

.activity-entry__msg {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty-state {
  font-size: var(--qss-text-label);
  padding: var(--qss-gap-tight) 0;
}

/* Tighter before the counts have to go above their details. */
@container algo-overview (max-width: 1000px) {
  .dashboard__top {
    grid-template-columns: minmax(0, 5fr) minmax(0, 3fr);
  }

  .bots-glance {
    column-gap: var(--qss-gap);
  }

  .stat-strip {
    gap: var(--qss-gap);
  }

  .kv-grid {
    column-gap: var(--qss-gap);
  }
}

/* The counts go above their details before the details get cramped. */
@container algo-overview (max-width: 800px) {
  .bots-glance {
    grid-template-columns: minmax(0, 1fr);
    gap: var(--qss-gap-tight);
  }
}

@container algo-overview (max-width: 680px) {
  .dashboard__top {
    flex: none;
    max-height: none;
    grid-template-columns: minmax(0, 1fr);
    grid-auto-rows: auto;
  }
}
</style>
