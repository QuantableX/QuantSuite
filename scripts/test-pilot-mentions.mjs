// QuantPilot V3: the composer's @ / $ triggers, inline chips and commands (card 9).
// Run: node --test scripts/test-pilot-mentions.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const m = await import('../modules/pilot/app/utils/mentions.ts')
const { CHIP_OPEN: O, CHIP_CLOSE: C } = m

test('triggers start a word; / only a line', () => {
  assert.deepEqual(m.triggerAt('look at @src/ma', 15), { kind: '@', query: 'src/ma', start: 8 })
  assert.deepEqual(m.triggerAt('@', 1), { kind: '@', query: '', start: 0 })
  assert.equal(m.triggerAt('mail@host', 9), null, 'not inside a word')
  assert.deepEqual(m.triggerAt('use $pd', 7), { kind: '$', query: 'pd', start: 4 })
  assert.deepEqual(m.triggerAt('/comp', 5), { kind: '/', query: 'comp', start: 0 })
  assert.deepEqual(m.triggerAt('first\n/re', 9), { kind: '/', query: 're', start: 6 })
  assert.equal(m.triggerAt('a /b', 4), null, '/ mid-line is text')
  assert.deepEqual(m.triggerAt('see @"docs/my no', 16), { kind: '@', query: 'docs/my no', start: 4 })
  assert.equal(m.triggerAt('done @a.rs then', 15), null, 'the caret left the word')
})

test('a pick replaces the trigger and adds a space', () => {
  const t = m.triggerAt('open @src/ma now', 12)
  assert.deepEqual(m.replaceTrigger('open @src/ma now', t, 12, '@src/main.rs'), { text: 'open @src/main.rs now', caret: 17 })
  const t2 = m.triggerAt('/co', 3)
  assert.deepEqual(m.replaceTrigger('/co', t2, 3, '/compact'), { text: '/compact ', caret: 9 })
})

test('chips delete whole and keep the caret outside', () => {
  const text = `fix ${m.chipToken('Terminal 1')} now`
  const end = text.indexOf(C) + 1
  assert.deepEqual(m.chipNextTo(text, end, -1), [4, end], 'Backspace after a chip')
  assert.deepEqual(m.chipNextTo(text, 4, 1), [4, end], 'Delete before a chip')
  assert.equal(m.chipNextTo(text, 2, -1), null)
  assert.equal(m.outsideChip(text, 7), end, 'a caret inside moves past the chip')
  assert.equal(m.outsideChip(text, 7, -1), 4)
  assert.equal(m.outsideChip(text, 2), 2)
  assert.deepEqual(m.chipsIn(`${m.chipToken('a')} x ${m.chipToken('b')}`), ['a', 'b'])
  assert.equal(m.uniqueLabel('a', m.chipToken('a')), 'a (2)')
  assert.equal(m.plainText(text), 'fix [Terminal 1] now')
  assert.deepEqual(m.segments(`a${O}x${C}b`), [{ chip: false, text: 'a' }, { chip: true, text: 'x' }, { chip: false, text: 'b' }])
})

test('commands: app ones first, prefixes before substrings', () => {
  const rows = m.commandRows([{ name: 'compact', description: 'Clear history but keep a summary' }, { name: 'review', description: 'Review a PR' }], 'co')
  assert.deepEqual(rows.map((r) => r.name), ['copy', 'compact'])
  assert.equal(rows[0].name, 'copy', 'an app command with the prefix first')
  assert.ok(rows.some((r) => r.name === 'compact' && r.source === 'provider'))
  assert.equal(m.commandRows([], '').length, m.APP_COMMANDS.length)
  assert.deepEqual(m.appCommand('/rename  Fix the login '), { name: 'rename', arg: 'Fix the login' })
  assert.equal(m.appCommand('/compact'), null, 'a provider command is sent')
  assert.equal(m.appCommand('please /plan'), null)
})
