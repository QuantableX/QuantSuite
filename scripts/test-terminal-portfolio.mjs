import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { mockIPC } from '@tauri-apps/api/mocks'
import { usePortfolioStore } from '../modules/terminal/app/stores/portfolio.ts'
import { calculateCash, calculateHoldings, emptyPortfolio, portfolioCashHistory, portfolioTotals, validatePortfolio, recordPortfolioSnapshot, portfolioHistory } from '../modules/terminal/app/utils/portfolio.ts'

const clone = value => JSON.parse(JSON.stringify(value))
const at = day => `2025-01-${String(day).padStart(2, '0')}T12:00:00.000Z`
const asset = (id = 'a', extra = {}) => ({ id, symbol: id.toUpperCase(), name: id, source: 'manual', manualPrice: 160, marketName: '', marketPage: 1, updatedAt: at(1), ...extra })
const trade = (id, kind, quantity, price, day, fees = 0) => ({ id, assetId: 'a', kind, quantity, price, date: at(day), fees, notes: '', order: day })
const doc = trades => ({ ...emptyPortfolio(), assets: [asset()], trades })
const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-8, `${actual} != ${expected}`)
const cashEntry = (id, kind, amount, day, extra = {}) => ({ id, kind, amount, date: at(day), notes: '', order: day, ...extra })

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

test('position adjustments set quantity and cost without realizing a trade', () => {
  const d = doc([trade('open', 'opening', 10, 100, 1), trade('sell', 'sell', 2, 150, 2, 2), trade('fix', 'adjustment', 12, 110, 3), trade('later', 'sell', 3, 160, 4, 1)])
  validatePortfolio(d)
  const [h] = calculateHoldings(d)
  assert.equal(h.quantity, 9); assert.equal(h.cost, 990); assert.equal(h.realized, 247)
  const before = calculateHoldings(d, undefined, Date.parse(at(2)))[0]
  assert.equal(before.quantity, 8); assert.equal(before.realized, 98)
})

test('position corrections affect valuations only from their date and remain editable', () => {
  const d = doc([trade('open', 'opening', 10, 100, 1), trade('fix', 'adjustment', 4, 120, 3)])
  recordPortfolioSnapshot(d, at(2)); recordPortfolioSnapshot(d, at(4))
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.cost]), [[1600, 1000], [640, 480]])
  d.trades[1].quantity = 6
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.cost]), [[1600, 1000], [960, 720]])
  d.trades.pop()
  assert.equal(calculateHoldings(d)[0].quantity, 10)
})

test('zero-quantity adjustments are valid but negative values, fees and later oversells are rejected', () => {
  validatePortfolio(doc([trade('open', 'opening', 10, 100, 1), trade('fix', 'adjustment', 0, 0, 2)]))
  for (const entry of [trade('bad', 'buy', 0, 100, 1), trade('bad', 'adjustment', -1, 100, 1), trade('bad', 'adjustment', 1, -1, 1), trade('bad', 'adjustment', 1, 100, 1, 2)]) assert.throws(() => validatePortfolio(doc([entry])))
  assert.throws(() => validatePortfolio(doc([trade('open', 'opening', 10, 100, 1), trade('fix', 'adjustment', 1, 100, 2), trade('sell', 'sell', 2, 160, 3)])), /exceeds/)
})

