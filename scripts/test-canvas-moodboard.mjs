// QuantPilot round 3, card 07: moodboards — moving an image by drag, which dropped files count.
// Run: node --test scripts/test-canvas-moodboard.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { moveItem, gapAt, imageFiles } = await import('../modules/canvas/app/utils/moodboard.ts')

test('an image moves to the gap it was dropped in', () => {
  const l = ['a', 'b', 'c', 'd']
  assert.deepEqual(moveItem(l, 0, 3), ['b', 'c', 'a', 'd'], 'into the gap before d')
  assert.deepEqual(moveItem(l, 0, 4), ['b', 'c', 'd', 'a'], 'to the end')
  assert.deepEqual(moveItem(l, 3, 0), ['d', 'a', 'b', 'c'], 'to the front')
  assert.deepEqual(moveItem(l, 1, 1), l, 'its own gap changes nothing')
  assert.deepEqual(moveItem(l, 1, 2), l, 'the gap right after it changes nothing')
  assert.deepEqual(moveItem(l, 9, 0), l)
  assert.deepEqual(l, ['a', 'b', 'c', 'd'], 'the list itself is left alone')
})

test('the left half of a card means before it, the right half after it', () => {
  assert.equal(gapAt(2, { left: 100, width: 80 }, 120), 2)
  assert.equal(gapAt(2, { left: 100, width: 80 }, 170), 3)
})

test('only PNG, JPEG, GIF and WebP files go on a board', () => {
  const files = [{ type: 'image/png', name: 'a.png' }, { type: 'text/plain', name: 'b.txt' }, { type: 'image/webp', name: 'c.webp' }, { type: 'image/svg+xml', name: 'd.svg' }]
  assert.deepEqual(imageFiles(files), [0, 2])
})
