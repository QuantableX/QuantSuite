import { defineStore } from 'pinia'

export interface JournalTrade {
  id: string
  date: string
  pair: string
  side: 'long' | 'short'
  entry: number
  exit: number | null
  size: number
  pnl: number | null
  pnlPercent: number | null
  status: 'open' | 'closed'
  notes: string
  tags: string[]
  strategy: string
  exchange: string
  fees: number
  screenshots: string[]
  emotion: 'confident' | 'fearful' | 'neutral' | 'greedy' | 'frustrated'
  rating: number
  createdAt: string
  updatedAt: string
}

export type SortField = 'date' | 'pnl' | 'pair'
export type SortDir = 'asc' | 'desc'

export interface JournalFilters {
  dateFrom: string
  dateTo: string
  pair: string
  side: 'all' | 'long' | 'short'
  status: 'all' | 'open' | 'closed'
  tags: string[]
  strategy: string
}

const STORAGE_KEY = 'quantview-journal'

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8)
}

function calcPnl(trade: JournalTrade): { pnl: number; pnlPercent: number } {
  if (trade.exit === null) return { pnl: 0, pnlPercent: 0 }
  const direction = trade.side === 'long' ? 1 : -1
  const pnl = direction * (trade.exit - trade.entry) * trade.size - trade.fees
  const pnlPercent = ((trade.exit - trade.entry) / trade.entry) * 100 * direction
  return { pnl: Math.round(pnl * 100) / 100, pnlPercent: Math.round(pnlPercent * 100) / 100 }
}

