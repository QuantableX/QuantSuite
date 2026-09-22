/** Data-integrity and lifecycle tests. Run: node --test scripts/test-terminal-metrics.mjs */
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { registerHooks } from 'node:module'
import { createPinia, setActivePinia } from 'pinia'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { isReactive } from 'vue'
import {
  DAY,
  metricPoints,
  forwardOutcomes,
  bandFor,
  bindings,
  joinedMetricRows,
} from '../modules/terminal/app/utils/metrics.ts'

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('#terminal/'))
      return nextResolve(
        new URL(`../modules/terminal/app/${specifier.slice(10)}.ts`, import.meta.url).href,
        context,
      )
    return nextResolve(specifier, context)
  },
})
globalThis.window = {}
const { useMetricsStore } = await import('../modules/terminal/app/stores/metrics.ts')
const { cardReading } = await import('../modules/terminal/app/utils/metric-card.ts')
const origin = 1700006400
const row = (day, values, provisional = []) => ({
  time: origin + day * DAY,
  values,
  provisional,
})
async function waitFor(predicate) {
  const deadline = Date.now() + 5000
  while (!predicate()) {
    assert.ok(Date.now() < deadline, 'Timed out waiting for background work')
    await new Promise((resolve) => setTimeout(resolve, 5))
  }
}

test('rolling research metrics require consecutive days and do not use future prices', () => {
  const prices = Array.from({ length: 230 }, (_, i) =>
    row(i, { PriceUSD: 100 * Math.exp(i / 1000) }),
  )
  const mayer = metricPoints('Mayer Multiple', prices)
  assert.equal(mayer.length, 31)
  const expectedMean = prices.slice(0, 200).reduce((sum, r) => sum + r.values.PriceUSD, 0) / 200
  assert.ok(Math.abs(mayer[0].value - prices[199].values.PriceUSD / expectedMean) < 1e-10)
  const future = metricPoints('Mayer Multiple', [...prices, row(230, { PriceUSD: 1000000 })])
  assert.deepEqual(future.slice(0, -1), mayer)
  assert.ok(metricPoints('Realized Volatility 30D', prices).every((p) => p.value < 1e-8))
  const missing = prices.filter((_, i) => i !== 210)
  assert.equal(metricPoints('Realized Volatility 30D', missing).at(-1).time, origin + 209 * DAY)
  assert.deepEqual(
    metricPoints('Drawdown from ATH', [
      row(0, { PriceUSD: 100 }),
      row(1, { PriceUSD: 80 }),
      row(2, { PriceUSD: 120 }),
    ]).map((p) => Math.round(p.value)),
    [0, -20, 0],
  )
})

test('MVRV Z-score uses expanding past-only variance with an explicit warmup', () => {
  const rows = Array.from({ length: 400 }, (_, i) =>
    row(i, { CapMrktCurUSD: 100 + i, CapMVRVCur: 2 }),
  )
  const points = metricPoints('MVRV Z-Score', rows)
  assert.equal(points.length, 36)
  const deviation = Math.sqrt((365 ** 2 - 1) / 12)
  assert.ok(Math.abs(points[0].value - 464 / 2 / deviation) < 1e-10)
  assert.deepEqual(
    metricPoints('MVRV Z-Score', [
      ...rows,
      row(400, { CapMrktCurUSD: 1e12, CapMVRVCur: 10 }),
    ]).slice(0, -1),
    points,
  )
  assert.equal(
    metricPoints(
      'MVRV Z-Score',
      rows.filter((_, i) => i !== 100),
    ).length,
    0,
  )
})

test('NVT smoothing keeps observed zero volume and miner ratios preserve units', () => {
  const rows = Array.from({ length: 90 }, (_, i) =>
    row(i, { transferUSD: i === 0 ? 0 : 10, CapMrktCurUSD: 100 }),
  )
  const points = metricPoints('NVT Signal 90D', rows)
  assert.equal(points.length, 1)
  assert.ok(Math.abs(points[0].value - 100 / (890 / 90)) < 1e-10)
  assert.equal(metricPoints('NVT Signal 90D', rows.slice(1)).length, 0)
  const miner = [row(0, { FeeTotNtv: 1, PriceUSD: 100, IssTotUSD: 900, HashRate: 10000 })]
  assert.equal(metricPoints('Hashprice', miner)[0].value, 100)
  assert.equal(metricPoints('Fee Share of Miner Revenue', miner)[0].value, 10)
})

