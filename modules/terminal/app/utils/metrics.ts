import { historicalMetrics, calculatedMetrics } from '../data/historical-metrics.ts'

export type FeedSource =
  | 'positioning'
  | 'utxo-profit'
  | 'utxo-supply'
  | 'utxo-value'
  | 'utxo-activity'
  | 'network'
  | 'sentiment'
  | 'funding'
  | 'open-interest'
  | 'long-short'
  | 'price'
  | 'futures'
  | 'top-accounts'
  | 'top-positions'
  | 'chain-volume'
  | 'chain-difficulty'
  | 'chain-transactions'
  | 'defi-tvl'
  | 'defi-dex'
  | 'defi-fees'
  | 'defi-revenue'
export interface HistoryRow {
  time: number
  values: Record<string, number>
  provisional: string[]
}
export interface History {
  rows: HistoryRow[]
  fetchedAt: number
}
export interface Point {
  time: number
  value: number
  provisional?: boolean
}
export interface Band {
  label: string
  min: number
  max: number
  color: string
  explanation: string
}
export interface MetricBinding {
  source: FeedSource
  fields: string[]
  unit: string
  formula: string
  provider: string
  url: string
  note: string
  bands?: Band[]
  calculate?: (values: Record<string, number>) => number
  joins?: { source: FeedSource; fields: string[]; dayOffset?: number }[]
  analysisStart?: number
  derived?: boolean
}
const teal = '#35b6a0',
  amber = '#e7ad53',
  red = '#ed7884',
  gray = '#a8acb8'
const cm = {
  source: 'network' as const,
  provider: 'Coin Metrics Community',
  url: 'https://coinmetrics.io/community-network-data/',
  note: 'Daily UTC observations; provider history may be revised. Exchange flows cover provider-labeled wallets.',
}
const network = (
  fields: string[],
  unit: string,
  formula: string,
  calculate?: MetricBinding['calculate'],
): MetricBinding => ({ ...cm, fields, unit, formula, calculate })
const exchange = {
  provider: 'Binance USDⓈ-M · BTCUSDT',
  url: 'https://www.binance.com/en/futures/BTCUSDT',
}
const futures = (
  fields: string[],
  unit: string,
  formula: string,
  calculate?: MetricBinding['calculate'],
): MetricBinding => ({
  ...exchange,
  source: 'futures',
  fields,
  unit,
  formula,
  calculate,
  note: 'Binance BTCUSDT perpetuals only. Completed UTC daily candles; quote volumes are in USDT, not a market-wide aggregate.',
})
const chain = (
  source: FeedSource,
  fields: string[],
  unit: string,
  formula: string,
  calculate?: MetricBinding['calculate'],
): MetricBinding => ({
  source,
  fields,
  unit,
  formula,
  calculate,
  provider: 'Blockchain.com',
  url: 'https://www.blockchain.com/explorer/charts',
  note: 'Unsampled daily Bitcoin observations. Transfer volume is the provider’s estimate excluding suspected change outputs; historical estimates can be revised.',
})
const defi = (source: FeedSource, formula: string): MetricBinding => ({
  source,
  fields: ['value'],
  unit: 'USD',
  formula,
  provider: 'DefiLlama',
  url: 'https://defillama.com/',
  note: 'Provider aggregate across covered chains/protocols, using completed UTC dates. Coverage evolves and history can be revised.',
})

