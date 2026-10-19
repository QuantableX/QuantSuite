/**
 * The order book's numbers — pure, so scripts/test-terminal-orderbook.mjs
 * covers them: the price steps a pair offers, a new pair's default step, the
 * decimals a step needs, the grouped ladder with its USD values, and the
 * compact coin and dollar formats that keep four columns inside the 220px
 * panel.
 */
import { formatMoney } from '@quantsuite/core/money'

export interface RawLevel {
  price: number
  amount: number
}

export interface OrderLevel {
  price: number
  /** Coins at this level. */
  amount: number
  /** amount × price — USDT, read as USD. */
  value: number
  /** Cumulative USD from the spread outwards. */
  total: number
}

/** Binance prices have at most 8 decimals; buckets are compared on that grid. */
const GRID = 1e8
const onGrid = (x: number) => Math.round(x * GRID) / GRID

/** Decimals a step needs: 10 → 0, 0.05 → 2, 0.00000001 → 8. */
export function stepDecimals(step: number): number {
  if (!(step > 0) || step >= 1) return 0
  const text = step.toFixed(10).replace(/0+$/, '')
  return text.length - text.indexOf('.') - 1
}

/**
 * A pair's steps: its exchange step, then 5×, 10×, 50×, … up to 2 % of the
 * price — 0.01$ … 1000$ for BTC, 0.00000001$ and 0.00000005$ for PEPE, whose
 * own step is already 0.25 % of its price. Without a price yet only the
 * exchange step.
 */
export function tickOptions(minTick: number, price: number): number[] {
  if (!(minTick > 0)) return []
  const limit = price > 0 ? price * 0.02 : minTick
  const out: number[] = []
  for (let k = 0; k < 20; k++) {
    for (const m of [1, 5]) {
      const step = Number((minTick * m * 10 ** k).toPrecision(12))
      if (step > limit && out.length) return out
      out.push(step)
    }
  }
  return out
}

/** A new pair's step: the first one of at least 0.01 % of the price — 10$ for BTC. */
export function defaultTick(options: number[], price: number): number {
  return options.find((t) => t >= price * 1e-4) ?? options[options.length - 1] ?? 0
}

/**
 * One side of the book grouped into `tick` buckets: `rows` levels anchored at
 * the best ask/bid and extending outward, empty buckets included. Asks come
 * highest first (the spread sits below them), bids highest first.
 */
export function groupLevels(raw: RawLevel[], side: 'ask' | 'bid', anchor: number, tick: number, rows: number): OrderLevel[] {
  if (raw.length === 0 || !(anchor > 0) || !(tick > 0)) return []

  // The epsilon keeps a price that sits exactly on a bucket in it: 0.00000712
  // / 0.00000001 is 711.9999… in floating point.
  const bucketOf = (price: number) =>
    onGrid(side === 'ask' ? Math.ceil(price / tick - 1e-9) * tick : Math.floor(price / tick + 1e-9) * tick)

  const grouped = new Map<number, number>()
  for (const level of raw) {
    const key = bucketOf(level.price)
    grouped.set(key, (grouped.get(key) || 0) + level.amount)
  }

  const start = bucketOf(anchor)
  const levels: OrderLevel[] = []
  let total = 0
  for (let i = 0; i < rows; i++) {
    const price = onGrid(side === 'ask' ? start + i * tick : start - i * tick)
    if (price <= 0) break
    const amount = grouped.get(price) || 0
    const value = amount * price
    total += value
    levels.push({ price, amount, value, total })
  }
  return side === 'ask' ? levels.reverse() : levels
}

/** A ladder price with the step's decimals; no thousands separators, the column is narrow. */
export function formatBookPrice(price: number, decimals: number): string {
  return price.toFixed(decimals)
}

const COINS = new Intl.NumberFormat('en-US', { maximumSignificantDigits: 4, useGrouping: false })
const COINS_COMPACT = new Intl.NumberFormat('en-US', { notation: 'compact', maximumSignificantDigits: 3 })

/** Coins in a few characters: 1.204, 0.512, 512.3, 12.4K, 5.79B. */
export function formatCoins(amount: number): string {
  return amount >= 1000 ? COINS_COMPACT.format(amount) : COINS.format(amount)
}

/** Dollars, suffix form with three significant digits: 355$, 42.7K$, 1.25M$. */
export function formatUsd(value: number): string {
  return formatMoney(value, 'USD', { notation: 'compact', maximumSignificantDigits: 3 })
}