test('BRK fields retain their units and cohort cards use actual reference levels', () => {
  assert.equal(
    metricPoints('Supply in Profit', [row(0, { supply_in_profit_share: 72.5 })])[0].value,
    72.5,
  )
  assert.equal(
    metricPoints('Net Realized Profit/Loss', [row(0, { net_realized_pnl_sum_24h: -123 })])[0].value,
    -123,
  )
  assert.equal(cardReading('SOPR', [{ time: origin, value: 1.1 }]).baseline, 1)
  assert.equal(cardReading('Short-Term Holder NUPL', [{ time: origin, value: -0.2 }]).baseline, 0)
  assert.equal(cardReading('Supply in Profit', [{ time: origin, value: 72.5 }]).share, 0.725)
})

test('joined metrics use exact dates and OI compares the previous completed day', () => {
  const feeds = {
    'chain-volume': [
      row(0, { transferUSD: 20 }),
      row(1, { transferUSD: 0 }),
      row(2, { transferUSD: 10 }),
    ],
    network: [row(0, { CapMrktCurUSD: 100 }, ['CapMrktCurUSD']), row(1, { CapMrktCurUSD: 120 })],
    positioning: [row(1, { sumOpenInterestValue: 100 }), row(2, { sumOpenInterestValue: 200 })],
    futures: [row(0, { quoteVolume: 50 }), row(2, { quoteVolume: 400 })],
  }
  const nvt = metricPoints(
    'NVT Ratio',
    joinedMetricRows(bindings['NVT Ratio'], (source) => feeds[source] ?? []),
  )
  assert.equal(nvt.length, 1)
  assert.equal(nvt[0].value, 5)
  assert.equal(nvt[0].provisional, true)
  const ratio = metricPoints(
    'OI / 24H Volume',
    joinedMetricRows(bindings['OI / 24H Volume'], (source) => feeds[source] ?? []),
  )
  assert.deepEqual(
    ratio.map((p) => [p.time, p.value]),
    [[origin + DAY, 2]],
  )
})

test('annualized funding uses completed daily sums and taker ratios reject zero divisors', () => {
  const rows = [
    row(0, { fundingRate: 1 }),
    row(1, { fundingRate: 0.0001 }),
    {
      ...row(1, { fundingRate: -0.00005 }),
      time: origin + DAY + 8 * 3600,
    },
    row(2, { fundingRate: 1 }),
  ]
  const points = metricPoints('Annualized Funding', rows, origin + 2 * DAY + 100)
  assert.equal(points.length, 1)
  assert.ok(Math.abs(points[0].value - 1.825) < 1e-10)
  assert.equal(
    metricPoints('Taker Buy/Sell Ratio', [row(0, { takerBuyQuote: 20, takerSellQuote: 10 })])[0]
      .value,
    2,
  )
  assert.equal(
    metricPoints('Taker Buy/Sell Ratio', [row(0, { takerBuyQuote: 20, takerSellQuote: 0 })]).length,
    0,
  )
})

test('joined metric freshness includes failed dependencies while retaining cached observations', async (t) => {
  setActivePinia(createPinia())
  t.after(clearMocks)
  let fail = false
  mockIPC((_, args) => {
    if (args.source === 'network') {
      if (fail) throw new Error('Market cap offline')
      return { rows: [row(0, { CapMrktCurUSD: 100 })], fetchedAt: 100 }
    }
    return { rows: [row(0, { transferUSD: 20 })], fetchedAt: 200 }
  })
  const store = useMetricsStore()
  await Promise.all([store.load('network'), store.load('chain-volume')])
  assert.equal(store.series['NVT Ratio'][0].value, 5)
  assert.equal(store.metricState('NVT Ratio').data.fetchedAt, 100)
  fail = true
  await store.load('network', 'BTC', true)
  assert.match(store.metricState('NVT Ratio').error, /Market cap offline/)
  assert.equal(store.metricState('NVT Ratio').data.fetchedAt, 100)
  assert.equal(store.series['NVT Ratio'][0].value, 5)
})

