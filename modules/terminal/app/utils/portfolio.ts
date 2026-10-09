import type { PortfolioDocument, PortfolioHolding } from '../types/portfolio'

export const emptyPortfolio = (): PortfolioDocument => ({ version: 1, revision: '', assets: [], trades: [], quotes: {}, snapshots: [] })
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 1e15
const date = (value: unknown): value is string => typeof value === 'string' && Number.isFinite(Date.parse(value))
const text = (value: unknown): value is string => typeof value === 'string'

/** Reject corrupt saves instead of replacing them with an empty portfolio. */
export function validatePortfolio(value: unknown): asserts value is PortfolioDocument {
  const doc = value as PortfolioDocument | null
  if (!doc || doc.version !== 1 || !text(doc.revision) || !Array.isArray(doc.assets) || !Array.isArray(doc.trades)
    || !Array.isArray(doc.snapshots) || !doc.quotes || typeof doc.quotes !== 'object' || Array.isArray(doc.quotes)) throw new Error('This portfolio could not be read. Its saved data has been preserved.')
  const ids = new Set<string>()
  for (const a of doc.assets) {
    if (!a || !text(a.id) || !a.id || ids.has(a.id) || !text(a.symbol) || !a.symbol.trim() || a.symbol.length > 30
      || !text(a.name) || !a.name.trim() || a.name.length > 150 || !['manual', 'market'].includes(a.source)
      || !text(a.marketName) || !Number.isInteger(a.marketPage) || a.marketPage < 1 || a.marketPage > 100
      || (a.manualPrice !== null && !finite(a.manualPrice)) || !date(a.updatedAt)) throw new Error('Check the asset name, symbol and price.')
    if (a.source === 'market' && !a.marketName) throw new Error('Choose a crypto asset from the market list.')
    ids.add(a.id)
  }
  const tradeIds = new Set<string>()
  for (const t of doc.trades) {
    if (!t || !text(t.id) || !t.id || tradeIds.has(t.id) || !ids.has(t.assetId) || !['opening', 'buy', 'sell'].includes(t.kind)
      || !date(t.date) || !finite(t.quantity) || t.quantity <= 0 || !finite(t.price) || !finite(t.fees)
      || !finite(t.quantity * t.price + t.fees) || !text(t.notes) || t.notes.length > 2000
      || !Number.isSafeInteger(t.order) || t.order < 0) throw new Error('Check the trade asset, date, quantity, price and fees.')
    if (Date.parse(t.date) > Date.now() + 60000) throw new Error('Trade dates cannot be in the future.')
    tradeIds.add(t.id)
  }
  for (const q of Object.values(doc.quotes)) {
    if (!q || !finite(q.price) || !date(q.checkedAt)) throw new Error('Invalid saved market quote. Saved data has been preserved.')
  }
  for (const s of doc.snapshots) {
    if (!s || !date(s.at) || !s.prices || typeof s.prices !== 'object' || Array.isArray(s.prices)
      || !Object.values(s.prices).every(finite)) throw new Error('Invalid saved valuation history. Saved data has been preserved.')
  }
  calculateHoldings(doc)
}

/** Moving average cost, including buy fees; a sell realizes its allocated cost. */
export function calculateHoldings(doc: PortfolioDocument, prices?: Record<string, number>, asOf = Infinity): PortfolioHolding[] {
  const rows = new Map(doc.assets.map((asset) => [asset.id, { asset, quantity: 0, cost: 0, realized: 0 }]))
  const trades = [...doc.trades].sort((a, b) => Date.parse(a.date) - Date.parse(b.date) || a.order - b.order || a.id.localeCompare(b.id))
  for (const t of trades) {
    if (Date.parse(t.date) > asOf) continue
    const row = rows.get(t.assetId)
    if (!row) throw new Error('A trade references an asset that no longer exists.')
    if (t.kind === 'sell') {
      const tolerance = Math.max(row.quantity, t.quantity) * 1e-10
      if (t.quantity - row.quantity > tolerance || row.quantity === 0) throw new Error(`${row.asset.symbol}: sale on ${t.date.slice(0, 10)} exceeds the holdings available then. Correct its quantity/date or the earlier entries.`)
      const soldCost = row.cost * Math.min(1, t.quantity / row.quantity)
      row.realized += t.quantity * t.price - t.fees - soldCost
      row.cost -= soldCost
      row.quantity -= t.quantity
      if (row.quantity <= tolerance) { row.quantity = 0; row.cost = 0 }
    } else {
      row.quantity += t.quantity
      row.cost += t.quantity * t.price + t.fees
    }
    if (!Number.isFinite(row.cost + row.realized + row.quantity) || Math.abs(row.cost) > 1e15 || Math.abs(row.realized) > 1e15) throw new Error('Portfolio amounts are too large.')
  }
  return [...rows.values()].map((row) => {
    const price = prices ? prices[row.asset.id] ?? null
      : row.asset.source === 'manual' ? row.asset.manualPrice : doc.quotes[row.asset.id]?.price ?? null
    const value = row.quantity === 0 ? 0 : price === null ? null : row.quantity * price
    return { ...row, averageCost: row.quantity ? row.cost / row.quantity : 0, price, value, unrealized: value === null ? null : value - row.cost }
  })
}

export function portfolioTotals(holdings: PortfolioHolding[]) {
  const current = holdings.filter((h) => h.quantity > 0)
  const missing = current.filter((h) => h.value === null).length
  const value = current.reduce((sum, h) => sum + (h.value ?? 0), 0)
  const cost = current.reduce((sum, h) => sum + h.cost, 0)
  const realized = holdings.reduce((sum, h) => sum + h.realized, 0)
  const unrealized = missing ? null : value - cost
  return { value, cost, realized, unrealized, totalGain: unrealized === null ? null : unrealized + realized, missing, count: current.length }
}

export function recordPortfolioSnapshot(doc: PortfolioDocument, at = new Date().toISOString()) {
  const prices: Record<string, number> = {}
  for (const row of calculateHoldings(doc)) if (row.price !== null) prices[row.asset.id] = row.price
  // The latest recorded valuation per UTC day. Historical points retain their own prices.
  const next = { at, prices }
  const index = doc.snapshots.findIndex((s) => s.at.slice(0, 10) === at.slice(0, 10))
  if (index < 0) doc.snapshots.push(next)
  else doc.snapshots[index] = next
}

export function portfolioHistory(doc: PortfolioDocument) {
  return [...doc.snapshots].sort((a, b) => Date.parse(a.at) - Date.parse(b.at)).map((s) => {
    const totals = portfolioTotals(calculateHoldings(doc, s.prices, Date.parse(s.at)))
    return { at: s.at, value: totals.missing ? null : totals.value, cost: totals.cost, gain: totals.totalGain }
  })
}

export const portfolioMoney = (value: number | null) => value === null ? '—' : new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 2 }).format(value)
export const portfolioNumber = (value: number) => new Intl.NumberFormat('en-US', { maximumFractionDigits: 10 }).format(value)
export const portfolioPrice = (value: number | null) => value === null ? '—' : new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 8 }).format(value)
export function assetColor(id: string) {
  let hash = 0
  for (const char of id) hash = (Math.imul(hash, 31) + char.charCodeAt(0)) >>> 0
  return `hsl(${hash % 360} 52% 57%)`
}
