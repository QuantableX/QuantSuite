// QuantPilot round 3, card 04: review guides — the header line and the saved agent.
// Run: node --test scripts/test-pilot-guide.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { riskCounts, riskLine, loadGuideAgent, saveGuideAgent } = await import('../modules/pilot/app/utils/guide.ts')

const guide = (risks) => ({ summary: '', questions: [], missing: [], cut: [], key: '', provider: 'claude', model: '', createdAt: 0, groups: [{ title: 'A', files: risks.map((risk, i) => ({ path: `f${i}`, what: '', risk, check: [], line: null })) }] })

test('a guide counts its files by risk', () => {
  assert.deepEqual(riskCounts(guide(['high', 'low', 'medium', 'low', ''])), { high: 1, medium: 1, low: 2, other: 1 })
  assert.equal(riskLine(guide(['high', 'low', 'medium', 'low', ''])), '5 files · 1 high risk · 1 medium · 2 low · 1 not covered')
  assert.equal(riskLine(guide(['low'])), '1 file · 1 low')
})

test('only Claude Code and Codex are remembered as the guide writer', () => {
  const store = new Map()
  globalThis.localStorage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v) }
  const fallback = { provider: 'claude', model: 'haiku', instance: '' }
  assert.deepEqual(loadGuideAgent(fallback), fallback)
  saveGuideAgent({ provider: 'codex', model: 'gpt-6', instance: 'work' })
  assert.deepEqual(loadGuideAgent(fallback), { provider: 'codex', model: 'gpt-6', instance: 'work' })
  store.set('qp-guide-agent', JSON.stringify({ provider: 'gemini', model: 'x' }))
  assert.deepEqual(loadGuideAgent(fallback), fallback)
})
