import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { mockIPC } from '@tauri-apps/api/mocks'
import { usePortfolioStore } from '../modules/terminal/app/stores/portfolio.ts'
import { calculateHoldings, emptyPortfolio, portfolioTotals, validatePortfolio, recordPortfolioSnapshot, portfolioHistory } from '../modules/terminal/app/utils/portfolio.ts'

const clone = value => JSON.parse(JSON.stringify(value))
const at = day => `2025-01-${String(day).padStart(2, '0')}T12:00:00.000Z`
const asset = (id = 'a', extra = {}) => ({ id, symbol: id.toUpperCase(), name: id, source: 'manual', manualPrice: 160, marketName: '', marketPage: 1, updatedAt: at(1), ...extra })
const trade = (id, kind, quantity, price, day, fees = 0) => ({ id, assetId: 'a', kind, quantity, price, date: at(day), fees, notes: '', order: day })
const doc = trades => ({ ...emptyPortfolio(), assets: [asset()], trades })
const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-8, `${actual} != ${expected}`)

test('average cost includes buy fees and partial sales realize cost proportionally', () => {
  const d = doc([trade('open', 'opening', 10, 100, 1), trade('buy', 'buy', 10, 120, 2, 20), trade('sell', 'sell', 5, 150, 3, 5)])
  validatePortfolio(d)
  const [h] = calculateHoldings(d)
  assert.equal(h.quantity, 15); assert.equal(h.cost, 1665); assert.equal(h.averageCost, 111)
  assert.equal(h.realized, 190); assert.equal(h.value, 2400); assert.equal(h.unrealized, 735)
  assert.equal(portfolioTotals([h]).totalGain, 925)
})

test('editing a historical purchase recalculates later realized and unrealized gains', () => {
  const d = doc([trade('buy', 'buy', 10, 100, 1), trade('sell', 'sell', 4, 150, 2)])
  d.trades[0].price = 120
  const [h] = calculateHoldings(d)
  assert.equal(h.cost, 720); assert.equal(h.realized, 120); assert.equal(h.unrealized, 240)
  d.trades[1].quantity = 10
  const [closed] = calculateHoldings(d)
  assert.equal(closed.quantity, 0); assert.equal(closed.cost, 0); assert.equal(closed.unrealized, 0)
})

test('closed positions retain realized gains and a later buy gets a new cost basis', () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1), trade('sell', 'sell', 2, 90, 2, 2), trade('reopen', 'buy', 1, 130, 3)])
  const [h] = calculateHoldings(d)
  assert.equal(h.quantity, 1); assert.equal(h.averageCost, 130); assert.equal(h.realized, -22)
  assert.equal(portfolioTotals([h]).totalGain, 8)
})

test('backdated oversells and deleting an earlier acquisition are rejected', () => {
  assert.throws(() => validatePortfolio(doc([trade('buy', 'buy', 2, 100, 2), trade('sell', 'sell', 1, 110, 1)])), /exceeds/)
  assert.throws(() => validatePortfolio(doc([trade('sell', 'sell', 1, 110, 2)])), /exceeds/)
  assert.throws(() => validatePortfolio(doc([trade('buy', 'buy', 1, 100, 1), trade('sell', 'sell', 1.01, 110, 2)])), /exceeds/)
})

test('fractional quantities close cleanly without allowing materially oversold tiny assets', () => {
  const [h] = calculateHoldings(doc([trade('a', 'buy', .1, 100, 1), trade('b', 'buy', .2, 100, 2), trade('c', 'sell', .3, 110, 3)]))
  assert.equal(h.quantity, 0); near(h.realized, 3)
  assert.throws(() => validatePortfolio(doc([trade('a', 'buy', 1e-12, 1, 1), trade('b', 'sell', 2e-12, 1, 2)])), /exceeds/)
})

test('missing prices differ from zero prices and never create a false total gain', () => {
  const d = doc([trade('a', 'buy', 1, 100, 1)])
  d.assets[0].manualPrice = null
  let totals = portfolioTotals(calculateHoldings(d))
  assert.equal(totals.missing, 1); assert.equal(totals.totalGain, null)
  d.assets[0].manualPrice = 0
  totals = portfolioTotals(calculateHoldings(d))
  assert.equal(totals.missing, 0); assert.equal(totals.totalGain, -100)
})

test('history uses recorded prices, while ledger corrections recompute quantities and cost', () => {
  const d = doc([trade('a', 'buy', 2, 100, 1)])
  recordPortfolioSnapshot(d, at(2))
  d.assets[0].manualPrice = 200
  recordPortfolioSnapshot(d, at(3))
  d.trades[0].quantity = 3
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.cost]), [[480, 300], [600, 300]])
  recordPortfolioSnapshot(d, at(3).replace('12:00', '18:00'))
  assert.equal(d.snapshots.length, 2)
  assert.equal(portfolioHistory(d).length, 2)
})

test('retroactively added assets leave gaps where no historical valuation is known', () => {
  const d = doc([trade('a', 'buy', 1, 100, 1)])
  recordPortfolioSnapshot(d, at(2))
  d.assets.push(asset('b'))
  d.trades.push({ ...trade('b', 'buy', 1, 20, 1), assetId: 'b' })
  assert.equal(portfolioHistory(d)[0].value, null)
})

test('invalid and corrupt documents fail without normalizing away user data', () => {
  for (const value of [null, {}, { ...emptyPortfolio(), version: 2 }, { ...emptyPortfolio(), assets: [null] }, doc([trade('bad', 'buy', NaN, 1, 1)]), doc([trade('bad', 'buy', 1, -1, 1)])]) assert.throws(() => validatePortfolio(value))
  const d = doc([trade('id', 'buy', 1, 1, 1), trade('id', 'buy', 1, 1, 2)])
  assert.throws(() => validatePortfolio(d))
})

