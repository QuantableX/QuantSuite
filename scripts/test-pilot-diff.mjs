// QuantPilot V3 card 24: the Diff tab's all-files view — patches as lines, find across files.
// Run: node --test scripts/test-pilot-diff.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const D = await import('../modules/pilot/app/utils/diff.ts')

const PATCH = [
  'diff --git a/src/a.rs b/src/a.rs',
  'index 111..222 100644',
  '--- a/src/a.rs',
  '+++ b/src/a.rs',
  '@@ -10,3 +10,4 @@ fn main() {',
  ' let x = 1;',
  '-let y = tab;',
  '+let y = Tab;',
  '+let z = tab;',
  ' }',
  '\\ No newline at end of file',
].join('\n')

test('a patch reads as numbered lines without its file header', () => {
  const lines = D.parsePatch(PATCH)
  assert.deepEqual(lines.map((l) => l.kind), ['hunk', 'ctx', 'del', 'add', 'add', 'ctx', 'meta'])
  assert.deepEqual(lines.map((l) => [l.old, l.new]), [[null, null], [10, 10], [11, null], [null, 11], [null, 12], [12, 13], [null, null]])
  assert.equal(lines[2].text, 'let y = tab;')
  assert.deepEqual(D.parsePatch('Binary files a/x.png and b/x.png differ').map((l) => l.kind), ['meta'])
  assert.deepEqual(D.parsePatch(''), [])
})

test('find runs across every file, case-insensitive, never in hunk headers', () => {
  const a = D.parsePatch(PATCH)
  const b = D.parsePatch('@@ -1 +1 @@ tab\n-tab tab\n+ok')
  const m = D.findMatches([a, b], 'TAB')
  assert.deepEqual(m.map((x) => [x.file, x.line, x.start]), [[0, 2, 8], [0, 3, 8], [0, 4, 8], [1, 1, 0], [1, 1, 4]])
  assert.equal(D.findMatches([a], 'Tab', true).length, 1)
  assert.deepEqual(D.findMatches([a], ''), [])
})

test('marks split a line; steps wrap around', () => {
  assert.deepEqual(D.markSegments('let tab tab', [{ start: 8, end: 11, current: true }, { start: 4, end: 7, current: false }]), [
    { text: 'let ', mark: false, current: false },
    { text: 'tab', mark: true, current: false },
    { text: ' ', mark: false, current: false },
    { text: 'tab', mark: true, current: true },
  ])
  assert.deepEqual(D.markSegments('', []), [{ text: '', mark: false, current: false }])
  assert.equal(D.stepMatch(3, 2, 1), 0)
  assert.equal(D.stepMatch(3, 0, -1), 2)
  assert.equal(D.stepMatch(0, 0, 1), -1)
})