export const bindings: Record<string, MetricBinding> = {
  'Fear & Greed Index': {
    source: 'sentiment',
    fields: ['value'],
    unit: 'Score',
    provider: 'Alternative.me',
    url: 'https://alternative.me/crypto/fear-and-greed-index/',
    formula: 'Provider sentiment index, 0–100',
    note: 'Bitcoin sentiment, published daily. Greed describes sentiment, not an automatic buy signal.',
    bands: [
      {
        label: 'Extreme fear',
        min: 0,
        max: 25,
        color: red,
        explanation:
          'Very fearful sentiment. It can persist during a drawdown; compare with price structure and past recoveries.',
      },
      {
        label: 'Fear',
        min: 25,
        max: 45,
        color: amber,
        explanation:
          'Cautious sentiment. Look for whether price is stabilizing or still making lower lows.',
      },
      {
        label: 'Neutral',
        min: 45,
        max: 56,
        color: gray,
        explanation: 'Sentiment is balanced. The score alone gives little directional information.',
      },
      {
        label: 'Greed',
        min: 56,
        max: 75,
        color: teal,
        explanation:
          'Optimistic sentiment. Compare continued momentum with earlier periods of similar enthusiasm.',
      },
      {
        label: 'Extreme greed',
        min: 75,
        max: 101,
        color: amber,
        explanation:
          'Very optimistic sentiment. Crowding can increase correction risk, but strong trends can remain here.',
      },
    ],
  },
  'MVRV Ratio': {
    ...network(['CapMVRVCur'], 'Ratio', 'Market capitalization / Realized capitalization'),
    bands: [
      {
        label: 'Below cost basis',
        min: -Infinity,
        max: 1,
        color: teal,
        explanation:
          'Market value is below aggregate on-chain cost basis. This is a valuation observation; losses can still deepen.',
      },
      {
        label: 'Moderate valuation',
        min: 1,
        max: 2,
        color: gray,
        explanation:
          'Market value is one to two times realized value. Compare the trend with similar phases of previous cycles.',
      },
      {
        label: 'Elevated valuation',
        min: 2,
        max: 3.5,
        color: amber,
        explanation:
          'Unrealized profits are elevated. Check whether demand and price momentum are keeping pace.',
      },
      {
        label: 'Stretched valuation',
        min: 3.5,
        max: Infinity,
        color: red,
        explanation:
          'Market value is high relative to aggregate cost basis. This historical heuristic is not a timing rule.',
      },
    ],
  },
  'Net Unrealized Profit/Loss': {
    ...network(['CapMVRVCur'], 'NUPL', '1 − 1 / MVRV', (v) => 1 - 1 / v.CapMVRVCur!),
    bands: [
      {
        label: 'Aggregate loss',
        min: -Infinity,
        max: 0,
        color: teal,
        explanation:
          'The supply is in aggregate unrealized loss. A negative value is not a guarantee of a market bottom.',
      },
      {
        label: 'Low unrealized profit',
        min: 0,
        max: 0.25,
        color: gray,
        explanation:
          'The supply holds relatively little unrealized profit. Compare recovery attempts with the price chart.',
      },
      {
        label: 'Moderate profit',
        min: 0.25,
        max: 0.5,
        color: teal,
        explanation:
          'A moderate share of market value is unrealized profit. Follow whether profits expand with price.',
      },
      {
        label: 'Elevated profit',
        min: 0.5,
        max: 0.75,
        color: amber,
        explanation:
          'Unrealized profits are elevated. Historical profit-taking behavior is useful context.',
      },
      {
        label: 'Very high profit',
        min: 0.75,
        max: Infinity,
        color: red,
        explanation:
          'A large share of market value is unrealized profit. Compare previous stretched periods and their drawdowns.',
      },
    ],
  },
  'Realized Price': network(
    ['PriceUSD', 'CapMVRVCur'],
    'USD',
    'Coin Metrics PriceUSD / MVRV',
    (v) => v.PriceUSD! / v.CapMVRVCur!,
  ),
  'Realized Capitalization': network(
    ['CapMrktCurUSD', 'CapMVRVCur'],
    'USD',
    'Market capitalization / MVRV',
    (v) => v.CapMrktCurUSD! / v.CapMVRVCur!,
  ),
  'Exchange Netflow': network(
    ['FlowInExNtv', 'FlowOutExNtv'],
    'BTC',
    'Exchange inflow − Exchange outflow',
    (v) => v.FlowInExNtv! - v.FlowOutExNtv!,
  ),
  'Exchange Reserve': network(
    ['SplyExNtv'],
    'BTC',
    'BTC held by provider-labeled exchange addresses',
  ),
  'Exchange Inflow Total': network(
    ['FlowInExNtv'],
    'BTC',
    'BTC transferred into provider-labeled exchange addresses per day',
  ),
  'Exchange Outflow Total': network(
    ['FlowOutExNtv'],
    'BTC',
    'BTC transferred out of provider-labeled exchange addresses per day',
  ),
  'Exchange Supply Ratio': network(
    ['SplyExNtv', 'SplyCur'],
    '%',
    'Exchange supply / Current supply × 100',
    (v) => (v.SplyExNtv! / v.SplyCur!) * 100,
  ),
  'Active Addresses': network(
    ['AdrActCnt'],
    'Addresses',
    'Distinct addresses participating in a ledger change per day',
  ),
  'Transaction Count': network(['TxCnt'], 'Transactions', 'On-chain transactions per day'),
  Hashrate: network(
    ['HashRate'],
    'EH/s',
    'Estimated network hashrate (TH/s) / 1,000,000',
    (v) => v.HashRate! / 1e6,
  ),
  'Total Fees': network(
    ['FeeTotNtv', 'PriceUSD'],
    'USD',
    'Daily native fees × Coin Metrics PriceUSD',
    (v) => v.FeeTotNtv! * v.PriceUSD!,
  ),
  'Miner Revenue USD': network(
    ['IssTotUSD', 'FeeTotNtv', 'PriceUSD'],
    'USD',
    'Daily issuance USD + Daily native fees × PriceUSD',
    (v) => v.IssTotUSD! + v.FeeTotNtv! * v.PriceUSD!,
  ),
  'Puell Multiple': {
    ...network(['IssTotUSD'], 'Ratio', 'Daily issuance USD / Trailing 365-day mean issuance USD'),
    bands: [
      {
        label: 'Low issuance value',
        min: 0,
        max: 0.5,
        color: teal,
        explanation:
          'Daily issuance value is below half of its trailing annual mean. Halvings also affect this comparison.',
      },
      {
        label: 'Typical issuance value',
        min: 0.5,
        max: 4,
        color: gray,
        explanation:
          'Daily issuance value is within the broad historical middle range relative to the annual mean.',
      },
      {
        label: 'High issuance value',
        min: 4,
        max: Infinity,
        color: amber,
        explanation:
          'Daily issuance value is several times the annual mean. Compare with previous periods of elevated miner incentives.',
      },
    ],
  },
  'Funding Rate': {
    ...exchange,
    source: 'funding',
    fields: ['fundingRate'],
    unit: '%',
    formula: 'Settled funding rate × 100; daily sum for comparison',
    note: 'Binance BTCUSDT only. Sum of settled payments per complete UTC day, paginated from the first available settlements. Not OI-weighted.',
    calculate: (v) => v.fundingRate! * 100,
  },
  'Open Interest': {
    ...exchange,
    source: 'open-interest',
    fields: ['sumOpenInterestValue'],
    unit: 'USDT',
    formula: 'Notional open interest at the daily snapshot',
    note: 'Binance BTCUSDT notional open interest in USDT.',
  },
  'Long/Short Ratio': {
    ...exchange,
    source: 'long-short',
    fields: ['longShortRatio'],
    unit: 'Ratio',
    formula: 'Accounts net long / Accounts net short',
    note: 'Binance BTCUSDT account ratio, not position size.',
  },
  'NVT Ratio': {
    ...chain(
      'chain-volume',
      ['transferUSD', 'CapMrktCurUSD'],
      'Ratio',
      'Coin Metrics market cap / Blockchain.com estimated daily transfer volume',
      (v) => (v.transferUSD! > 0 ? v.CapMrktCurUSD! / v.transferUSD! : NaN),
    ),
    provider: 'Blockchain.com + Coin Metrics',
    joins: [{ source: 'network', fields: ['CapMrktCurUSD'] }],
    note: 'Daily NVT using estimated transfer volume and same-date market capitalization. This methodology can differ from other providers’ adjusted NVT or smoothed NVT Signal.',
  },
  'Transfer Volume': chain(
    'chain-volume',
    ['transferUSD'],
    'USD',
    'Estimated on-chain transfer volume, excluding suspected change',
  ),
  'Mean Transaction Size': {
    ...chain(
      'chain-volume',
      ['transferUSD', 'transactions'],
      'USD',
      'Estimated transfer volume / Confirmed transaction count',
      (v) => (v.transactions! > 0 ? v.transferUSD! / v.transactions! : NaN),
    ),
    joins: [{ source: 'chain-transactions', fields: ['transactions'] }],
  },
  Difficulty: chain(
    'chain-difficulty',
    ['difficulty'],
    'Difficulty',
    'Daily Bitcoin mining difficulty',
  ),
  'Futures Volume': futures(
    ['quoteVolume'],
    'USDT',
    'Binance BTCUSDT perpetual daily quote volume',
  ),
  'Futures Trade Count': futures(
    ['trades'],
    'Trades',
    'Binance BTCUSDT perpetual trades per UTC day',
  ),
  'Taker Buy Volume': futures(
    ['takerBuyQuote'],
    'USDT',
    'Daily buyer-initiated perpetual quote volume',
  ),
  'Taker Sell Volume': futures(
    ['takerSellQuote'],
    'USDT',
    'Daily quote volume − Taker buy quote volume',
  ),
  'Taker Buy/Sell Ratio': futures(
    ['takerBuyQuote', 'takerSellQuote'],
    'Ratio',
    'Taker buy quote volume / Taker sell quote volume',
    (v) => (v.takerSellQuote! > 0 ? v.takerBuyQuote! / v.takerSellQuote! : NaN),
  ),
  'Annualized Funding': {
    ...exchange,
    source: 'funding',
    fields: ['fundingRate'],
    unit: '%',
    formula: 'Complete UTC day’s summed funding × 100 × 365',
    calculate: (v) => v.fundingRate! * 100 * 365,
    note: 'Simple APR implied by one completed day’s net funding, without compounding. It is not a forecast or guaranteed annual yield. Binance BTCUSDT only.',
  },
  'Top Trader Account Ratio': {
    ...exchange,
    source: 'top-accounts',
    fields: ['longShortRatio'],
    unit: 'Ratio',
    formula: 'Top traders’ net-long accounts / Net-short accounts',
    note: 'Binance BTCUSDT only: top 20% of users by margin balance. Account counts, not position sizes. The public archive omits much of 2022 for this field.',
  },
  'Top Trader Position Ratio': {
    ...exchange,
    source: 'top-positions',
    fields: ['longShortRatio'],
    unit: 'Ratio',
    formula: 'Top traders’ long position size / Short position size',
    note: 'Binance BTCUSDT only: top 20% of users by margin balance. Position sizes, not account counts. The public archive omits much of 2022 for this field.',
  },
  'OI / Market Cap': {
    ...exchange,
    source: 'open-interest',
    fields: ['sumOpenInterestValue', 'CapMrktCurUSD'],
    unit: '%',
    joins: [{ source: 'network', fields: ['CapMrktCurUSD'], dayOffset: -1 }],
    formula: 'Daily OI snapshot / Previous UTC day’s market cap × 100',
    calculate: (v) =>
      v.CapMrktCurUSD! > 0 ? (v.sumOpenInterestValue! / v.CapMrktCurUSD!) * 100 : NaN,
    note: 'Binance BTCUSDT notional OI (USDT) relative to the previous day’s Coin Metrics BTC capitalization (USD). Assumes USDT≈USD; no intraday or missing-date substitution.',
  },
  'OI / 24H Volume': {
    ...exchange,
    source: 'open-interest',
    fields: ['sumOpenInterestValue', 'quoteVolume'],
    unit: 'Ratio',
    joins: [{ source: 'futures', fields: ['quoteVolume'], dayOffset: -1 }],
    formula: 'Daily OI snapshot / Previous complete UTC day’s futures quote volume',
    calculate: (v) => (v.quoteVolume! > 0 ? v.sumOpenInterestValue! / v.quoteVolume! : NaN),
    note: 'Binance BTCUSDT only. Compares a daily OI snapshot with the preceding UTC day’s traded volume, avoiding unfinished or future volume.',
  },
  'DeFi TVL': {
    ...defi('defi-tvl', 'DefiLlama aggregate DeFi TVL, USD'),
    note: 'Provider aggregate across covered chains, excluding liquid staking and double-counted TVL. Completed UTC dates; historical coverage can be revised.',
  },
  'DEX Volume': defi('defi-dex', 'Aggregate decentralized exchange daily volume, USD'),
  'Protocol Fees': defi('defi-fees', 'Aggregate daily user fees across covered protocols, USD'),
  'Protocol Revenue': defi('defi-revenue', 'Aggregate daily protocol revenue, USD'),
}

