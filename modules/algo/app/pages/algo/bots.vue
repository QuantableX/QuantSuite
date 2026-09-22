<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useBotsStore } from '#algo/stores/bots'
import { useStrategiesStore } from '#algo/stores/strategies'
import { useExchangeStore } from '#algo/stores/exchange'
import { formatCurrency, formatPnl, formatDuration } from '#algo/utils/format'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import type { BotSnapshot } from '#algo/types'

const router = useRouter()
const botsStore = useBotsStore()
const strategiesStore = useStrategiesStore()
const exchangeStore = useExchangeStore()

const showCreate = ref(false)
const busy = ref<Record<string, string>>({})
const actionError = ref<string | null>(null)

// Uptime ticks only while the page is visible (V3 warm cache).
const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | null = null
function ensureTicking() {
  ticker ??= setInterval(() => { now.value = Date.now() }, 1000)
}
onMounted(() => { if (inActiveKeepAliveTree()) ensureTicking() })
onActivated(() => {
  ensureTicking()
  void botsStore.load()
  void botsStore.loadPnlByBot()
})
onDeactivated(() => {
  if (ticker) { clearInterval(ticker); ticker = null }
})
onUnmounted(() => { if (ticker) clearInterval(ticker) })

function strategyName(id: string): string {
  return strategiesStore.strategies.find((s) => s.id === id)?.name ?? id.slice(0, 8)
}

function uptime(bot: BotSnapshot): string {
  if (bot.status !== 'running' || !bot.started_at) return '--'
  return formatDuration(Math.max(0, (now.value - new Date(bot.started_at).getTime()) / 1000))
}

function statusClass(bot: BotSnapshot): string {
  return bot.status === 'running' ? 'dot--running' : bot.status === 'error' ? 'dot--error' : 'dot--stopped'
}

async function run(bot: BotSnapshot, action: string, fn: () => Promise<unknown>) {
  busy.value = { ...busy.value, [bot.id]: action }
  actionError.value = null
  try {
    await fn()
  } catch (err) {
    actionError.value = `${bot.name}: ${String(err)}`
  } finally {
    const next = { ...busy.value }
    delete next[bot.id]
    busy.value = next
  }
}

// Live bots place real orders: every action that can touch the account asks once.
function liveVenue(bot: BotSnapshot): string {
  const exchange = exchangeStore.byId(bot.exchange_id)
  return exchange ? `${exchange.name}${exchange.sandbox ? ' (sandbox)' : ''}` : 'the exchange'
}

function start(bot: BotSnapshot) {
  if (bot.trading_mode === 'live') {
    const quote = bot.pair.split('/')[1] ?? ''
    const confirmed = window.confirm(
      `Start "${bot.name}" LIVE on ${liveVenue(bot)}? It places real market orders for ${bot.pair} with up to ${bot.budget} ${quote}.`,
    )
    if (!confirmed) return Promise.resolve()
  }
  return run(bot, 'start', () => botsStore.start(bot.id))
}
function stop(bot: BotSnapshot) {
  if (bot.trading_mode === 'live' && bot.open_positions > 0) {
    const confirmed = window.confirm(
      `Stop "${bot.name}"? Its ${bot.open_positions} live position(s) stay on ${liveVenue(bot)} unmanaged. Use "Close positions" first to sell them.`,
    )
    if (!confirmed) return Promise.resolve()
  }
  return run(bot, 'stop', () => botsStore.stop(bot.id))
}
function closePositions(bot: BotSnapshot) {
  if (bot.trading_mode === 'live') {
    const confirmed = window.confirm(
      `Sell the ${bot.open_positions} open position(s) of "${bot.name}" at market on ${liveVenue(bot)}?`,
    )
    if (!confirmed) return Promise.resolve()
  }
  return run(bot, 'close', () => botsStore.closePositions(bot.id))
}
async function remove(bot: BotSnapshot) {
  const confirmed = window.confirm(`Delete "${bot.name}"? Its journal stays; the bot cannot be started again.`)
  if (!confirmed) return
  await run(bot, 'delete', () => botsStore.remove(bot.id))
}

function onCreated(_bot: unknown, started: boolean) {
  showCreate.value = false
  void botsStore.loadPnlByBot()
  if (started) void botsStore.load()
}
</script>

