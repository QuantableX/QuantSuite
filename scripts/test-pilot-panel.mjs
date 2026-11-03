// QuantPilot round 2, card 04: the thread panel's sections, Agents now, Changes.
// Run: node --test scripts/test-pilot-panel.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { PANEL_SECTIONS, normalizeLayout, moveSection, dropIndex, agentsNow, aggregateChanges, tokenPair } = await import('../modules/pilot/app/utils/panel.ts')

test('a saved layout is made whole', () => {
  assert.deepEqual(normalizeLayout(null).order, [...PANEL_SECTIONS])
  const l = normalizeLayout({ order: ['changes', 'nope', 'agents', 'changes'], hidden: ['tasks', 'nope'] })
  assert.deepEqual(l.order.slice(0, 2), ['changes', 'agents'])
  assert.equal(l.order.length, PANEL_SECTIONS.length, 'new sections join at the end, duplicates once')
  assert.deepEqual(l.hidden, ['tasks'])
})

test('a section moves by drag to the slot the pointer passed', () => {
  const order = ['agents', 'thread', 'changes', 'branch']
  assert.deepEqual(moveSection(order, 'branch', 0), ['branch', 'agents', 'thread', 'changes'])
  assert.deepEqual(moveSection(order, 'agents', 9), ['thread', 'changes', 'branch', 'agents'])
  assert.equal(dropIndex([10, 50, 90], 5), 0)
  assert.equal(dropIndex([10, 50, 90], 60), 2)
  assert.equal(dropIndex([10, 50, 90], 200), 3)
})

test('Agents now: who needs you, who works, then unseen results', () => {
  const t = (id, status, since, unread = false) => ({ id, title: id, status, since, unread })
  const rows = agentsNow([t('idle', 'idle', 1), t('w2', 'working', 20), t('need', 'waiting', 30), t('w1', 'working', 10), t('done', 'idle', 40, true), t('old', 'idle', 5, true), t('err', 'error', 50, true)])
  assert.deepEqual(rows.map((r) => r.id), ['need', 'w1', 'w2', 'err', 'done', 'old'])
  assert.deepEqual(rows.map((r) => r.bucket), ['waiting', 'working', 'working', 'done', 'done', 'done'])
  assert.equal(agentsNow(Array.from({ length: 20 }, (_, i) => t(`x${i}`, 'working', i)), 5).length, 5)
})

test('Changes: every file once, lines summed, newest first, its newest turn', () => {
  const items = [
    { kind: 'changes', turnId: 't1', data: { files: [{ path: 'a.ts', added: 2, removed: 1, status: 'M' }, { path: 'b.ts', added: 5, removed: 0, status: 'A' }] } },
    { kind: 'user', turnId: 't2', data: {} },
    { kind: 'changes', turnId: 't2', data: { files: [{ path: 'a.ts', added: 1, removed: 3, status: 'M' }] } },
  ]
  const c = aggregateChanges(items)
  assert.deepEqual(c.map((f) => f.path), ['a.ts', 'b.ts'])
  assert.deepEqual([c[0].added, c[0].removed, c[0].turnId], [3, 4, 't2'])
  assert.equal(tokenPair(83_400, 200_000), '83k / 200k')
  assert.equal(tokenPair(512_000, 1_000_000), '512k / 1M')
})