// Keep the raw provider data accessible, but omit the initial price-discovery
// period from the default research view. This is a visible view preference.
const researchStart = Date.UTC(2011, 0, 1) / 1000
for (const title of [
  'MVRV Ratio',
  'Net Unrealized Profit/Loss',
  'Realized Price',
  'Realized Capitalization',
]) {
  bindings[title]!.analysisStart = researchStart
}
for (const title of [
  'Open Interest',
  'Long/Short Ratio',
  'Top Trader Account Ratio',
  'Top Trader Position Ratio',
  'OI / Market Cap',
  'OI / 24H Volume',
]) {
  const binding = bindings[title]!
  binding.source = 'positioning'
  binding.note +=
    ' Public Binance archive since September 2020 plus recent REST observations, sampled at exact UTC midnight. Missing archive dates remain gaps.'
}
bindings['Top Trader Account Ratio']!.fields = ['topAccountRatio']
bindings['Top Trader Position Ratio']!.fields = ['topPositionRatio']

const ratioBands: Band[] = [
  {
    min: -Infinity,
    max: 1,
    label: 'Below break-even',
    color: teal,
    explanation: 'The ratio is below one. Compare prior losses and recoveries alongside price.',
  },
  {
    min: 1,
    max: Infinity,
    label: 'Above break-even',
    color: amber,
    explanation:
      'The ratio is above one. Compare the size and persistence of profits across previous cycles.',
  },
]
for (const [title, source, field, unit, , description] of historicalMetrics) {
  bindings[title] = {
    source,
    fields: [field],
    unit,
    formula: description,
    provider: 'Bitcoin Research Kit · Bitview',
    url: `https://bitview.space/api/series/${field}`,
    note:
      'BRK daily samples at each UTC period’s final block. Flow measures are trailing 24 hours at that block, not exact calendar-day sums. UTXO cohorts use 150 days; no entity or exchange labels. Historical price before block 340,000 uses baked exchange data; later valuation uses BRK’s on-chain USD price estimate. Provider conventions include SOPR=1 when creation value is zero. ' +
      description,
    analysisStart: researchStart,
    ...(/SOPR|Holder MVRV/.test(title) ? { bands: ratioBands } : {}),
    ...(/Holder NUPL/.test(title) ? { bands: bindings['Net Unrealized Profit/Loss']!.bands } : {}),
  }
}
bindings['MVRV Z-Score'] = {
  ...network(
    ['CapMrktCurUSD', 'CapMVRVCur'],
    'Ratio',
    '(Market cap − realized cap) / expanding population standard deviation of market cap',
  ),
  note: 'Calculated only from observations available through each date. Expanding population standard deviation begins at the first joint market cap/MVRV observation; 365 consecutive daily observations required for the first value. This convention can differ from another provider’s Z-score.',
}
bindings['Mayer Multiple'] = network(
  ['PriceUSD'],
  'Ratio',
  'BTC USD reference / complete trailing 200-day mean',
)
bindings['Mayer Multiple']!.bands = [
  {
    min: 0,
    max: 1,
    label: 'Below 200D average',
    color: teal,
    explanation:
      'Price is below its trailing 200-day average. Compare the duration and depth with earlier cycles.',
  },
  {
    min: 1,
    max: Infinity,
    label: 'Above 200D average',
    color: amber,
    explanation:
      'Price is above its trailing 200-day average. This relative level does not set a universal sell threshold.',
  },
]
bindings['Realized Volatility 30D'] = network(
  ['PriceUSD'],
  '%',
  '30 daily log returns: population standard deviation × √365 × 100',
)
bindings['Drawdown from ATH'] = network(
  ['PriceUSD'],
  '%',
  '(BTC USD reference / highest reference price to date − 1) × 100',
)
bindings['NVT Signal 90D'] = {
  ...bindings['NVT Ratio']!,
  formula: 'Daily market cap / complete trailing 90-day mean estimated transfer volume',
  calculate: (v) => (v.transferUSD! >= 0 && v.CapMrktCurUSD! > 0 ? v.transferUSD! : NaN),
}
bindings['Fee Share of Miner Revenue'] = network(
  ['FeeTotNtv', 'PriceUSD', 'IssTotUSD'],
  '%',
  'Transaction fees / (issuance + fees) × 100',
  (v) =>
    v.IssTotUSD! + v.FeeTotNtv! * v.PriceUSD! > 0
      ? ((v.FeeTotNtv! * v.PriceUSD!) / (v.IssTotUSD! + v.FeeTotNtv! * v.PriceUSD!)) * 100
      : NaN,
)
bindings['Hashprice'] = {
  ...network(
    ['FeeTotNtv', 'PriceUSD', 'IssTotUSD', 'HashRate'],
    'USD',
    '(USD issuance + USD fees) / daily hashrate in PH/s',
    (v) =>
      v.HashRate! > 0 ? (v.IssTotUSD! + v.FeeTotNtv! * v.PriceUSD!) / (v.HashRate! / 1000) : NaN,
  ),
  note: 'Daily estimated USD miner revenue per PH/s. Coin Metrics HashRate is TH/s; divided by 1,000 for PH/s. Daily estimates are noisy and do not model mining costs.',
}

