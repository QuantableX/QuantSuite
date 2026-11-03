// QuantPilot round 2, card 10: side chats — the bounded main-thread context and titles.
// Run: node --test scripts/test-pilot-side.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { mainContext, sideTitle, SIDE_CONTEXT_MAX } = await import('../modules/pilot/app/utils/side.ts')
const item = (seq, kind, text, streaming = false) => ({ seq, kind, data: { text, streaming } })

test('a side message carries what the main thread said since the last question', () => {
  const items = [item(1, 'user', 'Refactor the parser'), item(2, 'tool', 'ls'), item(3, 'assistant', 'Working on it'), item(4, 'assistant', 'Halfway there', true)]
  const first = mainContext({ title: 'Parser', state: 'running' }, items, -1)
  assert.match(first.text, /^Main thread "Parser" — running\. What was said there since your last question:/)
  assert.match(first.text, /## User\nRefactor the parser/)
  assert.match(first.text, /## Agent \(still writing\)\nHalfway there/)
  assert.ok(!first.text.includes('ls'), 'tool calls stay out')
  assert.equal(first.lastSeq, 4)
  const again = mainContext({ title: 'Parser', state: 'running' }, items, first.lastSeq)
  assert.match(again.text, /Nothing new there since your last question\./)
  assert.equal(again.included, 0)
})

test('the block stays under about 24 KB and says what it left out', () => {
  const items = Array.from({ length: 40 }, (_, i) => item(i + 1, i % 2 ? 'assistant' : 'user', `${i} `.repeat(800)))
  const c = mainContext({ title: 'Big', state: 'idle' }, items, -1)
  assert.ok(c.text.length <= SIDE_CONTEXT_MAX + 200, String(c.text.length))
  assert.ok(c.omitted > 0)
  assert.match(c.text, /\(\d+ earlier messages? left out\.\)/)
  assert.ok(c.text.includes('39 '), 'the newest message is kept')
  const one = mainContext({ title: 'Huge', state: 'idle' }, [item(1, 'assistant', 'x'.repeat(60_000))], -1)
  assert.ok(one.text.length <= SIDE_CONTEXT_MAX + 200 && one.text.includes('…'), 'one long answer is cut from its start')
})

test('a side chat is named after its first question', () => {
  assert.equal(sideTitle('  what   about the tests? '), 'Side: what about the tests?')
  assert.equal(sideTitle(''), 'Side chat')
  assert.ok(sideTitle('a'.repeat(80)).endsWith('…'))
})
