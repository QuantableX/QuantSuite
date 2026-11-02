// QuantPilot parity 21: where a streaming message's unfinished tail starts.
// Run: node --test scripts/test-pilot-stream.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { streamBoundary } = await import('../modules/pilot/app/utils/stream.ts')

test('the tail starts after the last blank line outside a code fence', () => {
  const s = '# Title\n\nFirst paragraph.\n\nSecond, still stre'
  assert.equal(s.slice(streamBoundary(s)), 'Second, still stre')
  assert.equal(streamBoundary('no blank line yet'), 0)
  const fenced = 'Intro\n\n```ts\nconst a = 1\n\nconst b = 2\n'
  assert.equal(fenced.slice(streamBoundary(fenced)), '```ts\nconst a = 1\n\nconst b = 2\n', 'a blank line inside an open fence is not a boundary')
  const closed = 'Intro\n\n```ts\nconst a = 1\n```\n\nAfter the code'
  assert.equal(closed.slice(streamBoundary(closed)), 'After the code')
  const tilde = 'A\n\n~~~\nx\n\n```\nnot a close\n~~~\n\nB'
  assert.equal(tilde.slice(streamBoundary(tilde)), 'B', 'only the same fence closes')
  const trailing = 'Para one.\n\n'
  assert.equal(trailing.slice(streamBoundary(trailing)), '', 'a finished last block leaves no tail')
})
