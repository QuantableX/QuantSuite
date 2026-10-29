// QuantPilot V3: the thread sidebar's sections, order, drops, undo and snooze times (card 7).
// Run: node --test scripts/test-pilot-sidebar.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const {
  rowStatus,
  sectionOf,
  sections,
  visibleOrder,
  keyAt,
  dropAction,
  canDrop,
  arrangementFor,
  inverse,
  needsRespacing,
  respaced,
  pushUndo,
  latestUndo,
  dropExpired,
  UNDO_MS,
  inHours,
  tomorrowAt,
  parseDuration,
  wakeLabel,
} = await import('../modules/pilot/app/utils/sidebar.ts')

const NOW = new Date(2026, 9, 29, 14, 30).getTime()
const t = (id, extra = {}) => ({
  id,
  contextId: 'ws-a',
  pinned: false,
  sortKey: 0,
  settledAt: null,
  snoozedUntil: null,
  status: 'idle',
  sentAt: 0,
  ...extra,
})
const ids = (list) => list.map((x) => x.id)

test('saved states map to row statuses', () => {
  assert.equal(rowStatus('running'), 'working')
  assert.equal(rowStatus('starting'), 'working')
  assert.equal(rowStatus('waiting'), 'waiting')
  assert.equal(rowStatus('limited'), 'limited')
  assert.equal(rowStatus('interrupted'), 'idle')
  assert.equal(rowStatus(undefined), 'idle')
})

test('a thread lands in exactly one section', () => {
  assert.equal(sectionOf(t('a'), NOW, true), 'active')
  assert.equal(sectionOf(t('a', { pinned: true, status: 'working' }), NOW, true), 'pinned', 'pinned threads stay pinned while working')
  assert.equal(sectionOf(t('a', { status: 'working' }), NOW, true), 'working')
  assert.equal(sectionOf(t('a', { status: 'working' }), NOW, false), 'active', 'without the fold a working thread stays active')
  assert.equal(sectionOf(t('a', { settledAt: 5, pinned: true }), NOW, true), 'settled')
  assert.equal(sectionOf(t('a', { snoozedUntil: NOW + 1000, status: 'working' }), NOW, true), 'snoozed', 'snoozed work stays hidden')
  assert.equal(sectionOf(t('a', { snoozedUntil: NOW + 1000, status: 'waiting' }), NOW, true), 'active', 'it comes back when it needs you')
  assert.equal(sectionOf(t('a', { snoozedUntil: NOW - 1 }), NOW, true), 'active', 'past wake time = awake')
})

test('each section keeps its own order', () => {
  const s = sections(
    [
      t('p1', { pinned: true, sortKey: 1 }),
      t('p2', { pinned: true, sortKey: 9 }),
      t('a1', { sortKey: 3 }),
      t('a2', { sortKey: 7 }),
      t('w1', { status: 'working', sentAt: 10 }),
      t('w2', { status: 'working', sentAt: 30 }),
      t('s1', { settledAt: 100 }),
      t('s2', { settledAt: 200 }),
      t('z1', { snoozedUntil: NOW + 5000 }),
      t('z2', { snoozedUntil: NOW + 1000 }),
      t('other', { contextId: 'ws-b', sortKey: 99 }),
    ],
    { now: NOW, workingSection: true }
  )
  assert.deepEqual(ids(s.pinned), ['p2', 'p1'])
  assert.deepEqual(ids(s.active), ['other', 'a2', 'a1'])
  assert.deepEqual(ids(s.working), ['w2', 'w1'], 'last sent work first')
  assert.deepEqual(ids(s.settled), ['s2', 's1'], 'newest settlement first')
  assert.deepEqual(ids(s.snoozed), ['z2', 'z1'], 'soonest wake first')
  const filtered = sections([t('a'), t('b', { contextId: 'ws-b' })], { now: NOW, workingSection: true, contextId: 'ws-b' })
  assert.deepEqual(ids(filtered.active), ['b'], 'the project filter')
  assert.deepEqual(ids(visibleOrder(s, { working: false, settled: true, snoozed: false })), ['p2', 'p1', 'other', 'a2', 'a1', 's2', 's1'])
})

test('sort keys put a row at the drop spot', () => {
  const list = [t('a', { sortKey: 10 }), t('b', { sortKey: 6 }), t('c', { sortKey: 2 })]
  assert.equal(keyAt(list, 0, 0), 11)
  assert.equal(keyAt(list, 1, 0), 8)
  assert.equal(keyAt(list, 3, 0), 1)
  assert.equal(keyAt([], 0, 42), 42, 'an empty section keeps the old key')
  assert.equal(needsRespacing([t('a', { sortKey: 1 }), t('b', { sortKey: 1 - 1e-9 })]), true)
  assert.equal(needsRespacing(list), false)
  assert.deepEqual(respaced(list), [
    { id: 'a', sortKey: 10 },
    { id: 'b', sortKey: 9 },
    { id: 'c', sortKey: 8 },
  ])
})

