// QuantPilot V3: the composer's pickers (card 5).
// Run: node --test scripts/test-pilot-models.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { modelRows, effortsFor, storedModelId, MODES, compactTokens, modesFor } = await import('../modules/pilot/app/utils/models.ts')

// The shape Claude Code 2.1.312's initialize reports (fixture: claude_tool_turn.jsonl).
const models = [
  { id: 'default', displayName: 'Default (recommended)', description: 'Opus 5.5 · Best for everyday, complex tasks', resolvedModel: 'claude-opus-5-5', efforts: ['low', 'medium', 'high', 'xhigh', 'max'] },
  { id: 'opus', displayName: 'Opus 5.5', description: 'For complex work and everyday tasks', resolvedModel: 'claude-opus-5-5', efforts: ['low', 'medium', 'high', 'xhigh', 'max'] },
  { id: 'sonnet', displayName: 'Sonnet 5.5', description: 'Most efficient for simpler tasks', resolvedModel: 'claude-sonnet-5-5', efforts: ['low', 'medium', 'high'] },
  { id: 'haiku', displayName: 'Haiku 5.5', description: 'Fastest', resolvedModel: 'claude-haiku-5-5', efforts: [] },
]

test('the catalog default is stored as no override; ids stay apart from names', () => {
  const rows = modelRows(models, '')
  assert.equal(rows[0].id, '')
  assert.equal(rows[0].label, 'Default (recommended)')
  assert.deepEqual(rows.map((r) => r.id), ['', 'opus', 'sonnet', 'haiku'])
  assert.ok(rows[1].detail.includes('claude-opus-5-5'))
  assert.equal(storedModelId('default'), '')
})

test('search ranks exact and prefix matches first and offers a custom id', () => {
  const rows = modelRows(models, 'son')
  assert.equal(rows[0].id, 'sonnet')
  const custom = modelRows(models, 'claude-opus-5-5-20261001')
  assert.equal(custom.at(-1).custom, true)
  assert.equal(custom.at(-1).id, 'claude-opus-5-5-20261001')
  assert.equal(modelRows(models, 'opus').some((r) => r.custom), false, 'a listed id is not offered as custom')
  assert.equal(modelRows(models, 'two words').some((r) => r.custom), false, 'not an id')
})

test('effort levels come from the model, and a model without them has none', () => {
  assert.deepEqual(effortsFor(models, ''), ['low', 'medium', 'high', 'xhigh', 'max'])
  assert.deepEqual(effortsFor(models, 'sonnet'), ['low', 'medium', 'high'])
  assert.deepEqual(effortsFor(models, 'haiku'), [])
  assert.deepEqual(effortsFor(models, 'claude-sonnet-5-5'), ['low', 'medium', 'high'], 'a resolved id finds its entry')
  assert.deepEqual(effortsFor(models, 'something-else'), [])
})

test('five modes in the reference order, and compact token labels', () => {
  assert.deepEqual(MODES.map((m) => m.id), ['ask', 'edits', 'auto', 'plan', 'full'])
  assert.equal(compactTokens(200_000), '200k')
  assert.equal(compactTokens(1_000_000), '1M')
})

test('agents without an automatic reviewer offer no Auto (card 20)', () => {
  assert.deepEqual(modesFor('claude').map((m) => m.id), ['ask', 'edits', 'auto', 'plan', 'full'])
  assert.deepEqual(modesFor('pi').map((m) => m.id), ['ask', 'edits', 'plan', 'full'])
  assert.ok(!modesFor('gemini').some((m) => m.id === 'auto'))
})
