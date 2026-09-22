import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ref, computed } from 'vue'
import type {
  Bot,
  BotSnapshot,
  BotDraft,
  LogEntry,
  TradeStats,
  TradeStatsBreakdown,
  TradingMode,
  PreflightResult,
  BotLogEvent,
  BotTradeEvent,
  BotStatusEvent,
  BotEquityEvent,
  BotErrorEvent,
} from '#algo/types'

const MAX_LOG_ENTRIES = 1000

// Every window the summary shows is the user's wall clock — the day rolls over
// at local midnight (scheduleDayRollover below) and the journal heatmap keys
// its cells the same way. The trade timestamps are UTC, so the boundaries are
// local midnights turned into instants, never date strings.
function startOfDaysAgo(days: number): number {
  const d = new Date()
  d.setDate(d.getDate() - days)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

// `exit_time` is TEXT and the backend compares it lexicographically, so the
// bound has to be shaped like the values: `Utc::now().to_rfc3339()` writes
// `+00:00`, not `Z`, and carries fractional seconds.
function utcBound(instant: number): string {
  return `${new Date(instant).toISOString().slice(0, 19)}+00:00`
}

export interface PnlSummary {
  today: number
  week: number
  month: number
  all: number
}

export interface BotError {
  botId: string | null
  message: string
}

/**
 * Every bot the module knows (PLAN-QUANTALGO §3): the rows from `list_bots`
 * kept current by the per-bot events. One store for the Bots page, the
 * dashboard, the header and the per-bot terminal/charts/journal filters.
 */
export const useBotsStore = defineStore('algo/bots', () => {
  // ── State ──

  const bots = ref<Record<string, BotSnapshot>>({})
  /** Realised statistics per mode and per bot (all time), from the journal. */
  const breakdown = ref<TradeStatsBreakdown | null>(null)
  const statsByBot = computed<Record<string, TradeStats>>(() =>
    Object.fromEntries(
      (breakdown.value?.bots ?? []).filter((b) => b.bot_id).map((b) => [b.bot_id as string, b.stats]),
    ),
  )
  /** Realised PnL per bot (all time). */
  const pnlByBot = computed<Record<string, number>>(() =>
    Object.fromEntries(Object.entries(statsByBot.value).map(([id, s]) => [id, s.total_pnl])),
  )
  const recentLogs = ref<LogEntry[]>([])
  const lastError = ref<BotError | null>(null)
  const pnl = ref<PnlSummary>({ today: 0, week: 0, month: 0, all: 0 })
  const pnlPaper = ref<PnlSummary>({ today: 0, week: 0, month: 0, all: 0 })
  const pnlLive = ref<PnlSummary>({ today: 0, week: 0, month: 0, all: 0 })
  const isLoading = ref(false)
  const isInitialized = ref(false)

  let initGen = 0
  let unlistenHandlers: UnlistenFn[] = []
  let rolloverTimer: ReturnType<typeof setTimeout> | null = null

  // ── Getters ──

  /** Running first, live before paper, then by name. */
  const list = computed<BotSnapshot[]>(() =>
    Object.values(bots.value).sort((a, b) => {
      const ra = a.status === 'running' ? 0 : a.status === 'error' ? 1 : 2
      const rb = b.status === 'running' ? 0 : b.status === 'error' ? 1 : 2
      if (ra !== rb) return ra - rb
      if (a.trading_mode !== b.trading_mode) return a.trading_mode === 'live' ? -1 : 1
      return a.name.localeCompare(b.name)
    }),
  )
  const running = computed(() => list.value.filter((b) => b.status === 'running'))
  const runningCount = computed(() => running.value.length)
  const liveRunningCount = computed(() => running.value.filter((b) => b.trading_mode === 'live').length)
  const totalEquity = computed(() => running.value.reduce((sum, b) => sum + b.equity, 0))
  const totalOpenPositions = computed(() => running.value.reduce((sum, b) => sum + b.open_positions, 0))
  /** Running bots' equity and open positions, paper and live apart. */
  const equityByMode = computed(() => ({
    paper: running.value.filter((b) => b.trading_mode !== 'live').reduce((sum, b) => sum + b.equity, 0),
    live: running.value.filter((b) => b.trading_mode === 'live').reduce((sum, b) => sum + b.equity, 0),
  }))
  const openByMode = computed(() => ({
    paper: running.value.filter((b) => b.trading_mode !== 'live').reduce((sum, b) => sum + b.open_positions, 0),
    live: running.value.filter((b) => b.trading_mode === 'live').reduce((sum, b) => sum + b.open_positions, 0),
  }))
  /** Whether anything live exists to show a live column for. */
  const hasLive = computed(() =>
    list.value.some((b) => b.trading_mode === 'live') || (breakdown.value?.live.total_trades ?? 0) > 0,
  )
  const hasError = computed(() => list.value.some((b) => b.status === 'error'))

  function byId(id: string | null | undefined): BotSnapshot | null {
    if (!id) return null
    return bots.value[id] ?? null
  }

  /** A bot's name for a `bot_id`; the short id when the row is gone. */
  function name(id: string | null | undefined): string {
    if (!id) return '--'
    return bots.value[id]?.name ?? id.slice(0, 8)
  }

  // ── Actions ──

  async function load() {
    isLoading.value = true
    try {
      const rows = await invoke<BotSnapshot[]>('plugin:algo|list_bots')
      const next: Record<string, BotSnapshot> = {}
      for (const row of rows) next[row.id] = row
      bots.value = next
    } catch (err) {
      console.error('[bots store] Failed to load bots:', err)
    } finally {
      isLoading.value = false
    }
  }

  // One call for every bot and both modes (PLAN-QUANTALGO §3.4).
  async function loadPnlByBot() {
    try {
      breakdown.value = await invoke<TradeStatsBreakdown>('plugin:algo|get_trade_stats_breakdown', {
        filters: { is_backtest: false },
      })
    } catch (err) {
      console.error('[bots store] Failed to load the per-bot statistics:', err)
    }
  }

  // A closed trade changes the per-bot table; one reload after a burst.
  let breakdownTimer: ReturnType<typeof setTimeout> | null = null
  function scheduleBreakdownReload() {
    if (breakdownTimer) clearTimeout(breakdownTimer)
    breakdownTimer = setTimeout(() => {
      breakdownTimer = null
      if (isInitialized.value) void loadPnlByBot()
    }, 1000)
  }

  async function create(draft: BotDraft): Promise<Bot> {
    try {
      const bot = await invoke<Bot>('plugin:algo|create_bot', { draft })
      await load()
      return bot
    } catch (err) {
      console.error('[bots store] Failed to create bot:', err)
      throw err
    }
  }

  async function start(botId: string): Promise<Bot> {
    try {
      const bot = await invoke<Bot>('plugin:algo|start_bot', { botId })
      lastError.value = null
      await load()
      return bot
    } catch (err) {
      lastError.value = { botId, message: String(err) }
      console.error('[bots store] Failed to start bot:', err)
      throw err
    }
  }

  async function stop(botId: string) {
    try {
      await invoke('plugin:algo|stop_bot', { botId })
      await load()
    } catch (err) {
      console.error('[bots store] Failed to stop bot:', err)
      throw err
    }
  }

  async function stopAll() {
    try {
      await invoke('plugin:algo|stop_all_bots')
      await load()
    } catch (err) {
      console.error('[bots store] Failed to stop bots:', err)
      throw err
    }
  }

  async function remove(botId: string) {
    try {
      await invoke('plugin:algo|delete_bot', { botId })
      const next = { ...bots.value }
      delete next[botId]
      bots.value = next
    } catch (err) {
      console.error('[bots store] Failed to delete bot:', err)
      throw err
    }
  }

  async function closePositions(botId: string): Promise<number> {
    try {
      return await invoke<number>('plugin:algo|close_bot_positions', { botId })
    } catch (err) {
      console.error('[bots store] Failed to close positions:', err)
      throw err
    }
  }

  /** The deploy preflight on a draft (create modal) or an existing bot. */
  async function preflight(draft: BotDraft | null, botId: string | null = null): Promise<PreflightResult> {
    return invoke<PreflightResult>('plugin:algo|validate_bot_deploy', {
      botId,
      draft,
    })
  }

  async function loadLogs(botId: string | null = null, limit = 500): Promise<LogEntry[]> {
    try {
      const logs = await invoke<LogEntry[]>('plugin:algo|get_bot_logs', { botId, limit, offset: 0 })
      if (botId === null) recentLogs.value = logs
      return logs
    } catch (err) {
      console.error('[bots store] Failed to load logs:', err)
      return []
    }
  }

  function addLog(entry: LogEntry) {
    recentLogs.value.push(entry)
    if (recentLogs.value.length > MAX_LOG_ENTRIES) {
      recentLogs.value = recentLogs.value.slice(-MAX_LOG_ENTRIES)
    }
  }

  // Realized PnL across every bot, bucketed by exit time so a position held
  // overnight counts on the day it closed (see the journal heatmap).
  async function loadPnlSummary() {
    try {
      const window = (mode: TradingMode, days: number | null) =>
        invoke<TradeStats>('plugin:algo|get_trade_stats', {
          filters: {
            is_backtest: false,
            trading_mode: mode,
            ...(days === null ? {} : { exited_from: utcBound(startOfDaysAgo(days)) }),
          },
        })
      const summary = async (mode: TradingMode): Promise<PnlSummary> => {
        const [today, week, month, all] = await Promise.all([
          window(mode, 0),
          window(mode, 7),
          window(mode, 30),
          window(mode, null),
        ])
        return { today: today.total_pnl, week: week.total_pnl, month: month.total_pnl, all: all.total_pnl }
      }
      const [paper, live] = await Promise.all([summary('paper'), summary('live')])
      pnlPaper.value = paper
      pnlLive.value = live
      pnl.value = {
        today: paper.today + live.today,
        week: paper.week + live.week,
        month: paper.month + live.month,
        all: paper.all + live.all,
      }
    } catch (err) {
      console.error('[bots store] Failed to load PnL summary:', err)
    }
  }

  function msUntilNextMidnight(): number {
    const now = new Date()
    const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)
    return next.getTime() - now.getTime() + 1000
  }

  function scheduleDayRollover() {
    if (rolloverTimer) clearTimeout(rolloverTimer)
    rolloverTimer = setTimeout(() => {
      rolloverTimer = null
      loadPnlSummary().finally(() => {
        if (isInitialized.value) scheduleDayRollover()
      })
    }, msUntilNextMidnight())
  }

  function patch(botId: string, fields: Partial<BotSnapshot>) {
    const current = bots.value[botId]
    if (!current) return
    bots.value = { ...bots.value, [botId]: { ...current, ...fields } }
  }

  // ── Event subscriptions ──

  async function init() {
    if (isInitialized.value) return
    isInitialized.value = true
    const gen = ++initGen

    try {
      await load()
      await Promise.all([loadLogs(null, MAX_LOG_ENTRIES), loadPnlSummary(), loadPnlByBot()])
      if (gen !== initGen) return
      scheduleDayRollover()

      const unlistenStatus = await listen<BotStatusEvent>('bot:status', (event) => {
        const p = event.payload
        if (!p.bot_id) return
        if (!bots.value[p.bot_id]) {
          // A bot created elsewhere (the MCP tool) — pick up the row.
          void load()
          return
        }
        patch(p.bot_id, {
          status: p.status,
          started_at: p.started_at,
          last_error: p.last_error ?? null,
          ...(p.status !== 'running' ? { process_alive: false, open_positions: 0 } : {}),
        })
        if (p.status === 'running') lastError.value = null
      })

      const unlistenLog = await listen<BotLogEvent[]>('bot:log', (event) => {
        for (const { timestamp, level, message, bot_id } of event.payload) {
          addLog({ timestamp, level: level as LogEntry['level'], message, bot_id: bot_id ?? null })
        }
      })

      const unlistenTrade = await listen<BotTradeEvent>('bot:trade', (event) => {
        const trade = event.payload.trade
        if (trade.exit_price !== null && trade.pnl !== null) {
          const realised = trade.pnl
          const id = trade.bot_id ?? event.payload.bot_id
          const mode: TradingMode = trade.trading_mode ?? (id ? bots.value[id]?.trading_mode : undefined) ?? 'paper'
          for (const summary of [pnl.value, mode === 'live' ? pnlLive.value : pnlPaper.value]) {
            summary.today += realised
            summary.week += realised
            summary.month += realised
            summary.all += realised
          }
          scheduleBreakdownReload()
        }
      })

      const unlistenEquity = await listen<BotEquityEvent>('bot:equity', (event) => {
        const p = event.payload
        if (!p.bot_id) return
        patch(p.bot_id, {
          equity: p.equity,
          balance: p.balance ?? bots.value[p.bot_id]?.balance ?? 0,
          last_price: p.last_price ?? bots.value[p.bot_id]?.last_price ?? 0,
          open_positions: p.open_position_count ?? bots.value[p.bot_id]?.open_positions ?? 0,
          process_alive: true,
        })
      })

      const unlistenError = await listen<BotErrorEvent>('bot:error', (event) => {
        const p = event.payload
        lastError.value = { botId: p.bot_id ?? null, message: p.message }
        if (p.bot_id) patch(p.bot_id, { status: 'error', last_error: p.message })
        if (p.details) console.error('[bots store] Bot error details:', p.details)
      })

      const handlers = [unlistenStatus, unlistenLog, unlistenTrade, unlistenEquity, unlistenError]
      if (gen !== initGen) {
        for (const unlisten of handlers) unlisten()
        return
      }
      unlistenHandlers = handlers
    } catch (err) {
      console.error('[bots store] Failed to initialize:', err)
    }
  }

  function dispose() {
    initGen++
    for (const unlisten of unlistenHandlers) unlisten()
    unlistenHandlers = []
    if (rolloverTimer) {
      clearTimeout(rolloverTimer)
      rolloverTimer = null
    }
    if (breakdownTimer) {
      clearTimeout(breakdownTimer)
      breakdownTimer = null
    }
    isInitialized.value = false
  }

  return {
    // State
    bots,
    pnlByBot,
    statsByBot,
    breakdown,
    recentLogs,
    lastError,
    pnl,
    pnlPaper,
    pnlLive,
    isLoading,
    // Getters
    list,
    running,
    runningCount,
    liveRunningCount,
    totalEquity,
    totalOpenPositions,
    equityByMode,
    openByMode,
    hasLive,
    hasError,
    byId,
    name,
    // Actions
    load,
    loadPnlByBot,
    loadBreakdown: loadPnlByBot,
    create,
    start,
    stop,
    stopAll,
    remove,
    closePositions,
    preflight,
    loadLogs,
    loadPnlSummary,
    init,
    dispose,
  }
})
