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
      const saved = raw ? JSON.parse(raw) : []
      trades.value = Array.isArray(saved) ? saved as JournalTrade[] : []
    } catch {
      // Preserve unreadable saved data; never replace it with fabricated trades.
      trades.value = []
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