test('Edit holding saves the entire position atomically while preserving prior entries and realized gains', async () => {
  const d = doc([trade('open', 'opening', 10, 100, 1), trade('sell', 'sell', 2, 150, 2, 2)])
  const prior = clone(d.trades)
  const { store, saved } = setup(d)
  await store.load()
  assert.equal(await store.saveAsset({ ...asset(), manualPrice: 200 }, { quantity: 12, price: 110 }), true)
  assert.deepEqual(saved().trades.slice(0, 2), prior)
  assert.equal(saved().trades[2].kind, 'adjustment')
  assert.equal(store.holdings[0].quantity, 12); assert.equal(store.holdings[0].cost, 1320)
  assert.equal(store.totals.realized, 98); assert.equal(store.totals.value, 2400); assert.equal(store.totals.unrealized, 1080)
  await store.load(); assert.equal(store.holdings[0].quantity, 12)
  assert.equal(await store.saveAsset({ ...asset(), name: 'Renamed' }, { quantity: 12, price: 110 }), true)
  assert.equal(store.document.trades.length, 3)
  assert.equal(await store.saveAsset(asset(), { quantity: 12, price: 90 }), true)
  assert.equal(store.holdings[0].cost, 1080); assert.equal(store.document.trades.length, 4)
})

test('direct edits can close and reopen positions without adding realized gains', async () => {
  const d = doc([trade('open', 'opening', 10, 100, 1), trade('sell', 'sell', 2, 150, 2)])
  const { store } = setup(d)
  await store.load()
  assert.equal(await store.saveAsset(asset(), { quantity: 0, price: 100 }), true)
  assert.equal(store.totals.count, 0); assert.equal(store.totals.cost, 0); assert.equal(store.totals.realized, 100)
  assert.equal(await store.saveAsset(asset(), { quantity: 3, price: 120 }), true)
  assert.equal(store.holdings[0].quantity, 3); assert.equal(store.holdings[0].cost, 360); assert.equal(store.totals.realized, 100)
})

test('failed position correction leaves both asset metadata and the original ledger intact', async () => {
  const d = doc([trade('open', 'opening', 2, 100, 1)])
  const { store, saved } = setup(d, { failWrite: true })
  await store.load()
  assert.equal(await store.saveAsset({ ...asset(), name: 'Changed' }, { quantity: 4, price: 90 }), false)
  assert.deepEqual(saved(), d); assert.deepEqual(clone(store.document), d)
})

test('invalid direct quantities are rejected and adjustment deletion respects subsequent sales', async () => {
  const d = doc([trade('open', 'opening', 1, 100, 1), trade('fix', 'adjustment', 4, 100, 2), trade('sell', 'sell', 3, 120, 3)])
  const { store, writes } = setup(d)
  await store.load()
  for (const quantity of [-1, NaN, Infinity]) assert.equal(await store.saveAsset(asset(), { quantity, price: 100 }), false)
  assert.equal(await store.removeTrade('fix'), false)
  assert.equal(writes(), 0); assert.equal(store.document.trades.length, 3)
})

test('holding notes are optional for existing portfolios and reject invalid saved text', () => {
  const d = doc([trade('open', 'opening', 2, 100, 1)])
  validatePortfolio(d)
  d.assets[0].notes = 'Thesis: long-term growth\nEntry criteria: support holds\nExit: thesis invalidated'
  validatePortfolio(d)
  for (const notes of [null, 3, {}, 'x'.repeat(10001)]) {
    d.assets[0].notes = notes
    assert.throws(() => validatePortfolio(d), /Holding notes/)
  }
})

test('holding notes persist, can be cleared and do not alter the trade ledger or quantities', async () => {
  const d = doc([trade('open', 'opening', 2, 100, 1)])
  const { store, saved } = setup(d)
  await store.load()
  const notes = 'Thesis / Theorie\nEntry criteria: hold above support\nRisks: earnings gap'
  assert.equal(await store.saveAsset({ ...asset(), notes }, { quantity: 2, price: 100 }), true)
  await store.load()
  assert.equal(store.document.assets[0].notes, notes)
  assert.deepEqual(saved().trades, d.trades)
  assert.equal(store.holdings[0].quantity, 2)
  assert.equal(await store.saveAsset({ ...store.document.assets[0], notes: '' }), true)
  await store.load()
  assert.equal(store.document.assets[0].notes, '')
})