function buildSampleTrades(): JournalTrade[] {
  const now = new Date('2026-03-26')
  const d = (daysAgo: number) => {
    const dt = new Date(now)
    dt.setDate(dt.getDate() - daysAgo)
    return dt.toISOString().slice(0, 10)
  }
  const ts = (daysAgo: number) => {
    const dt = new Date(now)
    dt.setDate(dt.getDate() - daysAgo)
    return dt.toISOString()
  }

  const raw: Array<Omit<JournalTrade, 'id' | 'pnl' | 'pnlPercent' | 'createdAt' | 'updatedAt'>> = [
    { date: d(1), pair: 'BTC/USDT', side: 'long', entry: 69800, exit: 71250, size: 0.4, status: 'closed', notes: 'Entered on support bounce at 69.5k zone. Clean breakout above consolidation.', tags: ['setup-A', 'technical'], strategy: 'Breakout', exchange: 'Binance', fees: 5.64, screenshots: [], emotion: 'confident', rating: 5 },
    { date: d(2), pair: 'ETH/USDT', side: 'short', entry: 3950, exit: 3820, size: 3, status: 'closed', notes: 'Rejection at 4000 psychological resistance. Bearish divergence on RSI.', tags: ['technical', 'high-conviction'], strategy: 'Mean Reversion', exchange: 'Binance', fees: 3.12, screenshots: [], emotion: 'confident', rating: 4 },
    { date: d(3), pair: 'SOL/USDT', side: 'long', entry: 168, exit: 159, size: 15, status: 'closed', notes: 'Stopped out. Failed breakout turned into bull trap.', tags: ['setup-A'], strategy: 'Breakout', exchange: 'Bybit', fees: 2.50, screenshots: [], emotion: 'neutral', rating: 3 },
    { date: d(4), pair: 'BNB/USDT', side: 'long', entry: 580, exit: 602, size: 5, status: 'closed', notes: 'Mean reversion play from oversold RSI. Took profit at 20 EMA.', tags: ['technical'], strategy: 'Mean Reversion', exchange: 'Binance', fees: 2.96, screenshots: [], emotion: 'neutral', rating: 4 },
    { date: d(5), pair: 'BTC/USDT', side: 'long', entry: 68200, exit: 69500, size: 0.3, status: 'closed', notes: 'Trend continuation after 4H pullback to EMA. News catalyst from ETF flows.', tags: ['news-driven', 'technical'], strategy: 'Trend Following', exchange: 'Binance', fees: 4.14, screenshots: [], emotion: 'confident', rating: 4 },
    { date: d(7), pair: 'XRP/USDT', side: 'long', entry: 0.62, exit: 0.58, size: 5000, status: 'closed', notes: 'Bottom fishing attempt. Market had more downside than expected.', tags: ['high-conviction'], strategy: 'Mean Reversion', exchange: 'Bybit', fees: 1.50, screenshots: [], emotion: 'greedy', rating: 2 },
    { date: d(8), pair: 'ETH/USDT', side: 'long', entry: 3680, exit: 3760, size: 4, status: 'closed', notes: 'Quick scalp on 15m chart. Double bottom pattern.', tags: ['setup-A', 'technical'], strategy: 'Scalp', exchange: 'Binance', fees: 2.97, screenshots: [], emotion: 'neutral', rating: 3 },
    { date: d(10), pair: 'SOL/USDT', side: 'short', entry: 182, exit: 175, size: 12, status: 'closed', notes: 'Breakdown below ascending wedge. Clean setup.', tags: ['technical'], strategy: 'Breakout', exchange: 'Bybit', fees: 2.14, screenshots: [], emotion: 'confident', rating: 5 },
    { date: d(12), pair: 'BTC/USDT', side: 'short', entry: 71500, exit: 72800, size: 0.2, status: 'closed', notes: 'Tried to short the top. Squeezed out by strong buying.', tags: ['news-driven'], strategy: 'Mean Reversion', exchange: 'Binance', fees: 5.86, screenshots: [], emotion: 'fearful', rating: 1 },
    { date: d(14), pair: 'BNB/USDT', side: 'short', entry: 610, exit: 595, size: 8, status: 'closed', notes: 'Head and shoulders breakdown on 1H chart.', tags: ['technical', 'setup-A'], strategy: 'Breakout', exchange: 'Binance', fees: 3.84, screenshots: [], emotion: 'confident', rating: 4 },
    { date: d(18), pair: 'ETH/USDT', side: 'long', entry: 3520, exit: 3410, size: 5, status: 'closed', notes: 'Entered too early on a falling knife. Should have waited for confirmation.', tags: ['technical'], strategy: 'Trend Following', exchange: 'Binance', fees: 3.46, screenshots: [], emotion: 'frustrated', rating: 1 },
    { date: d(22), pair: 'SOL/USDT', side: 'long', entry: 155, exit: 172, size: 20, status: 'closed', notes: 'Swing trade. Entered on weekly support with bullish engulfing.', tags: ['high-conviction', 'technical'], strategy: 'Swing', exchange: 'Bybit', fees: 3.27, screenshots: [], emotion: 'confident', rating: 5 },
    { date: d(25), pair: 'BTC/USDT', side: 'long', entry: 64200, exit: 65800, size: 0.5, status: 'closed', notes: 'Breakout above descending trendline. Strong volume.', tags: ['setup-A', 'technical'], strategy: 'Breakout', exchange: 'Binance', fees: 6.50, screenshots: [], emotion: 'neutral', rating: 4 },
    { date: d(30), pair: 'XRP/USDT', side: 'short', entry: 0.65, exit: 0.61, size: 8000, status: 'closed', notes: 'Overbought on daily RSI. Clean rejection at resistance.', tags: ['technical'], strategy: 'Mean Reversion', exchange: 'Bybit', fees: 2.56, screenshots: [], emotion: 'neutral', rating: 3 },
    { date: d(35), pair: 'ETH/USDT', side: 'long', entry: 3280, exit: 3350, size: 6, status: 'closed', notes: 'Trend following after golden cross on 4H chart.', tags: ['technical', 'setup-A'], strategy: 'Trend Following', exchange: 'Binance', fees: 3.98, screenshots: [], emotion: 'confident', rating: 4 },
    { date: d(40), pair: 'BTC/USDT', side: 'short', entry: 62500, exit: 61800, size: 0.3, status: 'closed', notes: 'Scalp on 5m bearish engulfing at local high.', tags: ['technical'], strategy: 'Scalp', exchange: 'Binance', fees: 3.73, screenshots: [], emotion: 'neutral', rating: 3 },
    { date: d(45), pair: 'SOL/USDT', side: 'long', entry: 142, exit: 138, size: 25, status: 'closed', notes: 'False breakout. Got trapped above resistance that did not hold.', tags: ['setup-A'], strategy: 'Breakout', exchange: 'Bybit', fees: 3.55, screenshots: [], emotion: 'frustrated', rating: 2 },
    { date: d(50), pair: 'BNB/USDT', side: 'long', entry: 545, exit: 568, size: 6, status: 'closed', notes: 'Bounce off 200 EMA on daily. Took profit at prior resistance.', tags: ['technical', 'high-conviction'], strategy: 'Swing', exchange: 'Binance', fees: 3.34, screenshots: [], emotion: 'confident', rating: 4 },
    // Open trades
    { date: d(1), pair: 'BTC/USDT', side: 'long', entry: 70500, exit: null, size: 0.25, status: 'open', notes: 'Scaling in on pullback to 20 EMA. Targeting 73k.', tags: ['trend', 'technical'], strategy: 'Trend Following', exchange: 'Binance', fees: 1.76, screenshots: [], emotion: 'confident', rating: 4 },
    { date: d(2), pair: 'ETH/USDT', side: 'long', entry: 3880, exit: null, size: 3, status: 'open', notes: 'Accumulation zone. Building position for ETH/BTC ratio reversal.', tags: ['high-conviction'], strategy: 'Swing', exchange: 'Binance', fees: 1.16, screenshots: [], emotion: 'neutral', rating: 3 },
  ]

  return raw.map((t, i) => {
    const { pnl, pnlPercent } = t.exit !== null ? calcPnl(t as JournalTrade) : { pnl: null, pnlPercent: null }
    return {
      ...t,
      id: generateId() + i,
      pnl,
      pnlPercent,
      createdAt: ts(Number(t.date.split('-')[2]) || i),
      updatedAt: ts(Number(t.date.split('-')[2]) || i),
    } as JournalTrade
  })
}