export const DAY = 86400
for (const [title] of calculatedMetrics) bindings[title]!.derived = true
export const utcDay = (time: number) => Math.floor(time / DAY) * DAY

/** Join only the specified UTC date; missing fields remain absent, never carried forward. */
export function joinedMetricRows(
  binding: MetricBinding,
  getRows: (source: FeedSource) => HistoryRow[],
): HistoryRow[] {
  const rows = getRows(binding.source)
  if (!binding.joins?.length) return rows
  const lookups = binding.joins.map((join) => ({
    ...join,
    days: new Map(getRows(join.source).map((row) => [utcDay(row.time), row])),
  }))
  return rows.map((row) => {
    const values = { ...row.values },
      provisional = [...row.provisional]
    for (const join of lookups) {
      const match = join.days.get(utcDay(row.time) + (join.dayOffset ?? 0) * DAY)
      for (const field of join.fields) {
        delete values[field]
        if (match && Number.isFinite(match.values[field])) values[field] = match.values[field]!
        if (match?.provisional.includes(field)) provisional.push(field)
      }
    }
    return { ...row, values, provisional }
  })
}

/** Keep unknowns missing. Never forward-fill financial observations. */
export function metricPoints(title: string, rows: HistoryRow[], now = Date.now() / 1000): Point[] {
  const binding = bindings[title]
  if (!binding) return []
  const sorted = [...rows].sort((a, b) => a.time - b.time)
  const days = new Map<number, Point>()
  for (const row of sorted) {
    if (!Number.isFinite(row.time) || row.time <= 0 || row.time > now) continue
    if (binding.fields.some((f) => !Number.isFinite(row.values[f]))) continue
    if (binding.fields.includes('CapMVRVCur') && row.values.CapMVRVCur! <= 0) continue
    if (binding.fields.includes('SplyCur') && row.values.SplyCur! <= 0) continue
    if (binding.fields.includes('PriceUSD') && row.values.PriceUSD! <= 0) continue
    const value = binding.calculate
      ? binding.calculate(row.values)
      : row.values[binding.fields[0]!]!
    if (!Number.isFinite(value)) continue
    if (title === 'Fear & Greed Index' && (value < 0 || value > 100)) continue
    const time = utcDay(row.time)
    const provisional = binding.fields.some((f) => row.provisional?.includes(f))
    if (binding.source === 'funding') {
      // The inception day may contain fewer than a full day's settlements.
      if (time === utcDay(sorted[0]!.time) || time >= utcDay(now)) continue
      days.set(time, {
        time,
        value: (days.get(time)?.value ?? 0) + value,
        provisional,
      })
    } else days.set(time, { time, value, provisional })
  }
  const points = [...days.values()].sort((a, b) => a.time - b.time)
  const sourceDays = new Map(sorted.map((row) => [utcDay(row.time), row.values]))
  if (title === 'MVRV Z-Score') {
    // Welford's online variance avoids future observations and cancellation.
    let count = 0,
      mean = 0,
      sumSquares = 0,
      previous = 0
    return points.flatMap((point) => {
      const cap = sourceDays.get(point.time)?.CapMrktCurUSD
      const mvrv = sourceDays.get(point.time)?.CapMVRVCur
      if (!(cap! > 0) || !(mvrv! > 0)) return []
      if (previous && point.time !== previous + DAY) {
        count = 0
        mean = 0
        sumSquares = 0
      }
      previous = point.time
      count++
      const delta = cap! - mean
      mean += delta / count
      sumSquares += delta * (cap! - mean)
      const deviation = Math.sqrt(sumSquares / count)
      return count >= 365 && deviation > 0
        ? [{ ...point, value: (cap! - cap! / mvrv!) / deviation }]
        : []
    })
  }
  if (title === 'Drawdown from ATH') {
    let high = 0
    return points.flatMap((p) => {
      if (p.value <= 0) return []
      high = Math.max(high, p.value)
      return [{ ...p, value: (p.value / high - 1) * 100 }]
    })
  }
  if (['Mayer Multiple', 'Realized Volatility 30D', 'NVT Signal 90D'].includes(title)) {
    const size = title === 'Mayer Multiple' ? 200 : title === 'NVT Signal 90D' ? 90 : 31
    return points.flatMap((point, i) => {
      if (i < size - 1 || point.time - points[i - size + 1]!.time !== (size - 1) * DAY) return []
      const window = points.slice(i - size + 1, i + 1)
      let value: number
      if (title === 'Realized Volatility 30D') {
        if (window.some((p) => p.value <= 0)) return []
        const returns = window.slice(1).map((p, j) => Math.log(p.value / window[j]!.value))
        const mean = returns.reduce((a, b) => a + b, 0) / 30
        value = Math.sqrt((returns.reduce((sum, r) => sum + (r - mean) ** 2, 0) / 30) * 365) * 100
      } else if (title === 'NVT Signal 90D') {
        const volumes = window.map((p) => sourceDays.get(p.time)?.transferUSD)
        if (volumes.some((v) => !Number.isFinite(v) || v! < 0)) return []
        const mean = volumes.reduce<number>((sum, v) => sum + v!, 0) / size
        value = mean > 0 ? sourceDays.get(point.time)!.CapMrktCurUSD! / mean : NaN
      } else {
        const mean = window.reduce((sum, p) => sum + p.value, 0) / size
        value = mean > 0 ? point.value / mean : NaN
      }
      return Number.isFinite(value)
        ? [{ ...point, value, provisional: window.some((p) => p.provisional) }]
        : []
    })
  }
  if (title !== 'Puell Multiple') return points
  return points.flatMap((point, i) => {
    if (i < 364 || point.time - points[i - 364]!.time !== 364 * DAY) return []
    const mean = points.slice(i - 364, i + 1).reduce((sum, p) => sum + p.value, 0) / 365
    return mean > 0 ? [{ ...point, value: point.value / mean }] : []
  })
}