test('notes survive crypto refresh and a later position correction', async () => {
  const d = doc([trade('open', 'opening', 2, 100, 1)])
  d.assets[0] = asset('a', { source: 'market', symbol: 'BTC', marketName: 'Bitcoin', notes: 'Thesis and entry criteria' })
  const { store } = setup(d, { market: () => [{ symbol: 'BTC', name: 'Bitcoin', price: 500 }] })
  await store.load(); await store.refreshPrices()
  assert.equal(await store.saveAsset({ ...store.document.assets[0] }, { quantity: 3, price: 120 }), true)
  await store.load()
  assert.equal(store.document.assets[0].notes, 'Thesis and entry criteria')
  assert.equal(store.holdings[0].quantity, 3)
})

test('legacy portfolios start with zero cash without changing their trades or gains', async () => {
  const d = doc([trade('buy', 'buy', 2, 100, 1), trade('sell', 'sell', 1, 150, 2)])
  delete d.cashEntries
  const { store, saved } = setup(d)
  await store.load()
  assert.equal(store.cash, 0); assert.equal(store.totals.totalGain, 110)
  assert.equal(await store.saveCashEntry(cashEntry('cash', 'balance', 800, 3)), true)
  assert.deepEqual(saved().trades, d.trades)
  assert.equal(store.totals.value, 960); assert.equal(store.totals.totalGain, 110)
  assert.equal(await store.saveTrade({ ...d.trades[0], price: 110 }), true)
  assert.equal(store.cash, 800); assert.equal(store.document.trades[0].settlesCash, false)
  assert.equal(await store.saveTrade({ ...d.trades[0], settlesCash: true }), false)
  assert.match(store.error, /Insufficient cash/)
})

test('cash-only portfolios support deposits, withdrawals, absolute corrections and historical balances', () => {
  const d = { ...emptyPortfolio(), cashEntries: [cashEntry('1', 'deposit', 1000, 1), cashEntry('2', 'withdrawal', 200, 2), cashEntry('3', 'balance', 500, 3), cashEntry('4', 'deposit', 50, 4)] }
  validatePortfolio(d)
  assert.equal(calculateCash(d), 550); assert.equal(calculateCash(d, Date.parse(at(2))), 800)
  assert.deepEqual(portfolioCashHistory(d).map(c => [c.amount, c.balance]), [[1000, 1000], [-200, 800], [-300, 500], [50, 550]])
  const totals = portfolioTotals([], calculateCash(d))
  assert.equal(totals.value, 550); assert.equal(totals.totalGain, 0); assert.equal(totals.cost, 0)
  recordPortfolioSnapshot(d, at(2)); recordPortfolioSnapshot(d, at(4))
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.cost, p.gain]), [[800, 0, 0], [550, 0, 0]])
})

test('new buys and sells settle gross plus buy fees and net sale proceeds by default', async () => {
  const { store, saved } = setup(doc([]))
  await store.load()
  assert.equal(await store.saveCashEntry(cashEntry('cash', 'deposit', 1000, 1)), true)
  assert.equal(await store.saveTrade(trade('buy', 'buy', 4, 100, 2, 10)), true)
  assert.equal(store.cash, 590); assert.equal(store.holdings[0].cost, 410)
  assert.equal(store.totals.value, 1230); assert.equal(store.totals.totalGain, 230)
  assert.equal(await store.saveTrade(trade('sell', 'sell', 2, 150, 3, 5)), true)
  assert.equal(store.cash, 885); assert.equal(store.totals.realized, 90)
  assert.equal(store.totals.value, 1205); assert.equal(store.totals.totalGain, 205)
  assert.deepEqual(saved().trades.map(t => t.settlesCash), [true, true])
  assert.deepEqual(store.cashHistory.map(c => c.balance), [1000, 590, 885])
  await store.load(); assert.equal(store.cash, 885)
  assert.equal(await store.saveTrade({ ...store.document.trades[1], price: 200 }), true)
  assert.equal(store.cash, 985); assert.equal(store.totals.realized, 190)
  assert.equal(await store.removeTrade('sell'), true)
  assert.equal(store.cash, 590); assert.equal(store.holdings[0].quantity, 4)
  assert.equal(await store.removeTrade('buy'), true)
  assert.equal(store.cash, 1000); assert.equal(store.totals.totalGain, 0)
})

