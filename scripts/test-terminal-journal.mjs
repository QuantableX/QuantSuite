import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { removeLegacyJournalSamples } from '../modules/terminal/app/utils/journal-migration.ts'
import { useJournalStore } from '../modules/terminal/app/stores/journal.ts'

const fixture = JSON.parse(readFileSync(new URL('./fixtures/terminal-journal-legacy.json', import.meta.url), 'utf8'))
const clone = value => JSON.parse(JSON.stringify(value))
const samples = () => clone(fixture.trades)
const realTrade = () => ({ ...clone(fixture.trades[0]), id: 'manual-trade', notes: 'My actual trade', createdAt: '2026-10-11T12:00:00.000Z', updatedAt: '2026-10-11T12:00:00.000Z' })
const key = 'quantview-journal'

function setup(raw) {
  const storage = new Map([['unrelated-setting', 'preserve me']])
  if (raw !== null) storage.set(key, raw)
  const writes = []
  const failures = { read: false, write: false }
  globalThis.localStorage = {
    getItem(name) {
      if (failures.read) throw new Error('Read failed')
      return storage.get(name) ?? null
    },
    setItem(name, value) {
      if (failures.write) throw new Error('Write failed')
      writes.push({ name, value })
      storage.set(name, value)
    },
  }
  setActivePinia(createPinia())
  return { store: useJournalStore(), storage, writes, failures }
}

test('removes all 20 frozen historical examples without depending on random IDs', () => {
  const old = samples()
  assert.equal(old.length, 20)
  assert.deepEqual(removeLegacyJournalSamples(old), [])
  for (const random of ['', 'a', 'zz9999']) {
    const anotherInstallation = old.map((trade, i) => ({ ...trade, id: `mzzzzzzz${random}${i}` }))
    assert.deepEqual(removeLegacyJournalSamples(anotherInstallation), [])
  }
  assert.deepEqual(old, fixture.trades, 'Migration must not mutate its input')
})

test('recognizes actual old DST variants independently of the current time zone', () => {
  for (const { timezone, plusOneHourIndices } of fixture.timezoneVariants) {
    const old = samples()
    for (const i of plusOneHourIndices) {
      old[i].createdAt = old[i].createdAt.replace('T00:', 'T01:')
      old[i].updatedAt = old[i].createdAt
    }
    assert.deepEqual(removeLegacyJournalSamples(old), [], timezone)
  }
})

test('mixed, partial, reordered and duplicate sample sets preserve all genuine trades', () => {
  const old = samples()
  const real = realTrade()
  const edited = { ...old[3], notes: 'Personal notes added to this row' }
  const mixed = [old[18], real, old[5], old[5], edited, old[0]]
  assert.deepEqual(removeLegacyJournalSamples(mixed), [real, edited])
})

test('changing or removing any historical field preserves the record', () => {
  for (const sample of samples()) {
    for (const [field, value] of Object.entries(sample)) {
      const changed = clone(sample)
      changed[field] = Array.isArray(value) ? [...value, 'user-added']
        : typeof value === 'number' ? value + 1 : value === null ? 123 : `changed-${value}`
      assert.deepEqual(removeLegacyJournalSamples([changed]), [changed], field)
      const missing = clone(sample)
      delete missing[field]
      assert.deepEqual(removeLegacyJournalSamples([missing]), [missing], `missing ${field}`)
    }
  }
})

test('new metadata, different IDs and malformed records are never deleted', () => {
  const records = [
    { ...samples()[0], extraUserData: 'keep' },
    { ...samples()[0], id: 'imported-record' },
    { ...samples()[0], id: 'mrck0000abc1239' },
    { ...samples()[0], updatedAt: '2026-03-01T01:00:00.000Z' },
    null, false, 10, 'unknown', [], {},
  ]
  assert.equal(removeLegacyJournalSamples(records), records)
})

test('field ordering from JSON exports does not prevent cleanup', () => {
  const reordered = samples().map(trade => Object.fromEntries(Object.entries(trade).reverse()))
  assert.deepEqual(removeLegacyJournalSamples(reordered), [])
})

test('loading an existing profile persists an empty journal once and does not reseed', () => {
  const { store, storage, writes } = setup(JSON.stringify(samples()))
  store.loadFromStorage()
  assert.equal(store.trades.length, 0)
  assert.equal(storage.get(key), '[]')
  assert.deepEqual(writes, [{ name: key, value: '[]' }])
  store.loadFromStorage()
  assert.equal(store.trades.length, 0)
  assert.equal(writes.length, 1)
  assert.equal(storage.get('unrelated-setting'), 'preserve me')
})

test('mixed-profile cleanup preserves saved real data and survives a new store instance', () => {
  const real = realTrade()
  const { store, storage } = setup(JSON.stringify([samples()[0], real, samples()[19]]))
  store.loadFromStorage()
  assert.deepEqual(JSON.parse(storage.get(key)), [real])
  setActivePinia(createPinia())
  const reopened = useJournalStore()
  reopened.loadFromStorage()
  assert.deepEqual(clone(reopened.trades), [real])
})

test('no-op loads preserve the exact stored bytes and never write to other storage', () => {
  for (const raw of [null, '[]', JSON.stringify([realTrade()], null, 2), '{bad', '{}', 'null']) {
    const { store, storage, writes } = setup(raw)
    store.loadFromStorage()
    assert.equal(storage.get(key) ?? null, raw)
    assert.deepEqual(writes, [])
    assert.equal(storage.get('unrelated-setting'), 'preserve me')
  }
})

test('failed cleanup writes retain real trades and retry on a later load', t => {
  const real = realTrade()
  const raw = JSON.stringify([samples()[0], real])
  const { store, storage, failures } = setup(raw)
  const warning = t.mock.method(console, 'warn', () => {})
  failures.write = true
  store.loadFromStorage()
  assert.deepEqual(clone(store.trades), [real])
  assert.equal(storage.get(key), raw)
  assert.equal(warning.mock.calls.length, 1)
  failures.write = false
  store.loadFromStorage()
  assert.deepEqual(JSON.parse(storage.get(key)), [real])
})

test('unavailable storage stays untouched', () => {
  const raw = JSON.stringify(samples())
  const { store, storage, failures, writes } = setup(raw)
  failures.read = true
  store.loadFromStorage()
  assert.equal(store.trades.length, 0)
  assert.equal(storage.get(key), raw)
  assert.equal(writes.length, 0)
})

test('manual logging, editing and deletion still work after migration', () => {
  const { store, storage } = setup(JSON.stringify(samples()))
  store.loadFromStorage()
  const { id, pnl, pnlPercent, createdAt, updatedAt, ...input } = realTrade()
  const added = store.addTrade(input)
  store.updateTrade(added.id, { notes: 'Saved after cleanup' })
  store.loadFromStorage()
  assert.equal(store.trades.length, 1)
  assert.equal(store.trades[0].notes, 'Saved after cleanup')
  store.deleteTrade(added.id)
  store.loadFromStorage()
  assert.equal(storage.get(key), '[]')
  assert.equal(store.trades.length, 0)
})
