// QuantPilot parity 14: quote sources and finding the quoted text again.
// Run: node --test scripts/test-pilot-quote.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { parseQuotePath, findQuote } = await import('../modules/pilot/app/utils/quote.ts')

test('a quote path names thread, item and occurrence', () => {
  assert.deepEqual(parseQuotePath('pilot-item:th-1:it-2:3'), { threadId: 'th-1', itemId: 'it-2', occurrence: 3 })
  assert.equal(parseQuotePath('C:/file.txt'), null)
  assert.equal(parseQuotePath(null), null)
})

test('the right occurrence is found, whatever the line breaks', () => {
  const text = 'Use a lock.\nThen use a lock again.\n\nUse a lock'
  assert.equal(findQuote(text, 'use a lock', 3), null, 'case matters: only one lowercase phrase exists')
  const hits = [0, 1, 2].map((n) => findQuote(text, 'Use a lock', n))
  assert.deepEqual(hits[0], [0, 10])
  assert.deepEqual(hits[1], [text.lastIndexOf('Use a lock'), text.length])
  const wrapped = findQuote('alpha beta\n  gamma delta', 'beta gamma')
  assert.deepEqual(wrapped, [6, 18], 'a selection across a line break still matches')
  assert.deepEqual(findQuote('Grüße — ünïcode ✓ end', 'ünïcode ✓'), [8, 17])
  assert.equal(findQuote('nothing here', 'missing'), null)
})