test('insufficient cash is rejected at the entry date even when a later deposit would cover it', async () => {
  const d = doc([]); d.cashEntries = [cashEntry('cash', 'deposit', 1000, 3)]
  const { store, saved, writes } = setup(d)
  await store.load()
  assert.equal(await store.saveTrade(trade('buy', 'buy', 1, 100, 2)), false)
  assert.equal(await store.saveCashEntry(cashEntry('out', 'withdrawal', 1001, 4)), false)
  assert.equal(writes(), 0); assert.deepEqual(saved(), d); assert.equal(store.cash, 1000)
})

test('cash edits and deletes preserve prior state if a later buy or withdrawal would overdraw', async () => {
  const d = doc([{ ...trade('buy', 'buy', 2, 100, 2), settlesCash: true }])
  d.cashEntries = [cashEntry('cash', 'deposit', 300, 1), cashEntry('out', 'withdrawal', 90, 3)]
  const { store, saved, writes } = setup(d)
  await store.load()
  assert.equal(await store.saveCashEntry({ ...d.cashEntries[0], amount: 200 }), false)
  assert.equal(await store.removeCashEntry('cash'), false)
  assert.equal(await store.saveTrade({ ...d.trades[0], price: 151 }), false)
  assert.equal(writes(), 0); assert.deepEqual(saved(), d); assert.equal(store.cash, 10)
  assert.equal(await store.saveCashEntry({ ...d.cashEntries[0], amount: 400, notes: 'Transfer corrected' }), true)
  assert.equal(store.cash, 110)
  assert.equal(await store.removeCashEntry('out'), true); assert.equal(store.cash, 200)
  await store.load(); assert.equal(store.cash, 200)
  assert.equal(store.document.cashEntries[0].notes, 'Transfer corrected')
})

test('removing a sale or its asset cannot orphan cash already withdrawn', async () => {
  const d = doc([trade('open', 'opening', 2, 100, 1), { ...trade('sell', 'sell', 1, 150, 2), settlesCash: true }])
  d.cashEntries = [cashEntry('out', 'withdrawal', 100, 3)]
  const { store, saved } = setup(d)
  await store.load()
  assert.equal(await store.removeTrade('sell'), false); assert.equal(await store.removeAsset('a'), false)
  assert.deepEqual(saved(), d); assert.equal(store.cash, 50)
  assert.equal(await store.removeCashEntry('out'), true)
  assert.equal(await store.removeAsset('a'), true)
  assert.equal(store.cash, 0); assert.equal(store.document.assets.length, 0)
})

test('same-timestamp cash and trade entries keep insertion order across edits', async () => {
  const { store } = setup(doc([]))
  await store.load()
  assert.equal(await store.saveCashEntry(cashEntry('z', 'deposit', 200, 1)), true)
  assert.equal(await store.saveTrade(trade('a', 'buy', 1, 100, 1)), true)
  assert.equal(await store.saveCashEntry(cashEntry('b', 'withdrawal', 50, 1)), true)
  assert.deepEqual(store.cashHistory.map(c => c.id), ['z', 'a', 'b'])
  assert.equal(await store.saveCashEntry({ ...store.document.cashEntries[0], amount: 220 }), true)
  assert.deepEqual(store.cashHistory.map(c => c.id), ['z', 'a', 'b']); assert.equal(store.cash, 70)
})

