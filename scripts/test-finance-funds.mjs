import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { mockIPC } from '@tauri-apps/api/mocks'
import { useFundsStore } from '../modules/finance/app/stores/funds.ts'
import { usePlanStore } from '../modules/finance/app/stores/plan.ts'
import { parseFundCents } from '../modules/finance/app/utils/fund-money.ts'
import registry from '../modules/registry.generated.json' with { type: 'json' }

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
function setup(handler) {
  globalThis.window = { crypto: globalThis.crypto }
  setActivePinia(createPinia())
  mockIPC(handler)
  return useFundsStore()
}
const fund = (id) => ({ id, name: id, currentValueCents: 15000, netInputCents: 10000, gainCents: 5000 })

test('exact money parsing accepts locale grouping without truncating invalid amounts', () => {
  for (const [text, expected] of [['0', 0], ['1.234', 123400], ['1,234', 123400], ['1.234,56', 123456], ['1,234.56', 123456], ['1 234,5', 123450], ['0,01', 1], ['90071992547409.91', Number.MAX_SAFE_INTEGER]]) assert.equal(parseFundCents(text), expected, text)
  for (const text of ['', '-1', 'NaN', '1e3', 'abc100', '1.2345', '1,23,45', '1,234.567', '1.', '90071992547409.92']) assert.equal(parseFundCents(text), null, text)
})

test('a slower refresh cannot overwrite a newer fund snapshot or selection', async () => {
  const first = deferred(), second = deferred()
  let calls = 0
  const store = setup(() => (++calls === 1 ? first.promise : second.promise))
  const a = store.load(), b = store.load()
  second.resolve([fund('new')]); await b
  first.resolve([fund('old')]); await a
  assert.equal(store.selectedId, 'new')
  assert.deepEqual(store.totals, { value: 15000, input: 10000, gain: 5000 })
})

test('load errors preserve last known balances and expose the failure', async () => {
  const store = setup(() => [fund('emergency')])
  await store.load()
  mockIPC(() => { throw new Error('database unavailable') })
  await store.load()
  assert.equal(store.selectedId, 'emergency')
  assert.match(store.error, /database unavailable/)
  assert.equal(store.loading, false)
})

test('double clicks book only one installment and forward its expected date', async () => {
  const pending = deferred()
  const writes = []
  const store = setup((command, args) => {
    if (command === 'plugin:finance|funds_overview') return [fund('f')]
    writes.push({ command, args }); return pending.promise
  })
  const first = store.bookPlan('p', '2026-09-16')
  assert.equal(await store.bookPlan('p', '2026-09-16'), false)
  pending.resolve(null)
  assert.equal(await first, true)
  assert.deepEqual(writes, [{ command: 'plugin:finance|book_fund_plan', args: { id: 'p', expectedOn: '2026-09-16' } }])
  assert.equal(store.busy, false)
})

test('successful writes stay successful when only the subsequent refresh fails', async () => {
  const store = setup((command) => {
    if (command === 'plugin:finance|funds_overview') throw new Error('refresh failed')
    return 'new-fund'
  })
  assert.equal(await store.saveFund({ name: 'New' }), true)
  assert.match(store.error, /refresh failed/)
  assert.equal(store.selectedId, 'new-fund')
})

test('failed writes keep balances and allow retry', async () => {
  const store = setup(() => [fund('existing')])
  await store.load()
  mockIPC(() => { throw new Error('would overdraw') })
  assert.equal(await store.remove('entry', 'deposit'), false)
  assert.equal(store.selected.currentValueCents, 15000)
  assert.equal(store.busy, false)
  assert.match(store.error, /would overdraw/)
})

test('plan refreshes cannot restore an older linked fund balance', async () => {
  const older = deferred(), newer = deferred()
  let calls = 0
  setup((command) => {
    if (command === 'plugin:finance|list_items') return ++calls === 1 ? older.promise : newer.promise
    if (command === 'plugin:finance|goals') return []
    return {}
  })
  const store = usePlanStore()
  const a = store.load(), b = store.load()
  newer.resolve([{ id: 'fund', fundId: 'fund', kind: 'saving', savedCents: 9000 }]); await b
  older.resolve([{ id: 'fund', fundId: 'fund', kind: 'saving', savedCents: 10000 }]); await a
  assert.equal(store.saving[0].savedCents, 9000)
})

test('rejected fund rate edits restore the persisted row and show the reason', async () => {
  const row = { id: 'fund', fundId: 'fund', kind: 'saving', amountCents: 5000 }
  setup((command) => {
    if (command === 'plugin:finance|update_item') throw new Error('Edit individual savings plans')
    if (command === 'plugin:finance|list_items') return [{ ...row }]
    if (command === 'plugin:finance|goals') return []
    return {}
  })
  const store = usePlanStore()
  await store.load()
  await store.update('fund', { amountCents: 9000 })
  assert.equal(store.saving[0].amountCents, 5000)
  assert.match(store.error, /Edit individual savings plans/)
})

test('QuantFlow is the single Zen entry for calendar and habits, including capabilities', () => {
  assert.deepEqual(registry.apps.find((app) => app.id === 'quantzen').modules, ['notes', 'flow', 'finance'])
  assert.ok(!registry.apps.find((app) => app.id === 'quantagent').modules.includes('flow'))
  assert.ok(!registry.modules.some((module) => ['plan', 'habit'].includes(module.id)))
  const capabilities = registry.capabilities.filter((c) => c.module === 'flow')
  assert.ok(capabilities.some((c) => c.command === 'plugin:plan|create_event'))
  assert.ok(capabilities.some((c) => c.command === 'plugin:habit|set_check'))
})