test('the drag badge names what the drop does', () => {
  assert.equal(dropAction('active', { section: 'pinned', index: 0 }), 'Pin')
  assert.equal(dropAction('pinned', { section: 'active', index: 2 }), 'Unpin')
  assert.equal(dropAction('active', { section: 'settled' }), 'Settle')
  assert.equal(dropAction('pinned', { section: 'settled' }), 'Settle')
  assert.equal(dropAction('settled', { section: 'active', index: 0 }), 'Un-settle')
  assert.equal(dropAction('snoozed', { section: 'pinned', index: 0 }), 'Wake')
  assert.equal(dropAction('active', { section: 'active', index: 1 }), null, 'a reorder shows no badge')
  assert.equal(dropAction('pinned', { section: 'pinned', index: 1 }), null)
  assert.equal(canDrop('settled', { section: 'settled' }, true), false)
  assert.equal(canDrop('working', { section: 'active', index: 0 }, true), false, 'it would fold straight back')
  assert.equal(canDrop('working', { section: 'pinned', index: 0 }, true), true)
})

test('a drop becomes one arrangement, and undo restores the thread', () => {
  const target = [t('x', { sortKey: 10 }), t('y', { sortKey: 4 })]
  const row = t('r', { sortKey: 1, settledAt: 50 })
  const a = arrangementFor(row, 'settled', { section: 'active', index: 1 }, target)
  assert.deepEqual(a, { pinned: false, sortKey: 7, settled: false })
  assert.deepEqual(inverse(row, a), { pinned: false, sortKey: 1, settled: true })
  const snoozed = t('z', { sortKey: 3, snoozedUntil: NOW + 10, pinned: false })
  assert.deepEqual(arrangementFor(snoozed, 'snoozed', { section: 'pinned', index: 0 }, target), {
    pinned: true,
    sortKey: 11,
    snoozedUntil: null,
  })
  const pinned = t('p', { pinned: true, sortKey: 5 })
  const settle = arrangementFor(pinned, 'pinned', { section: 'settled' }, [])
  assert.deepEqual(settle, { settled: true })
  assert.deepEqual(inverse(pinned, settle), { pinned: true, sortKey: 5, settled: false }, 'undo brings back the pin at its spot')
  assert.deepEqual(inverse(snoozed, { snoozedUntil: NOW + 99 }), { snoozedUntil: NOW + 10 })
})

test('undo keeps five seconds and takes the newest first', () => {
  let list = []
  list = pushUndo(list, { id: 1, label: 'Settled', at: NOW })
  list = pushUndo(list, { id: 2, label: 'Unpinned', at: NOW + 1000 })
  assert.equal(latestUndo(list, NOW + 1500).id, 2)
  assert.equal(latestUndo(list, NOW + UNDO_MS + 500).id, 2, 'the older one expired, the newer did not')
  assert.equal(latestUndo(list, NOW + UNDO_MS + 1000), null)
  assert.deepEqual(
    dropExpired(list, NOW + UNDO_MS).map((e) => e.id),
    [2]
  )
  for (let i = 0; i < 20; i++) list = pushUndo(list, { id: 10 + i, label: 'x', at: NOW })
  assert.equal(list.length, 10, 'bounded')
})

test('snooze times', () => {
  assert.equal(inHours(NOW, 3) - NOW, 3 * 3_600_000)
  const tomorrow = new Date(tomorrowAt(NOW))
  assert.equal(tomorrow.getDate(), 30)
  assert.equal(tomorrow.getHours(), 9)
  assert.equal(tomorrow.getMinutes(), 0)
  assert.equal(new Date(tomorrowAt(new Date(2026, 9, 31, 23, 0).getTime())).getMonth(), 10, 'across a month end')
  assert.equal(parseDuration('90m'), 90 * 60_000)
  assert.equal(parseDuration('1.5 h'), 5_400_000)
  assert.equal(parseDuration('2 days'), 2 * 86_400_000)
  assert.equal(parseDuration('soon'), null)
  assert.equal(parseDuration('0h'), null)
  assert.equal(wakeLabel(new Date(2026, 9, 29, 17, 0).getTime(), NOW), '17:00')
  assert.equal(wakeLabel(tomorrowAt(NOW), NOW), 'Tomorrow 09:00')
  assert.match(wakeLabel(new Date(2026, 10, 2, 9, 0).getTime(), NOW), /^Mon, Nov 2 09:00$/)
})