function setup(initial = null, options = {}) {
  globalThis.window = { crypto: globalThis.crypto }
  let saved = clone(initial)
  let writes = 0
  mockIPC(async (command, args) => {
    if (command === 'plugin:qs|get_setting') return clone(saved)
    if (command === 'plugin:qs|set_setting') {
      if (options.failWrite) throw new Error('disk full')
      writes++; saved = clone(args.value); return null
    }
    if (command === 'plugin:terminal|get_marketcap') return options.market?.(args) ?? []
    throw new Error(`Unexpected ${command}`)
  })
  setActivePinia(createPinia())
  return { store: usePortfolioStore(), saved: () => saved, writes: () => writes, external: value => { saved = clone(value) } }
}

test('opening holdings and trade corrections persist in native storage and survive reload', async () => {
  const { store, saved } = setup()
  await store.load()
  assert.equal(await store.saveAsset(asset(), { quantity: 3, price: 100, date: at(1) }), true)
  const id = store.document.trades[0].id
  assert.equal(await store.saveTrade({ ...store.document.trades[0], quantity: 4 }), true)
  assert.equal(saved().trades[0].quantity, 4)
  await store.load()
  assert.equal(store.holdings[0].quantity, 4)
  assert.equal(store.document.trades[0].id, id)
})

test('write failure leaves saved and displayed data unchanged', async () => {
  const { store, saved } = setup(doc([trade('a', 'buy', 1, 100, 1)]), { failWrite: true })
  await store.load()
  assert.equal(await store.saveAsset(asset('b')), false)
  assert.equal(store.document.assets.length, 1); assert.equal(saved().assets.length, 1)
  assert.match(store.error, /disk full/)
})

test('invalid deletion keeps original history; asset deletion removes only its own trades', async () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1), trade('sell', 'sell', 1, 110, 2)])
  d.assets.push(asset('b')); d.trades.push({ ...trade('b', 'buy', 2, 30, 1), assetId: 'b' })
  const { store, writes } = setup(d)
  await store.load()
  assert.equal(await store.removeTrade('buy'), false); assert.equal(writes(), 0)
  assert.equal(await store.removeAsset('a'), true)
  assert.deepEqual(store.document.trades.map(t => t.id), ['b'])
})

test('stale editor revision cannot overwrite a change from another window', async () => {
  const { store, external, writes } = setup()
  await store.load()
  external({ ...emptyPortfolio(), revision: 'another', assets: [asset('external')] })
  assert.equal(await store.saveAsset(asset()), false)
  assert.equal(writes(), 0); assert.equal(store.document.assets[0].id, 'external')
  assert.match(store.error, /another window/)
})

test('live quotes match both symbol and name; unpriced assets stay unknown', async () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1)])
  d.assets[0] = asset('a', { source: 'market', symbol: 'BTC', marketName: 'Bitcoin' })
  const { store } = setup(d, { market: () => [{ symbol: 'BTC', name: 'Unrelated coin', price: 1 }, { symbol: 'BTC', name: 'Bitcoin', price: 500 }] })
  await store.load(); assert.equal(store.totals.missing, 1)
  await store.refreshPrices(); assert.equal(store.totals.value, 1000)
})

test('quote failures preserve last prices and switching assets invalidates previous quotes', async () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1)])
  d.assets[0] = asset('a', { source: 'market', symbol: 'BTC', marketName: 'Bitcoin' })
  d.quotes.a = { price: 500, checkedAt: at(2) }
  const { store } = setup(d, { market: () => { throw new Error('offline') } })
  await store.load(); await store.refreshPrices()
  assert.equal(store.totals.value, 1000); assert.match(store.quoteError, /offline/)
  assert.equal(await store.saveAsset({ ...d.assets[0], symbol: 'ETH', marketName: 'Ethereum' }), true)
  assert.equal(store.totals.missing, 1)
})

test('a corrupt saved portfolio is never replaced by an empty or sample portfolio', async () => {
  const { store, writes } = setup({ version: 99 })
  await store.load()
  assert.equal(store.loaded, false); assert.match(store.error, /preserved/)
  assert.equal(await store.saveAsset(asset()), false); assert.equal(writes(), 0)
})

test('a delayed quote cannot overwrite a newer manual price edit', async () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1)])
  d.assets[0] = asset('a', { source: 'market', symbol: 'BTC', marketName: 'Bitcoin' })
  let finish
  const { store } = setup(d, { market: () => new Promise(resolve => { finish = resolve }) })
  await store.load()
  const refresh = store.refreshPrices()
  assert.equal(await store.saveAsset({ ...d.assets[0], source: 'manual', manualPrice: 450 }), true)
  finish([{ symbol: 'BTC', name: 'Bitcoin', price: 500 }])
  await refresh
  assert.equal(store.totals.value, 900)
  assert.equal(store.document.quotes.a, undefined)
})

test('browser fallback persists the ledger and refuses storage failures', async () => {
  globalThis.window = {}
  const entries = new Map()
  let fail = false
  globalThis.localStorage = {
    getItem: key => entries.get(key) ?? null,
    setItem: (key, value) => { if (fail) throw new Error('quota exceeded'); entries.set(key, value) },
  }
  setActivePinia(createPinia())
  const store = usePortfolioStore()
  await store.load()
  assert.equal(await store.saveAsset(asset(), { quantity: 2, price: 100, date: at(1) }), true)
  await store.load()
  assert.equal(store.holdings[0].quantity, 2)
  fail = true
  assert.equal(await store.removeAsset('a'), false)
  assert.equal(store.holdings[0].quantity, 2)
  assert.match(store.error, /quota exceeded/)
  delete globalThis.localStorage
})