<template>
  <div class="bots-page">
    <div class="bots-page__header">
      <div>
        <h2 class="bots-page__title">Bots</h2>
        <p class="bots-page__subtitle text-muted">
          {{ botsStore.runningCount }} running
          <template v-if="botsStore.liveRunningCount"> · {{ botsStore.liveRunningCount }} live</template>
          · {{ botsStore.list.length }} total
        </p>
      </div>
      <div class="bots-page__actions">
        <button v-if="botsStore.runningCount" class="btn btn-sm" @click="botsStore.stopAll()">Stop all</button>
        <button class="btn btn-primary" @click="showCreate = true">New Bot</button>
      </div>
    </div>

    <div v-if="actionError" class="bots-page__error card">
      <span class="text-error">{{ actionError }}</span>
      <button class="btn btn-sm" @click="actionError = null">Dismiss</button>
    </div>

    <div v-if="!botsStore.list.length" class="bots-page__empty card">
      <p class="empty__title">No bots yet</p>
      <p class="empty__desc text-muted">
        Choose a strategy, exchange, and budget to create a bot.
      </p>
      <button class="btn btn-primary" @click="showCreate = true">New bot</button>
    </div>

    <div v-else class="card bots-table-wrap">
      <table class="table bots-table">
        <thead>
          <tr>
            <th>Bot</th>
            <th>Strategy</th>
            <th>Exchange</th>
            <th>Pair</th>
            <th>TF</th>
            <th>Mode</th>
            <th class="num">Equity</th>
            <th class="num">Cash</th>
            <th class="num">PnL</th>
            <th class="num">Open</th>
            <th>Uptime</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="bot in botsStore.list" :key="bot.id" :class="{ 'row--error': bot.status === 'error' }">
            <td class="cell-name">
              <div class="name-line">
                <span class="dot" :class="statusClass(bot)" />
                <span class="name">{{ bot.name }}</span>
              </div>
              <div v-if="bot.status === 'error' && bot.last_error" class="name-error">{{ bot.last_error }}</div>
            </td>
            <td>{{ strategyName(bot.strategy_id) }}</td>
            <td>{{ exchangeStore.byId(bot.exchange_id)?.name ?? '--' }}</td>
            <td class="mono">{{ bot.pair }}</td>
            <td class="mono">{{ bot.timeframe }}</td>
            <td>
              <span class="mode" :class="bot.trading_mode === 'live' ? 'mode--live' : 'mode--paper'">
                {{ bot.trading_mode }}
              </span>
            </td>
            <td class="num mono">
              {{ bot.status === 'running' ? formatCurrency(bot.equity) : formatCurrency(bot.budget) }}
              <span v-if="bot.status !== 'running'" class="text-muted small">budget</span>
            </td>
            <td class="num mono">{{ bot.status === 'running' ? formatCurrency(bot.balance) : '--' }}</td>
            <td class="num mono" :class="formatPnl(botsStore.pnlByBot[bot.id] ?? 0).class">
              {{ formatPnl(botsStore.pnlByBot[bot.id] ?? 0).text }}
            </td>
            <td class="num mono">
              {{ bot.status === 'running' ? bot.open_positions : '--' }}
            </td>
            <td class="mono">{{ uptime(bot) }}</td>
            <td class="cell-actions">
              <template v-if="bot.status === 'running'">
                <button
                  v-if="bot.open_positions > 0"
                  class="btn btn-sm"
                  :disabled="!!busy[bot.id]"
                  @click="closePositions(bot)"
                >
                  {{ busy[bot.id] === 'close' ? 'Closing…' : 'Close positions' }}
                </button>
                <button class="btn btn-sm btn-danger" :disabled="!!busy[bot.id]" @click="stop(bot)">
                  {{ busy[bot.id] === 'stop' ? 'Stopping…' : 'Stop' }}
                </button>
              </template>
              <template v-else>
                <button class="btn btn-sm btn-success" :disabled="!!busy[bot.id]" @click="start(bot)">
                  {{ busy[bot.id] === 'start' ? 'Starting…' : 'Start' }}
                </button>
                <button class="btn btn-sm" :disabled="!!busy[bot.id]" title="Delete" @click="remove(bot)">
                  {{ busy[bot.id] === 'delete' ? '…' : 'Delete' }}
                </button>
              </template>
              <button class="btn btn-sm" title="Terminal" @click="router.push({ path: '/algo/terminal', query: { bot: bot.id } })">Log</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <AlgoBotCreateModal :visible="showCreate" @close="showCreate = false" @created="onCreated" />
  </div>
</template>

<style scoped>
.bots-page {
  height: 100%;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.bots-page__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.bots-page__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--qa-text);
}

.bots-page__subtitle {
  font-size: 13px;
  margin-top: 4px;
}

.bots-page__actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.bots-page__error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
}

.bots-page__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 48px 24px;
  text-align: center;
}

.empty__title {
  font-size: 15px;
  font-weight: 600;
  color: var(--qa-text);
}

.empty__desc {
  font-size: 13px;
  max-width: 420px;
  line-height: 1.5;
}

.bots-table-wrap {
  padding: 0;
  overflow-x: auto;
}

.bots-table th.num,
.bots-table td.num {
  text-align: right;
}

.row--error td {
  background: color-mix(in srgb, var(--qa-error) 6%, transparent);
}

.cell-name {
  min-width: 180px;
}

.name-line {
  display: flex;
  align-items: center;
  gap: 8px;
}

.name {
  font-weight: 600;
  color: var(--qa-text);
}

.name-error {
  margin-top: 2px;
  font-size: 11px;
  color: var(--qa-error);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot--running {
  background: var(--qa-success);
  animation: pulse-dot 2s ease-in-out infinite;
}

.dot--stopped {
  background: var(--qa-text-muted);
}

.dot--error {
  background: var(--qa-error);
}

.mode {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid var(--qa-border);
  color: var(--qa-text-secondary);
}

.mode--live {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 45%, transparent);
}

.small {
  font-size: 10px;
  margin-left: 4px;
}

.cell-actions {
  white-space: nowrap;
  text-align: right;
}

.cell-actions .btn {
  margin-left: 4px;
}
</style>