test('card reference points distinguish cost basis, break-even and positioning', () => {
  const reading = (title, value) => cardReading(title, [{ time: origin, value }])
  assert.equal(reading('MVRV Ratio', 0.8).baseline, 1)
  assert.equal(reading('MVRV Ratio', 0.8).label, 'Below cost basis')
  assert.equal(reading('Puell Multiple', 0.8).baseline, 1)
  assert.equal(reading('Net Unrealized Profit/Loss', -0.1).baseline, 0)
  assert.equal(reading('Exchange Netflow', 0).label, 'Balanced flows')
  assert.equal(reading('Exchange Netflow', -1).label, 'Net outflow')
  assert.equal(reading('Funding Rate', -0.01).label, 'Shorts pay longs')
  assert.equal(reading('Funding Rate', 0.01).label, 'Longs pay shorts')
  assert.equal(reading('Long/Short Ratio', 3).share, 0.75)
  assert.equal(reading('Long/Short Ratio', 1).share, 0.5)
  assert.equal(reading('Long/Short Ratio', 0).share, 0)
  assert.equal(reading('Fear & Greed Index', 100).label, 'Extreme greed')
  assert.equal(cardReading('MVRV Ratio', []).baseline, null)
})

test('activity cards require a complete 30-day window and never treat a missing mean as zero', () => {
  const points = Array.from({ length: 31 }, (_, day) => ({
    time: origin + day * DAY,
    value: day === 0 ? 10000 : 100,
  }))
  assert.equal(cardReading('Active Addresses', points).baseline, 100)
  assert.equal(cardReading('Active Addresses', points).label, 'At 30D average')
  assert.equal(
    cardReading(
      'Active Addresses',
      points.filter((_, i) => i !== 15),
    ).baseline,
    null,
  )
  assert.equal(cardReading('Active Addresses', points.slice(-29)).baseline, null)
  assert.equal(
    cardReading(
      'Open Interest',
      points.map((p) => ({ ...p, value: 0 })),
    ).baseline,
    null,
  )
})

test('realized price cards compare only same-date BTC observations', () => {
  const cost = [{ time: origin, value: 50000 }]
  const spot = [
    { time: origin, value: 75000 },
    { time: origin + DAY, value: 100000 },
  ]
  assert.equal(cardReading('Realized Price', cost, spot).label, 'BTC 50.0% above cost')
  assert.equal(cardReading('Realized Price', cost, spot).baseline, 75000)
  assert.equal(cardReading('Realized Price', cost, spot.slice(1)).baseline, null)
  assert.equal(
    cardReading('Realized Price', cost, [{ time: origin, value: 25000 }]).label,
    'BTC 50.0% below cost',
  )
})

test('NUPL and realized price derive from measured MVRV, preserving missing values and revisions', () => {
  const rows = [
    row(0, { CapMVRVCur: 2, PriceUSD: 60000 }),
    row(1, { CapMVRVCur: 0, PriceUSD: 60000 }),
    row(2, { PriceUSD: 61000 }),
    row(3, { CapMVRVCur: 4, PriceUSD: 80000 }, ['CapMVRVCur']),
  ]
  assert.deepEqual(
    metricPoints('Net Unrealized Profit/Loss', rows).map((p) => p.value),
    [0.5, 0.75],
  )
  assert.deepEqual(
    metricPoints('Realized Price', rows).map((p) => p.value),
    [30000, 20000],
  )
  assert.equal(metricPoints('Realized Price', rows)[1].provisional, true)
  assert.deepEqual(metricPoints('SOPR', rows), [])
})

test('Puell requires 365 consecutive observations and uses only trailing data', () => {
  const rows = Array.from({ length: 366 }, (_, i) => row(i, { IssTotUSD: i === 365 ? 200 : 100 }))
  const points = metricPoints('Puell Multiple', rows)
  assert.equal(points.length, 2)
  assert.equal(points[0].value, 1)
  assert.ok(Math.abs(points[1].value - 200 / (36600 / 365)) < 1e-10)
  assert.deepEqual(
    metricPoints(
      'Puell Multiple',
      rows.filter((_, i) => i !== 180),
    ),
    [],
  )
})

test('funding is summed per complete UTC day, excludes truncated first and current days', () => {
  const funding = (day, hour, value) => ({
    ...row(day, { fundingRate: value }),
    time: origin + day * DAY + hour * 3600,
  })
  const rows = [
    funding(0, 16, 0.01),
    funding(1, 0, 0.0001),
    funding(1, 8, -0.0002),
    funding(1, 16, 0.0003),
    funding(2, 0, 0.01),
  ]
  const points = metricPoints('Funding Rate', rows, origin + 2 * DAY + 3600)
  assert.equal(points.length, 1)
  assert.ok(Math.abs(points[0].value - 0.02) < 1e-10)
})

