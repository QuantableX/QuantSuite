// QuantPilot round 3, card 05: visual progress — references first, turns in order, before/after picks.
// Run: node --test scripts/test-pilot-progress.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { arrange, beforeAfter, togglePick, imageLabel } = await import('../modules/pilot/app/utils/progress.ts')
const im = (key, turn, source = 'you') => ({ key, itemId: key.split('#')[0], turn, source, name: key, path: null, gone: false, at: 0 })
const images = [im('a#0', 1), im('a#1', 1, 'browser'), im('p#0', 2, 'page'), im('t#0', 4, 'tool'), im('u#0', 0)]

test('references come first, the rest turn by turn', () => {
  const { pinned, turns } = arrange(images, ['t#0', 'gone#0', 'a#0'])
  assert.deepEqual(pinned.map((x) => x.key), ['t#0', 'a#0'], 'pin order, unknown keys skipped')
  assert.deepEqual(turns.map((t) => [t.turn, t.images.map((x) => x.key)]), [[1, ['a#0', 'a#1']], [2, ['p#0']], [4, ['t#0']], [0, ['u#0']]])
  assert.equal(imageLabel(images[1]), 'Turn 1 · browser')
  assert.equal(imageLabel(images[4]), 'Earlier · you')
})

test('two picks are before (earlier) and after; a third drops the oldest pick', () => {
  assert.equal(beforeAfter(images, ['a#0']), null)
  const pair = beforeAfter(images, ['t#0', 'a#1'])
  assert.equal(pair.before.key, 'a#1')
  assert.equal(pair.after.key, 't#0')
  assert.deepEqual(togglePick([], 'a#0'), ['a#0'])
  assert.deepEqual(togglePick(['a#0', 'p#0'], 't#0'), ['p#0', 't#0'])
  assert.deepEqual(togglePick(['a#0', 'p#0'], 'a#0'), ['p#0'])
})
