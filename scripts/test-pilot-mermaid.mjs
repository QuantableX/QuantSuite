// QuantPilot parity 13: Mermaid cache keys, size limits and placeholders.
// Run: node --test scripts/test-pilot-mermaid.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { hashOf, tooLarge, fillDiagrams, SOURCE_MAX, LINES_MAX } = await import('../modules/pilot/app/utils/mermaid.ts')

test('a diagram is cached by source and theme', () => {
  const a = hashOf('graph TD; A-->B', 'dark')
  assert.equal(a, hashOf('graph TD; A-->B', 'dark'))
  assert.notEqual(a, hashOf('graph TD; A-->B', 'light'), 'a theme change draws again')
  assert.notEqual(a, hashOf('graph TD; A-->C', 'dark'))
  assert.match(a, /^[0-9a-f]+$/)
})

test('huge diagrams stay source with a reason', () => {
  assert.equal(tooLarge('graph TD; A-->B'), null)
  assert.match(tooLarge('x'.repeat(SOURCE_MAX + 1)), /characters/)
  assert.match(tooLarge('a\n'.repeat(LINES_MAX + 1)), /lines/)
})

test('placeholders without a drawn diagram leave nothing behind', () => {
  assert.equal(fillDiagrams('<div>%%QPMM:deadbeef12%%</div>'), '<div></div>')
  assert.equal(fillDiagrams('<p>plain</p>'), '<p>plain</p>')
})