test('opening holdings and direct position corrections leave cash and prior gains intact', async () => {
  const { store } = setup()
  await store.load()
  assert.equal(await store.saveCashEntry(cashEntry('cash', 'balance', 500, 1)), true)
  assert.equal(await store.saveAsset(asset(), { quantity: 4, price: 100, date: at(1) }), true)
  assert.equal(await store.saveAsset(asset(), { quantity: 8, price: 100 }), true)
  assert.equal(store.cash, 500); assert.equal(store.totals.realized, 0)
  assert.equal(store.totals.totalGain, 480); assert.equal(store.totals.value, 1780)
})

test('recorded values include cash while cash transfers never appear as gain', () => {
  const d = doc([{ ...trade('buy', 'buy', 2, 100, 2), settlesCash: true }])
  d.cashEntries = [cashEntry('cash', 'deposit', 1000, 1), cashEntry('out', 'withdrawal', 100, 3)]
  for (const day of [1, 2, 3]) recordPortfolioSnapshot(d, at(day))
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.gain]), [[1000, 0], [1120, 120], [1020, 120]])
  d.cashEntries[0].amount = 2000
  assert.deepEqual(portfolioHistory(d).map(p => [p.value, p.gain]), [[2000, 0], [2120, 120], [2020, 120]])
  d.assets[0].manualPrice = null; recordPortfolioSnapshot(d, at(4))
  assert.equal(portfolioHistory(d).at(-1).value, null)
  assert.equal(portfolioTotals(calculateHoldings(d), calculateCash(d)).totalGain, null)
})

test('cash validates zero balances, decimals, corrupt entries and overflow', () => {
  const d = { ...emptyPortfolio(), cashEntries: [cashEntry('1', 'deposit', .1, 1), cashEntry('2', 'deposit', .2, 2), cashEntry('3', 'withdrawal', .3, 3), cashEntry('4', 'balance', 0, 4)] }
  validatePortfolio(d); assert.equal(calculateCash(d), 0)
  for (const extra of [{ amount: -1 }, { amount: NaN }, { amount: Infinity }, { amount: 0 }, { kind: 'other' }, { date: 'bad' }, { date: '2099-01-01' }, { order: -1 }, { notes: 42 }, { notes: 'x'.repeat(2001) }]) {
    assert.throws(() => validatePortfolio({ ...emptyPortfolio(), cashEntries: [cashEntry('bad', 'deposit', 10, 1, extra)] }))
  }
  for (const cashEntries of [null, {}, [null], [cashEntry('same', 'deposit', 1, 1), cashEntry('same', 'deposit', 1, 2)], [cashEntry('1', 'deposit', 1e15, 1), cashEntry('2', 'deposit', 1e15, 2)]]) assert.throws(() => validatePortfolio({ ...emptyPortfolio(), cashEntries }))
  assert.throws(() => validatePortfolio(doc([{ ...trade('open', 'opening', 1, 100, 1), settlesCash: true }])), /Only buys/)
  assert.throws(() => validatePortfolio(doc([{ ...trade('buy', 'buy', 1, 100, 1), settlesCash: 'yes' }])), /settlement/)
})

test('cash changes remain atomic on failed writes and stale revisions', async () => {
  const d = { ...emptyPortfolio(), cashEntries: [cashEntry('cash', 'balance', 100, 1)] }
  const failed = setup(d, { failWrite: true })
  await failed.store.load()
  assert.equal(await failed.store.saveCashEntry(cashEntry('in', 'deposit', 50, 2)), false)
  assert.equal(await failed.store.removeCashEntry('cash'), false)
  assert.deepEqual(failed.saved(), d); assert.equal(failed.store.cash, 100)
  const stale = setup(d)
  await stale.store.load()
  stale.external({ ...d, revision: 'newer', cashEntries: [cashEntry('cash', 'balance', 200, 1)] })
  assert.equal(await stale.store.saveCashEntry(cashEntry('in', 'deposit', 50, 2), ''), false)
  assert.equal(stale.store.cash, 200); assert.equal(stale.writes(), 0)
})
