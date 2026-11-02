// QuantPilot parity 15: Find compares text like the backend (history.rs find_*).
// The cases mirror the Rust tests so both sides count the same matches.
// Run: node --test scripts/test-pilot-find.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { normalize, needleOf, matchesIn, locateMatch, matchIndex } = await import('../modules/pilot/app/utils/find.ts')

const FSI = String.fromCodePoint(0x2068)
const PDI = String.fromCodePoint(0x2069)
const DOT = String.fromCodePoint(0x307) // combining dot above, as in the lowercase of İ

test('readable text: case folded, emphasis and chip marks dropped, spaces collapsed', () => {
  assert.equal(normalize('A  **b**\n\n_c_ `d` ~~e~~').norm, 'a b c d e')
  assert.equal(normalize(`${FSI}x.md${PDI} Ü`).norm, 'x.md ü')
  assert.equal(needleOf('  Hi  '), 'hi')
  assert.equal(needleOf('**'), '')
})

test('the same counts as the backend', () => {
  const count = (text, q) => matchesIn(text, needleOf(q)).length
  assert.equal(count('The OLDEST needle lives here.', 'needle'), 1)
  assert.equal(count(`Look at ${FSI}main.rs${PDI} for the needle please`, 'main.rs for'), 1)
  assert.equal(count('Subagent: **Needle** found, needle again.', 'needle'), 2)
  assert.equal(count('Subagent: **Needle** found, needle again.', 'subagent: needle found'), 1)
  assert.equal(count('Größe ÄRGER İstanbul  straße', 'größe ärger'), 1)
  assert.equal(count('Größe ÄRGER İstanbul  straße', 'GRÖSSE'), 0)
  assert.equal(count('Größe ÄRGER İstanbul  straße', `i${DOT}stanbul straße`), 1)
  assert.equal(count('aaaa', 'aa'), 2, 'non-overlapping')
  assert.equal(count('nothing', 'missing'), 0)
})

test('matches map back to the source text, surrogate pairs and folding included', () => {
  const text = 'Say **Hello** 👋 world, hello'
  const m = matchesIn(text, needleOf('hello'))
  assert.deepEqual(m.map(([s, e]) => text.slice(s, e)), ['Hello', 'hello'])
  const emoji = matchesIn(text, needleOf('👋 world'))
  assert.equal(text.slice(...emoji[0]), '👋 world')
  const t2 = 'x İstanbul'
  const [[s, e]] = matchesIn(t2, needleOf(`i${DOT}stanbul`))
  assert.equal(t2.slice(s, e), 'İstanbul')
})

test('a global match index walks the hits and back', () => {
  const hits = [{ count: 2 }, { count: 0 }, { count: 3 }]
  assert.deepEqual(locateMatch(hits, 0), { hit: 0, n: 0 })
  assert.deepEqual(locateMatch(hits, 2), { hit: 2, n: 0 })
  assert.deepEqual(locateMatch(hits, 4), { hit: 2, n: 2 })
  assert.equal(locateMatch(hits, 5), null)
  assert.equal(matchIndex(hits, 2, 1), 3)
})