export const useJournalStore = defineStore('terminal/journal', () => {
  const trades = ref<JournalTrade[]>([])
  const filters = ref<JournalFilters>({
    dateFrom: '',
    dateTo: '',
    pair: '',
    side: 'all',
    status: 'all',
    tags: [],
    strategy: '',
  })
  const sortField = ref<SortField>('date')
  const sortDir = ref<SortDir>('desc')

  // Persistence
  function loadFromStorage() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        trades.value = JSON.parse(raw) as JournalTrade[]
      } else {
        trades.value = buildSampleTrades()
        saveToStorage()
      }
    } catch {
      trades.value = buildSampleTrades()
      saveToStorage()
    }
  }

  function saveToStorage() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(trades.value))
  }

  // CRUD
  function addTrade(data: Omit<JournalTrade, 'id' | 'pnl' | 'pnlPercent' | 'createdAt' | 'updatedAt'>) {
    const now = new Date().toISOString()
    const { pnl, pnlPercent } = data.exit !== null ? calcPnl(data as JournalTrade) : { pnl: null, pnlPercent: null }
    const trade: JournalTrade = {
      ...data,
      id: generateId(),
      pnl,
      pnlPercent,
      createdAt: now,
      updatedAt: now,
    }
    trades.value.unshift(trade)
    saveToStorage()
    return trade
  }

  function updateTrade(id: string, data: Partial<JournalTrade>) {
    const idx = trades.value.findIndex(t => t.id === id)
    if (idx === -1) return
    const updated: JournalTrade = { ...trades.value[idx], ...data, updatedAt: new Date().toISOString() } as JournalTrade
    if (updated.exit !== null) {
      const { pnl, pnlPercent } = calcPnl(updated)
      updated.pnl = pnl
      updated.pnlPercent = pnlPercent
    } else {
      // Exit was cleared — the trade is open again, so the old realized PnL goes
      updated.pnl = null
      updated.pnlPercent = null
    }
    trades.value[idx] = updated
    saveToStorage()
  }

  function deleteTrade(id: string) {
    trades.value = trades.value.filter(t => t.id !== id)
    saveToStorage()
  }

  function closeTrade(id: string, exitPrice: number, fees?: number) {
    const idx = trades.value.findIndex(t => t.id === id)
    if (idx === -1) return
    const trade = { ...trades.value[idx] } as JournalTrade
    trade.exit = exitPrice
    trade.status = 'closed'
    if (fees !== undefined) trade.fees = trade.fees + fees
    trade.updatedAt = new Date().toISOString()
    const { pnl, pnlPercent } = calcPnl(trade)
    trade.pnl = pnl
    trade.pnlPercent = pnlPercent
    trades.value[idx] = trade
    saveToStorage()
  }

  // Filtered & sorted
  const filteredTrades = computed(() => {
    let result = trades.value.filter(t => {
      if (filters.value.side !== 'all' && t.side !== filters.value.side) return false
      if (filters.value.status !== 'all' && t.status !== filters.value.status) return false
      if (filters.value.pair && !t.pair.toLowerCase().includes(filters.value.pair.toLowerCase())) return false
      if (filters.value.strategy && t.strategy !== filters.value.strategy) return false
      if (filters.value.dateFrom && t.date < filters.value.dateFrom) return false
      if (filters.value.dateTo && t.date > filters.value.dateTo) return false
      if (filters.value.tags.length > 0 && !filters.value.tags.some(tag => t.tags.includes(tag))) return false
      return true
    })

    result.sort((a, b) => {
      let cmp = 0
      switch (sortField.value) {
        case 'date':
          cmp = a.date.localeCompare(b.date)
          break
        case 'pnl':
          cmp = (a.pnl ?? 0) - (b.pnl ?? 0)
          break
        case 'pair':
          cmp = a.pair.localeCompare(b.pair)
          break
      }
      return sortDir.value === 'desc' ? -cmp : cmp
    })

    return result
  })

  // Statistics
  const closedTrades = computed(() => trades.value.filter(t => t.status === 'closed'))
  const openTrades = computed(() => trades.value.filter(t => t.status === 'open'))

  const totalPnl = computed(() => closedTrades.value.reduce((s, t) => s + (t.pnl ?? 0), 0))
  const winningTrades = computed(() => closedTrades.value.filter(t => (t.pnl ?? 0) > 0))
  const losingTrades = computed(() => closedTrades.value.filter(t => (t.pnl ?? 0) <= 0))
  const winRate = computed(() => closedTrades.value.length > 0 ? (winningTrades.value.length / closedTrades.value.length * 100) : 0)
  const avgWin = computed(() => {
    const w = winningTrades.value
    return w.length > 0 ? w.reduce((s, t) => s + (t.pnl ?? 0), 0) / w.length : 0
  })
  const avgLoss = computed(() => {
    const l = losingTrades.value
    return l.length > 0 ? l.reduce((s, t) => s + (t.pnl ?? 0), 0) / l.length : 0
  })
  const profitFactor = computed(() => {
    const grossWin = winningTrades.value.reduce((s, t) => s + (t.pnl ?? 0), 0)
    const grossLoss = Math.abs(losingTrades.value.reduce((s, t) => s + (t.pnl ?? 0), 0))
    return grossLoss > 0 ? Math.round((grossWin / grossLoss) * 100) / 100 : grossWin > 0 ? Infinity : 0
  })
  const bestTrade = computed(() => {
    if (closedTrades.value.length === 0) return null
    return closedTrades.value.reduce((best, t) => (t.pnl ?? 0) > (best.pnl ?? 0) ? t : best)
  })
  const worstTrade = computed(() => {
    if (closedTrades.value.length === 0) return null
    return closedTrades.value.reduce((worst, t) => (t.pnl ?? 0) < (worst.pnl ?? 0) ? t : worst)
  })
  const maxDrawdown = computed(() => {
    const sorted = [...closedTrades.value].sort((a, b) => a.date.localeCompare(b.date))
    let peak = 0
    let cumPnl = 0
    let dd = 0
    for (const t of sorted) {
      cumPnl += t.pnl ?? 0
      if (cumPnl > peak) peak = cumPnl
      const currentDd = peak - cumPnl
      if (currentDd > dd) dd = currentDd
    }
    return Math.round(dd * 100) / 100
  })

  const streaks = computed(() => {
    const sorted = [...closedTrades.value].sort((a, b) => a.date.localeCompare(b.date))
    let currentStreak = 0
    let currentType: 'win' | 'loss' | null = null
    let longestWin = 0
    let longestLoss = 0
    let streak = 0
    let streakType: 'win' | 'loss' | null = null

    for (const t of sorted) {
      const isWin = (t.pnl ?? 0) > 0
      if (isWin) {
        if (streakType === 'win') { streak++ } else { streak = 1; streakType = 'win' }
        if (streak > longestWin) longestWin = streak
      } else {
        if (streakType === 'loss') { streak++ } else { streak = 1; streakType = 'loss' }
        if (streak > longestLoss) longestLoss = streak
      }
    }
    currentStreak = streak
    currentType = streakType
    return { current: currentStreak, currentType, longestWin, longestLoss }
  })

  // All unique values for filter dropdowns
  const allPairs = computed(() => [...new Set(trades.value.map(t => t.pair))].sort())
  const allStrategies = computed(() => [...new Set(trades.value.map(t => t.strategy).filter(Boolean))].sort())
  const allTags = computed(() => [...new Set(trades.value.flatMap(t => t.tags))].sort())

  // Import / Export
  function exportJSON(): string {
    return JSON.stringify(trades.value, null, 2)
  }

  function exportCSV(): string {
    const headers = ['id', 'date', 'pair', 'side', 'entry', 'exit', 'size', 'pnl', 'pnlPercent', 'status', 'notes', 'tags', 'strategy', 'exchange', 'fees', 'emotion', 'rating']
    // RFC 4180: quote anything with a comma, quote or line break, escape " as ""
    const escapeField = (val: string) =>
      /[",\r\n]/.test(val) ? `"${val.replace(/"/g, '""')}"` : val
    const rows = trades.value.map(t =>
      headers.map(h => {
        const val = t[h as keyof JournalTrade]
        if (Array.isArray(val)) return escapeField(val.join(';'))
        if (typeof val === 'string') return escapeField(val)
        return val ?? ''
      }).join(',')
    )
    return [headers.join(','), ...rows].join('\n')
  }

  function importJSON(data: JournalTrade[], merge: boolean) {
    if (merge) {
      const existingIds = new Set(trades.value.map(t => t.id))
      const newTrades = data.filter(t => !existingIds.has(t.id))
      trades.value = [...trades.value, ...newTrades]
    } else {
      trades.value = data
    }
    saveToStorage()
  }

  function setSort(field: SortField) {
    if (sortField.value === field) {
      sortDir.value = sortDir.value === 'desc' ? 'asc' : 'desc'
    } else {
      sortField.value = field
      sortDir.value = 'desc'
    }
  }

  return {
    trades,
    filters,
    sortField,
    sortDir,
    filteredTrades,
    closedTrades,
    openTrades,
    totalPnl,
    winRate,
    winningTrades,
    losingTrades,
    avgWin,
    avgLoss,
    profitFactor,
    bestTrade,
    worstTrade,
    maxDrawdown,
    streaks,
    allPairs,
    allStrategies,
    allTags,
    loadFromStorage,
    saveToStorage,
    addTrade,
    updateTrade,
    deleteTrade,
    closeTrade,
    setSort,
    exportJSON,
    exportCSV,
    importJSON,
  }
})