export function bandFor(title: string, value: number): Band | undefined {
  return bindings[title]?.bands?.find((b) => value >= b.min && value < b.max)
}

export function metricTone(
  title: string,
  value: number,
): 'bullish' | 'neutral' | 'caution' | 'bearish' {
  const color = bandFor(title, value)?.color
  return color === red ? 'bearish' : color === amber ? 'caution' : 'neutral'
}

export function formatValue(value: number | undefined | null, unit = '', compact = true): string {
  if (value == null || !Number.isFinite(value)) return '—'
  const digits =
    unit === 'Score' || unit === 'Addresses' || unit === 'Transactions'
      ? 0
      : unit === 'Ratio' || (unit === '%' && Math.abs(value) < 1)
        ? 4
        : 2
  const formatted = new Intl.NumberFormat('en-US', {
    notation: compact && Math.abs(value) >= 10000 ? 'compact' : 'standard',
    maximumFractionDigits: value !== 0 && Math.abs(value) < 0.01 ? 6 : digits,
  }).format(value)
  return `${unit === 'USD' ? '$' : ''}${formatted}${unit === '%' ? '%' : ''}`
}

export interface Outcome {
  horizon: number
  count: number
  median: number | null
  positive: number | null
  baseline: number | null
}
const median = (values: number[]) => {
  if (!values.length) return null
  const sorted = [...values].sort((a, b) => a - b),
    mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 ? sorted[mid]! : (sorted[mid - 1]! + sorted[mid]!) / 2
}

