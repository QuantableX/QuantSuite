/** The order book's numbers: steps per pair, grouping with USD values, the
 * compact formats of the four columns. Run: node --test scripts/test-terminal-orderbook.mjs */
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { defaultTick, formatBookPrice, formatCoins, formatUsd, groupLevels, stepDecimals, tickOptions } from '../modules/terminal/app/utils/orderbook.ts'

const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-9 * Math.max(1, Math.abs(expected)), `${actual} != ${expected}`)

test('a pair offers its exchange step up to 2 % of the price and starts where BTC always did', () => {
  const btc = tickOptions(0.01, 83425)
  assert.deepEqual(btc, [0.01, 0.05, 0.1, 0.5, 1, 5, 10, 50, 100, 500, 1000])
  assert.equal(defaultTick(btc, 83425), 10)

  const pepe = tickOptions(0.00000001, 0.00000412)
  assert.deepEqual(pepe, [0.00000001, 0.00000005], 'a coarse exchange step still gets a second choice')
  assert.equal(defaultTick(pepe, 0.00000412), 0.00000001)

  const usdc = tickOptions(0.00001, 1.0001)
  assert.deepEqual(usdc, [0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01])
  assert.equal(defaultTick(usdc, 1.0001), 0.0005)

  assert.deepEqual(tickOptions(0.01, 0), [0.01], 'no price yet: only the exchange step')
  assert.deepEqual(tickOptions(0, 100), [])
})

test('a step tells the decimals of its prices', () => {
  assert.equal(stepDecimals(10), 0)
  assert.equal(stepDecimals(1), 0)
  assert.equal(stepDecimals(0.5), 1)
  assert.equal(stepDecimals(0.05), 2)
  assert.equal(stepDecimals(0.00000001), 8)
  assert.equal(stepDecimals(0.00000005), 8)
  assert.equal(formatBookPrice(83440, stepDecimals(10)), '83440')
  assert.equal(formatBookPrice(0.00000712, stepDecimals(0.00000001)), '0.00000712')
})

test('grouping adds coins per bucket and values every level in USD, cumulative from the spread', () => {
  const asks = [
    { price: 83425.01, amount: 0.2 },
    { price: 83429.5, amount: 0.312 },
    { price: 83436, amount: 1.204 },
  ]
  const ask = groupLevels(asks, 'ask', 83425.01, 10, 3)
  assert.deepEqual(ask.map((l) => l.price), [83450, 83440, 83430], 'highest first, the spread below')
  near(ask[2].amount, 0.512)
  near(ask[2].value, 0.512 * 83430)
  near(ask[1].value, 1.204 * 83440)
  near(ask[1].total, 0.512 * 83430 + 1.204 * 83440)
  assert.equal(ask[0].amount, 0, 'an empty bucket stays in the ladder')
  near(ask[0].total, ask[1].total)

  const bids = [
    { price: 83424.99, amount: 0.83 },
    { price: 83409, amount: 2.115 },
  ]
  const bid = groupLevels(bids, 'bid', 83424.99, 10, 3)
  assert.deepEqual(bid.map((l) => l.price), [83420, 83410, 83400])
  near(bid[0].value, 0.83 * 83420)
  near(bid[2].total, 0.83 * 83420 + 2.115 * 83400)
})

test('a sub-cent price on a bucket edge stays in its bucket', () => {
  const bid = groupLevels([{ price: 0.00000712, amount: 52_300_000 }], 'bid', 0.00000712, 0.00000001, 2)
  assert.deepEqual(bid.map((l) => l.price), [0.00000712, 0.00000711])
  near(bid[0].value, 52_300_000 * 0.00000712)
  assert.deepEqual(groupLevels([], 'ask', 1, 1, 3), [])
})

test('coins and dollars fit the narrow columns in the suite\'s money format', () => {
  assert.equal(formatCoins(1.204), '1.204')
  assert.equal(formatCoins(0.512), '0.512')
  assert.equal(formatCoins(512.34), '512.3')
  assert.equal(formatCoins(12_412), '12.4K')
  assert.equal(formatCoins(5_789_000_000), '5.79B')
  assert.equal(formatCoins(52_300_000), '52.3M')
  assert.equal(formatCoins(0), '0')

  assert.equal(formatUsd(42716.48), '42.7K$')
  assert.equal(formatUsd(143178), '143K$')
  assert.equal(formatUsd(1_254_000), '1.25M$')
  assert.equal(formatUsd(372.38), '372$')
  assert.equal(formatUsd(0.352), '0.352$')
  assert.equal(formatUsd(0), '0$')
})
