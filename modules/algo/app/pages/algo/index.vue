<script setup lang="ts">
definePageMeta({ layout: 'algo' })

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

onActivated(() => {
  void botsStore.load()
})
</script>

<template>
  <div class="dashboard">
    <QPageHeading title="Trading overview" />
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
        <div class="stat-strip">
          <div class="stat">
            <span class="stat__value mono">{{ botsStore.runningCount }}</span>
            <span class="stat__label">running</span>
          </div>
          <div class="stat">
            <span class="stat__value mono">{{ botsStore.liveRunningCount }}</span>
            <span class="stat__label">live</span>
          </div>
          <div class="stat">
            <span class="stat__value mono">{{ botsStore.list.length }}</span>
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
    <div class="card card--table">
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
              <td class="mono">{{ bot.pair }} · {{ bot.timeframe }}</td>
              <td><span class="mode-tag" :class="`mode-tag--${bot.trading_mode}`">{{ bot.trading_mode }}</span></td>
              <td>
                <span class="dot" :class="`dot--${bot.status}`" />
                {{ bot.status }}
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
      <QEmptyState v-else compact title="No bots yet">
        <button class="btn btn-primary" @click="showCreate = true">New bot</button>
      </QEmptyState>
    </div>

    <!-- What just happened -->
    <div class="card card--activity">
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
/* One screen, no page scroll: the top cards take what they need, the bot
   table gets the rest, the activity strip keeps a bounded share. */
.dashboard {
  height: 100%;
  overflow: hidden;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.dashboard__top {
  flex: 0 0 auto;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.dashboard .card {
  padding: 12px 16px 14px;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.card--table {
  flex: 1 1 auto;
}

.card--activity {
  flex: 0 1 auto;
  max-height: 26%;
}

.card__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
  flex-shrink: 0;
}

.card__title {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--qa-text-secondary);
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
}

.card__hint {
  font-size: 11px;
  color: var(--qa-text-muted);
}

.card__buttons {
  display: flex;
  gap: 6px;
}

/* Bots card */
.stat-strip {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-bottom: 12px;
}

.stat {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 8px 12px;
  background: var(--qa-bg-hover);
  border: 1px solid var(--qa-border-subtle);
  border-radius: var(--qa-radius);
  min-width: 0;
}

.stat__value {
  font-size: 20px;
  line-height: 1;
  font-weight: 600;
  color: var(--qa-text);
}

.stat__label {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qa-text-muted);
}

.kv-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 5px 28px;
}

.kv {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
  font-size: 12px;
}

.kv__label {
  color: var(--qa-text-muted);
}

.kv__value {
  color: var(--qa-text);
  font-weight: 500;
}

/* PnL card */
.pnl-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.pnl-table th {
  padding: 0 0 6px;
  font-size: 10px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qa-text-muted);
  text-align: left;
}

.pnl-table th.num,
.pnl-table td.num {
  text-align: right;
}

.pnl-table td {
  padding: 5px 0;
  border-top: 1px solid var(--qa-border-subtle);
}

.pnl-table__label {
  color: var(--qa-text-secondary);
}

/* Bot table */
.bots-table-wrap {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
}

.table th {
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
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
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
  flex: 1 1 auto;
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
  font-size: 12px;
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
  font-size: 13px;
  padding: 8px 0;
}

@media (max-width: 1000px) {
  .dashboard {
    overflow-y: auto;
  }

  .dashboard__top {
    grid-template-columns: 1fr;
  }

  .card--activity {
    max-height: none;
  }
}
</style>