/** Descriptive event study, not a tradable backtest. Enter at next day's close
 * to avoid assuming that a daily on-chain observation was already published.
 * Exact UTC dates are required; missing days and unfinished horizons are omitted.
 */
export function forwardOutcomes(
  points: Point[],
  prices: Point[],
  matches: (value: number) => boolean,
): Outcome[] {
  const priceMap = new Map(prices.map((p) => [utcDay(p.time), p.value]))
  return [7, 30, 90].map((horizon) => {
    const baseline: number[] = [],
      selected: number[] = []
    for (const point of points) {
      const entry = priceMap.get(point.time + DAY),
        end = priceMap.get(point.time + (horizon + 1) * DAY)
      if (entry == null || end == null || entry <= 0 || end <= 0) continue
      const change = (end / entry - 1) * 100
      if (!Number.isFinite(change)) continue
      baseline.push(change)
      if (matches(point.value)) selected.push(change)
    }
    return {
      horizon,
      count: selected.length,
      median: median(selected),
      positive: selected.length
        ? (selected.filter((v) => v > 0).length / selected.length) * 100
        : null,
      baseline: median(baseline),
    }
  })
}

export function percentile(points: Point[], value: number): number | null {
  if (!points.length) return null
  return (points.filter((p) => p.value <= value).length / points.length) * 100
}
