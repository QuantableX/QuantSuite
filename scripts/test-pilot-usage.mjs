// QuantPilot V3 card 18: the Usage page's arithmetic.
// Run: node --test scripts/test-pilot-usage.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const U = await import('../modules/pilot/app/utils/usage.ts')

const cost = (input, cacheRead = 0, cacheWrite = 0, output = 0, savings = 0) => ({ input, cacheRead, cacheWrite, output, savings })
const row = (bucket, provider, model, tokens, c) => ({ bucket, provider, model, input: tokens[0], cacheRead: tokens[1], cacheWrite: tokens[2], output: tokens[3], cost: c })

test('totals add priced cost and keep unpriced tokens apart', () => {
  const rows = [
    row(0, 'claude', 'claude-opus-5-5', [10, 1000, 100, 50], cost(1, 2, 3, 4, 5)),
    row(0, 'codex', 'mystery', [5, 0, 0, 5], null),
  ]
  const t = U.totals(rows)
  assert.equal(t.tokens, 1170)
  assert.equal(t.cost, 10)
  assert.equal(t.savings, 5)
  assert.equal(t.unpricedTokens, 10)
})

test('every bucket of the period is there, empty ones too; days follow the calendar', () => {
  const day = (y, m, d) => new Date(y, m, d).getTime()
  // Across the end of daylight saving time in Europe (Oct 25, 2026) the days stay days.
  const since = day(2026, 9, 24) + 5 * 3_600_000
  const starts = U.bucketStarts({ since, bucket: 'day' }, day(2026, 9, 27) + 1000)
  assert.deepEqual(starts, [day(2026, 9, 24), day(2026, 9, 25), day(2026, 9, 26), day(2026, 9, 27)])
  const hours = U.bucketStarts({ since: 0, bucket: 'hour' }, 3 * 3_600_000)
  assert.equal(hours.length, 4)
  const report = {
    since,
    bucket: 'day',
    rows: [row(day(2026, 9, 25), 'claude', 'a', [1, 2, 3, 4], cost(1, 0, 0, 1)), row(day(2026, 9, 25), 'codex', 'b', [1, 0, 0, 1], cost(0.5)), row(day(2026, 9, 27), 'claude', 'a', [0, 0, 0, 10], cost(0, 0, 0, 3))],
  }
  const s = U.series(report, 'provider', null, day(2026, 9, 27) + 1000)
  assert.deepEqual(s.map((b) => b.values), [{}, { claude: 2, codex: 0.5 }, {}, { claude: 3 }])
  const k = U.series(report, 'kind', 'a', day(2026, 9, 27) + 1000)
  assert.deepEqual(k[1].values, { input: 1, cacheRead: 2, cacheWrite: 3, output: 4 })
})

test('the breakdown: costliest first, share, cache hit rate, cost per million, unpriced last', () => {
  const rows = [
    row(0, 'claude', 'opus', [100, 800, 100, 0], cost(3, 0, 0, 0)),
    row(1, 'claude', 'opus', [0, 0, 0, 1000], cost(0, 0, 0, 3)),
    row(0, 'codex', 'gpt', [500, 500, 0, 0], cost(2)),
    row(0, 'codex', 'mystery', [9000, 0, 0, 0], null),
  ]
  const m = U.byModel(rows)
  assert.deepEqual(m.map((x) => x.model), ['opus', 'gpt', 'mystery'])
  assert.equal(m[0].cost, 6)
  assert.equal(m[0].share, 0.75)
  assert.equal(m[0].cacheHitRate, 0.8)
  assert.equal(m[0].costPerMillion, 6 / (2000 / 1e6))
  assert.equal(m[2].cost, null)
  assert.equal(m[2].costPerMillion, null)
})

test('numbers and countdowns read short', () => {
  assert.equal(U.compact(912), '912')
  assert.equal(U.compact(8460), '8.5k')
  assert.equal(U.compact(217_126_957), '217M')
  assert.equal(U.compact(3_400_000_000), '3.4B')
  assert.equal(U.countdown(0 + 134 * 60_000, 0), '2h 14m')
  assert.equal(U.countdown(0 + 76 * 3_600_000, 0), '3d 4h')
  assert.equal(U.countdown(-5, 0), 'now')
  assert.equal(U.windowName('five_hour'), '5-hour')
  assert.equal(U.windowName('seven_day_opus'), 'Weekly · Opus')
  assert.equal(U.windowName('monthly_credits'), 'Monthly credits')
})
