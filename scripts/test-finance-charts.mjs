import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computed } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { mockIPC } from '@tauri-apps/api/mocks'
import { useFundsStore } from '../modules/finance/app/stores/funds.ts'
import { flowPieSlices, fundPieSlices, layoutPie } from '../modules/finance/app/utils/pie.ts'

const node = (id, layer, kind, valueCents, fundId = null) => ({ id, layer, kind, color: kind, label: id, valueCents, fundId })
const fund = (id, currentValueCents) => ({ id, name: id, currentValueCents, openingCents: 90000, netInputCents: 70000 })

test('budget pie counts destinations once, preserving fund links and positive leftover', () => {
  const slices = flowPieSlices({ nodes: [
    node('salary', 0, 'income', 300000), node('hub', 1, 'hub', 300000),
    node('saving', 2, 'saving', 50000, 'fund-1'), node('rent', 2, 'expense', 100000),
    node('leftover', 2, 'leftover', 150000), node('zero', 2, 'expense', 0),
  ] })
  assert.deepEqual(slices.map((s) => s.id), ['saving', 'rent', 'leftover'])
  assert.equal(layoutPie(slices).total, 300000)
  assert.equal(slices[0].fundId, 'fund-1')
  assert.equal(slices[0].color, 'var(--qf-flow-saving)')
})

test('an overspent or income-free plan uses spending as denominator, never a negative slice', () => {
  for (const income of [100000, 0]) {
    const slices = flowPieSlices({ nodes: [node('income', 0, 'income', income),
      node('rent', 2, 'expense', 125000), node('saving', 2, 'saving', 25000),
      node('leftover', 2, 'leftover', income - 150000)] })
    assert.equal(layoutPie(slices).total, 150000)
    assert.deepEqual(slices.map((s) => s.valueCents), [125000, 25000])
  }
})

test('empty, zero and single-slice pies produce finite, complete geometry', () => {
  assert.deepEqual(layoutPie(flowPieSlices(null)), { total: 0, segments: [] })
  assert.deepEqual(layoutPie(fundPieSlices([fund('zero', 0)])), { total: 0, segments: [] })
  const { total, segments } = layoutPie(fundPieSlices([fund('only', 1)]))
  assert.equal(total, 1)
  assert.equal(segments.length, 1)
  assert.equal((segments[0].path.match(/A 90 90/g) ?? []).length, 2)
  assert.match(segments[0].path, /M 100 10 A 90 90 0 0 1 100 190/)
  assert.doesNotMatch(segments[0].path, /L |M 100 100/)
  assert.doesNotMatch(segments[0].path, /NaN|Infinity/)
  const many = layoutPie(fundPieSlices(Array.from({ length: 150 }, (_, i) => fund(String(i), i + 1))))
  assert.equal(many.segments.length, 150)
  assert.ok(many.segments.every((s) => !/NaN|Infinity/.test(s.path)))
})

test('fund allocation uses current values and keeps zero-value funds selectable and colors stable', () => {
  const rows = [fund('reserve', 30000), fund('investment', 70000), fund('empty', 0)]
  const slices = fundPieSlices(rows)
  assert.equal(layoutPie(slices).total, 100000)
  assert.equal(slices[0].valueCents / layoutPie(slices).total, 0.3)
  assert.equal(slices[2].fundId, 'empty')
  const changed = fundPieSlices([...rows].reverse().map((f) => ({ ...f, name: 'renamed', currentValueCents: 500 })))
  for (const slice of slices) assert.equal(changed.find((s) => s.id === slice.id).color, slice.color)
})

test('fund refresh after a value update changes the pie without changing the selected fund', async () => {
  globalThis.window = { crypto: globalThis.crypto }
  setActivePinia(createPinia())
  let rows = [fund('a', 10000), fund('b', 30000)]
  mockIPC((command) => {
    if (command === 'plugin:finance|funds_overview') return rows
    rows = [fund('a', 30000), fund('b', 30000)]
    return true
  })
  const store = useFundsStore()
  const pie = computed(() => layoutPie(fundPieSlices(store.funds)))
  await store.load()
  store.selectedId = 'b'
  assert.equal(pie.value.total, 40000)
  await store.addEntry({ fundId: 'a', kind: 'valuation', amountCents: 30000, occurredOn: '2026-10-09', notes: '' })
  assert.equal(pie.value.total, 60000)
  assert.equal(pie.value.segments[0].valueCents / pie.value.total, 0.5)
  assert.equal(store.selectedId, 'b')
})