test('event study uses next-day entry, exact dates, and excludes unfinished outcomes', () => {
  const points = [0, 1, 2].map((i) => ({ time: origin + i * DAY, value: i }))
  const prices = [
    { time: origin, value: 999 },
    { time: origin + DAY, value: 100 },
    { time: origin + 8 * DAY, value: 110 },
    { time: origin + 31 * DAY, value: 90 },
  ]
  const outcomes = forwardOutcomes(points, prices, (v) => v === 0)
  assert.equal(outcomes[0].count, 1)
  assert.ok(Math.abs(outcomes[0].median - 10) < 1e-9)
  assert.equal(outcomes[0].positive, 100)
  assert.ok(Math.abs(outcomes[1].median + 10) < 1e-9)
  assert.equal(outcomes[2].median, null)
  assert.equal(outcomes[2].count, 0)
})

test('zero is a valid exchange netflow; invalid sentiment and non-finite numbers are absent', () => {
  assert.equal(
    metricPoints('Exchange Netflow', [row(0, { FlowInExNtv: 10, FlowOutExNtv: 10 })])[0].value,
    0,
  )
  assert.deepEqual(
    metricPoints('Fear & Greed Index', [
      row(0, { value: -1 }),
      row(1, { value: 101 }),
      row(2, { value: NaN }),
    ]),
    [],
  )
  assert.equal(bandFor('Fear & Greed Index', 57).label, 'Greed')
  assert.equal(bandFor('MVRV Ratio', 1).label, 'Moderate valuation')
})

test('refresh performs real requests, deduplicates in flight, keeps cached observations on failure', async (t) => {
  setActivePinia(createPinia())
  t.after(clearMocks)
  let resolve,
    requests = 0,
    fail = false
  mockIPC(() => {
    requests++
    if (fail) throw new Error('Provider offline')
    return new Promise((done) => {
      resolve = done
    })
  })
  const store = useMetricsStore()
  const first = store.load('network'),
    duplicate = store.load('network', 'BTC', true)
  await waitFor(() => requests === 1)
  assert.equal(requests, 1)
  resolve({ rows: [row(0, { CapMVRVCur: 2 })], fetchedAt: 123 })
  await Promise.all([first, duplicate])
  assert.equal(store.series['MVRV Ratio'][0].value, 2)
  await store.load('network')
  assert.equal(requests, 1)
  fail = true
  await store.load('network', 'BTC', true)
  assert.equal(requests, 2)
  assert.match(store.state('network').error, /Provider offline/)
  assert.equal(store.state('network').data.fetchedAt, 123)
  assert.equal(store.series['MVRV Ratio'][0].value, 2)
})

test('overlapping asset requests stay isolated when responses finish out of order', async (t) => {
  setActivePinia(createPinia())
  t.after(clearMocks)
  const resolve = {}
  mockIPC(
    (_, args) =>
      new Promise((done) => {
        resolve[args.asset] = done
      }),
  )
  const store = useMetricsStore()
  const btc = store.load('price', 'BTC'),
    eth = store.load('price', 'ETH')
  await waitFor(() => resolve.BTC && resolve.ETH)
  resolve.ETH({ rows: [row(0, { close: 3000 })], fetchedAt: 124 })
  await eth
  resolve.BTC({ rows: [row(0, { close: 60000 })], fetchedAt: 123 })
  await btc
  assert.equal(store.state('price', 'ETH').data.rows[0].values.close, 3000)
  assert.equal(store.state('price', 'BTC').data.rows[0].values.close, 60000)
})

test('refresh spam shares the whole batch, bounds concurrency and never restarts finished feeds', async (t) => {
  setActivePinia(createPinia())
  const store = useMetricsStore()
  t.after(() => {
    store.$dispose()
    clearMocks()
  })
  const requests = []
  const releases = new Map()
  let active = 0,
    peak = 0,
    drain = false
  mockIPC((_, args) => {
    requests.push(args.source)
    active++
    peak = Math.max(peak, active)
    return new Promise((resolve) => {
      const release = () => {
        active--
        releases.delete(args.source)
        resolve({
          rows: [row(0, { CapMVRVCur: 2, value: 50 })],
          fetchedAt: 123,
        })
      }
      if (drain) release()
      else releases.set(args.source, release)
    })
  })
  const batch = store.refresh(true)
  assert.equal(store.loading, true)
  assert.equal(store.canRefresh, false)
  const duplicates = Array.from({ length: 25 }, () => store.refresh(true))
  await waitFor(() => requests.length === 3)
  assert.equal(requests[0], 'network', 'Visible chart loads first')
  releases.get('network')()
  await waitFor(() => !store.state('network').loading)
  assert.equal(store.series['MVRV Ratio'][0].value, 2)
  assert.equal(store.loading, true, 'Slow/queued feeds keep the batch locked')
  duplicates.push(store.refresh(true), store.refresh())
  drain = true
  for (const release of [...releases.values()]) release()
  await Promise.all([batch, ...duplicates])
  assert.equal(peak, 3)
  assert.equal(new Set(requests).size, requests.length)
  assert.equal(requests.length, store.refreshTotal)
  assert.equal(store.refreshCompleted, store.refreshTotal)
  assert.equal(store.loading, false)
  const total = requests.length
  await store.refresh(true)
  assert.equal(requests.length, total, 'Rapid clicks after completion respect the cooldown')
  assert.equal(store.canRefresh, false)
})

