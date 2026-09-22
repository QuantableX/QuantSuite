import { bandFor, DAY, formatValue, type Point } from '#terminal/utils/metrics'

export type CardVisual =
  'gauge' | 'bands' | 'balance' | 'flow' | 'split' | 'comparison' | 'activity' | 'trend' | 'share'
export interface CardReading {
  kind: CardVisual
  color: string
  label: string
  reference: string
  baseline: number | null
  share?: number
  splitUnit?: 'accounts' | 'positions' | 'volume'
}

const teal = '#35b6a0',
  amber = '#e7ad53',
  red = '#ed7884',
  blue = '#70b5e8',
  violet = '#b49bea'
const activity = new Set([
  'Active Addresses',
  'Transaction Count',
  'Total Fees',
  'Miner Revenue USD',
  'Exchange Inflow Total',
  'Exchange Outflow Total',
  'Transfer Volume',
  'Futures Volume',
  'Futures Trade Count',
  'Taker Buy Volume',
  'Taker Sell Volume',
  'DEX Volume',
  'Protocol Fees',
  'Protocol Revenue',
  'New Addresses',
  'Realized Profit',
  'Realized Loss',
  'Coin Days Destroyed',
])

/** Card references describe measured values; they are not trading signals. */
export function cardReading(title: string, points: Point[], spot: Point[] = []): CardReading {
  const kind: CardVisual =
    title === 'Fear & Greed Index'
      ? 'gauge'
      : /Supply in Profit|Supply in Loss/.test(title)
        ? 'share'
        : ['MVRV Ratio', 'Puell Multiple', 'Mayer Multiple'].includes(title) ||
            /SOPR|Holder MVRV/.test(title)
          ? 'bands'
          : title === 'Net Unrealized Profit/Loss' || /Holder NUPL/.test(title)
            ? 'balance'
            : [
                  'Exchange Netflow',
                  'Funding Rate',
                  'Annualized Funding',
                  'Net Realized Profit/Loss',
                  'Drawdown from ATH',
                ].includes(title)
              ? 'flow'
              : [
                    'Long/Short Ratio',
                    'Top Trader Account Ratio',
                    'Top Trader Position Ratio',
                    'Taker Buy/Sell Ratio',
                  ].includes(title)
                ? 'split'
                : title === 'Realized Price'
                  ? 'comparison'
                  : activity.has(title)
                    ? 'activity'
                    : 'trend'
  const last = points.at(-1)
  const base: CardReading = {
    kind,
    color: blue,
    label: 'Awaiting observations',
    reference: '',
    baseline: null,
  }
  if (!last || !Number.isFinite(last.value)) return base
  const value = last.value
  const band = bandFor(title, value)
  if (kind === 'share')
    return {
      ...base,
      color: /Loss/.test(title) ? red : teal,
      share: Math.min(1, Math.max(0, value / 100)),
      label: `${value.toFixed(1)}% of ${/Holder/.test(title) ? 'cohort' : 'supply'}`,
      reference: /Loss/.test(title)
        ? 'Coins below creation price'
        : 'Coins at or above creation price',
      baseline: null,
    }
  if (kind === 'gauge')
    return {
      ...base,
      color: band?.color ?? blue,
      label: band?.label ?? 'Sentiment',
      reference: '50 = neutral · 0–100',
      baseline: 50,
    }
  if (kind === 'bands')
    return {
      ...base,
      color: band?.color ?? blue,
      label: band?.label ?? 'Valuation',
      baseline: 1,
      reference: /SOPR/.test(title)
        ? '1 = spent at break-even'
        : /MVRV/.test(title)
          ? '1 = cost basis'
          : title === 'Mayer Multiple'
            ? '1 = 200D average price'
            : '1 = annual issuance mean',
    }
  if (kind === 'balance')
    return {
      ...base,
      color: band?.color ?? blue,
      label: band?.label ?? 'Unrealized profit / loss',
      reference: '0 = aggregate break-even',
      baseline: 0,
    }
  if (title === 'Exchange Netflow')
    return {
      ...base,
      color: value < 0 ? teal : value > 0 ? amber : blue,
      label: value < 0 ? 'Net outflow' : value > 0 ? 'Net inflow' : 'Balanced flows',
      reference: '0 = balanced flows',
      baseline: 0,
    }
  if (title === 'Net Realized Profit/Loss' || title === 'Drawdown from ATH')
    return {
      ...base,
      color: value < 0 ? red : teal,
      label:
        title === 'Drawdown from ATH'
          ? 'Distance below recorded peak'
          : value < 0
            ? 'Net realized loss'
            : 'Net realized profit',
      reference:
        title === 'Drawdown from ATH' ? '0% = all-time high' : '0 = profit and loss balanced',
      baseline: 0,
    }
  if (title === 'Funding Rate' || title === 'Annualized Funding')
    return {
      ...base,
      color: value < 0 ? violet : value > 0 ? amber : blue,
      label: value < 0 ? 'Shorts pay longs' : value > 0 ? 'Longs pay shorts' : 'No net funding',
      reference:
        title === 'Annualized Funding' ? '0% = no net payment · simple APR' : '0% = no net payment',
      baseline: 0,
    }
  if (kind === 'split' && value >= 0) {
    const splitUnit =
      title === 'Taker Buy/Sell Ratio'
        ? 'volume'
        : title === 'Top Trader Position Ratio'
          ? 'positions'
          : 'accounts'
    const label =
      splitUnit === 'volume'
        ? value > 1
          ? 'More taker buy volume'
          : value < 1
            ? 'More taker sell volume'
            : 'Balanced taker volume'
        : splitUnit === 'positions'
          ? value > 1
            ? 'Larger long positions'
            : value < 1
              ? 'Larger short positions'
              : 'Balanced positions'
          : value > 1
            ? 'More long accounts'
            : value < 1
              ? 'More short accounts'
              : 'Balanced accounts'
    return {
      ...base,
      color: value > 1 ? teal : value < 1 ? violet : blue,
      label,
      splitUnit,
      reference:
        splitUnit === 'volume'
          ? '1 = balanced taker flow'
          : splitUnit === 'positions'
            ? '1 = equal position sizes'
            : '1 = equal account counts',
      baseline: 1,
      share: value / (1 + value),
    }
  }
  if (kind === 'comparison') {
    // Compare BTC from the same provider and UTC date, independent of the asset in analysis.
    const price = spot.find((p) => p.time === last.time)?.value
    if (price == null || !Number.isFinite(price) || price <= 0 || value <= 0)
      return {
        ...base,
        label: 'Aggregate cost basis',
        reference: 'Same-day BTC price unavailable',
      }
    const gap = (price / value - 1) * 100
    return {
      ...base,
      color: gap >= 0 ? teal : red,
      label:
        gap === 0
          ? 'BTC at cost basis'
          : `BTC ${Math.abs(gap).toFixed(1)}% ${gap > 0 ? 'above' : 'below'} cost`,
      reference: `Spot ${formatValue(price, 'USD')} · same date`,
      baseline: price,
    }
  }
  const window = points.filter((p) => p.time >= last.time - 29 * DAY && p.time <= last.time)
  // A gap must not silently turn a shorter sample into a "30D average".
  const dates = new Set(window.map((p) => p.time))
  if (
    dates.size !== 30 ||
    !Array.from({ length: 30 }, (_, i) => last.time - i * DAY).every((time) => dates.has(time))
  )
    return {
      ...base,
      label: 'Recent activity',
      reference: '30D average unavailable',
    }
  const mean = window.reduce((sum, p) => sum + p.value, 0) / 30
  if (!Number.isFinite(mean) || mean <= 0)
    return {
      ...base,
      label: 'Recent activity',
      reference: '30D average unavailable',
    }
  const change = (value / mean - 1) * 100
  return {
    ...base,
    color: change >= 0 ? blue : violet,
    label:
      Math.abs(change) < 0.05
        ? 'At 30D average'
        : `${Math.abs(change).toFixed(1)}% ${change > 0 ? 'above' : 'below'} 30D avg`,
    reference: 'Dashed line = 30D average',
    baseline: mean,
  }
}
