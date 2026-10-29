// QuantPilot V3: how a thread's items read as a conversation (card 3).
// Run: node --test scripts/test-pilot-timeline.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { groupTurns, summarize, formatDuration, GROUP_MAX } = await import('../modules/pilot/app/utils/timeline.ts')

let seq = 0
function item(kind, data = {}, extra = {}) {
  seq += 1
  return { id: `i${seq}`, threadId: 't', turnId: extra.turnId ?? 'u1', seq, kind, parent: extra.parent ?? null, data, createdAt: seq * 1000, updatedAt: seq * 1000 }
}
const tool = (category, status = 'completed', extra) => item('tool', { category, status, title: category }, extra)

test('the final reply stays outside the fold, work goes in', () => {
  seq = 0
  const items = [
    item('user', { text: 'fix it' }),
    item('assistant', { text: 'Let me look.' }),
    tool('read'),
    tool('command'),
    item('reasoning', { text: '' }),
    item('assistant', { text: 'Done: fixed.' }),
  ]
  const [b] = groupTurns(items)
  assert.equal(b.user.data.text, 'fix it')
  assert.equal(b.answer.data.text, 'Done: fixed.')
  const kinds = b.activity.map((e) => (e.kind === 'tools' ? `tools:${e.items.length}` : e.item.kind))
  assert.deepEqual(kinds, ['assistant', 'tools:2', 'reasoning'])
  assert.equal(b.summary, 'Ran 1 command, read 1 file')
})

test('narration followed by a tool moves into the fold', () => {
  seq = 0
  const items = [item('user', { text: 'go' }), item('assistant', { text: 'Starting.' }), tool('command', 'running')]
  const [b] = groupTurns(items)
  assert.equal(b.answer, null, 'no answer while a tool runs after the narration')
  assert.equal(b.toolCount, 1)
})

test('runs of tools split into groups of at most five', () => {
  seq = 0
  const tools = Array.from({ length: 12 }, () => tool('edit'))
  const [b] = groupTurns([item('user', { text: 'x' }), ...tools, item('assistant', { text: 'ok' })])
  const sizes = b.activity.filter((e) => e.kind === 'tools').map((e) => e.items.length)
  assert.deepEqual(sizes, [GROUP_MAX, GROUP_MAX, 2])
  assert.equal(b.summary, 'Edited 12 files')
})

test('pending requests, plans and errors stay outside; answered requests fold', () => {
  seq = 0
  const items = [
    item('user', { text: 'x' }),
    item('approval', { status: 'allowed', title: 'ls' }),
    tool('command'),
    item('approval', { status: 'pending', title: 'rm' }),
    item('plan', { status: 'approved', plan: '1. a' }),
    item('error', { text: 'boom' }),
  ]
  const [b] = groupTurns(items)
  assert.deepEqual(b.requests.map((r) => r.data.title), ['rm'])
  assert.deepEqual(b.outside.map((o) => o.kind), ['plan', 'error'])
  assert.ok(b.activity.some((e) => e.kind === 'item' && e.item.kind === 'approval'))
})

test('a plan before the answer reads before it', () => {
  seq = 0
  const items = [
    item('user', { text: 'plan first' }),
    item('plan', { status: 'approved', plan: '1. a' }),
    item('assistant', { text: 'Implemented.' }),
    item('notice', { text: 'Conversation compacted' }),
  ]
  const [b] = groupTurns(items)
  assert.deepEqual(b.before.map((o) => o.kind), ['plan'])
  assert.equal(b.answer.data.text, 'Implemented.')
  assert.deepEqual(b.outside.map((o) => o.kind), ['notice'])
})

test('a message steered into the turn stays visible in order', () => {
  seq = 0
  const items = [
    item('user', { text: 'run five echoes' }),
    tool('command'),
    item('user', { text: 'also BANANA', steer: true }),
    tool('command'),
    item('assistant', { text: 'Done, BANANA too.' }),
  ]
  const [b] = groupTurns(items)
  assert.equal(b.user.data.text, 'run five echoes')
  assert.deepEqual(b.before.map((o) => o.data.text), ['also BANANA'])
  assert.equal(b.answer.data.text, 'Done, BANANA too.')
  assert.equal(b.toolCount, 2)
})

test('turns keep their order and subagent text never becomes the answer', () => {
  seq = 0
  const items = [
    item('user', { text: 'a' }, { turnId: 'u1' }),
    item('assistant', { text: 'first' }, { turnId: 'u1' }),
    item('user', { text: 'b' }, { turnId: 'u2' }),
    tool('agent', 'completed', { turnId: 'u2' }),
    item('assistant', { text: 'child says' }, { turnId: 'u2', parent: 'task1' }),
  ]
  const blocks = groupTurns(items)
  assert.deepEqual(blocks.map((b) => b.turnId), ['u1', 'u2'])
  assert.equal(blocks[0].answer.data.text, 'first')
  assert.equal(blocks[1].answer, null)
})

test('summaries name two categories and count the rest', () => {
  seq = 0
  const s = summarize([tool('read'), tool('command'), tool('command'), tool('edit'), tool('web'), tool('search')])
  assert.equal(s, 'Ran 2 commands, edited 1 file and 3 more')
  assert.equal(summarize([]), '')
})

test('durations read like the references', () => {
  assert.equal(formatDuration(12_400), '12s')
  assert.equal(formatDuration(72_000), '1m 12s')
  assert.equal(formatDuration(3_780_000), '1h 3m')
})