test('unrelated feed updates and refresh failures retain immutable series and cached data', async (t) => {
  setActivePinia(createPinia())
  const store = useMetricsStore()
  t.after(() => {
    store.$dispose()
    clearMocks()
  })
  let fail = false
  mockIPC(() => {
    if (fail) throw new Error('Provider offline')
    return { rows: [row(0, { CapMVRVCur: 2, value: 50 })], fetchedAt: 123 }
  })
  await store.load('network')
  const points = store.series['MVRV Ratio']
  const data = store.state('network').data
  assert.equal(isReactive(data.rows), false)
  assert.equal(isReactive(points), false)
  await store.load('sentiment')
  assert.equal(store.series['MVRV Ratio'], points)
  fail = true
  const batch = store.refresh(true)
  assert.equal(store.series['MVRV Ratio'], points)
  await batch
  assert.equal(store.state('network').data, data)
  assert.equal(store.series['MVRV Ratio'], points)
  assert.equal(store.loading, false)
  assert.equal(store.refreshCompleted, store.refreshTotal)
  assert.match(store.state('network').error, /Provider offline/)
})

test('queued asset loads deduplicate and disposal stops queued requests and late updates', async (t) => {
  setActivePinia(createPinia())
  const store = useMetricsStore()
  t.after(clearMocks)
  const requests = []
  const releases = []
  mockIPC((_, args) => {
    requests.push(args.asset)
    return new Promise((resolve) => releases.push(resolve))
  })
  const jobs = ['BTC', 'ETH', 'SOL', 'BNB'].map((asset) => store.load('price', asset))
  jobs.push(store.load('price', 'BNB', true))
  await waitFor(() => requests.length === 3)
  assert.equal(store.state('price', 'BNB').loading, true)
  store.$dispose()
  releases.forEach((resolve) => resolve({ rows: [row(0, { close: 100 })], fetchedAt: 123 }))
  await Promise.all(jobs)
  assert.deepEqual(requests, ['BTC', 'ETH', 'SOL'])
  assert.equal(store.state('price', 'BTC').data, null)
  assert.equal(store.loading, false)
})

test('background worker failures release the job and the next calculation can recover', async (t) => {
  const { createMetricsProcessor } =
    await import('../modules/terminal/app/utils/metrics-processing.ts')
  const { affectedMetrics, calculateMetric } =
    await import('../modules/terminal/app/utils/metrics-calculation.ts')
  const workers = []
  const originalWorker = Object.getOwnPropertyDescriptor(globalThis, 'Worker')
  t.after(() => {
    if (originalWorker) Object.defineProperty(globalThis, 'Worker', originalWorker)
    else delete globalThis.Worker
  })
  globalThis.Worker = class {
    constructor() {
      workers.push(this)
    }
    postMessage(data) {
      this.input = data
    }
    terminate() {
      this.terminated = true
    }
  }
  const processor = createMetricsProcessor()
  t.after(() => processor.dispose())
  const input = {
    source: 'network',
    now: origin + DAY,
    histories: {
      network: { rows: [row(0, { CapMVRVCur: 2 })], fetchedAt: 123 },
    },
  }
  const failed = processor.calculate(input)
  await waitFor(() => workers[0]?.input)
  const failure = assert.rejects(failed, /Background metrics processing failed/)
  workers[0].onerror()
  await failure
  assert.equal(workers[0].terminated, true)
  const recovered = processor.calculate(input)
  await waitFor(() => workers[1]?.input)
  const data = workers[1].input
  const series = Object.fromEntries(
    affectedMetrics(data.source).map(([title]) => [title, calculateMetric(title, data)]),
  )
  workers[1].onmessage({ data: { id: data.id, series } })
  assert.equal((await recovered)['MVRV Ratio'][0].value, 2)
})
