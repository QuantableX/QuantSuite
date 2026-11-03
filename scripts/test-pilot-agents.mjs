// QuantPilot round 2, card 01: agent logos instead of letter marks.
// Run: node --test scripts/test-pilot-agents.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const { AGENT_MARKS, agentMark, agentInitial, agentGlyph, agentSlug } = await import('../modules/pilot/app/utils/agentIcons.ts')

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const ASSETS = join(ROOT, 'modules/pilot/app/assets/agents')
const files = readdirSync(ASSETS).filter((f) => f !== 'NOTICE.md')

test('every mark has its bundled file, and every file is a registered mark', () => {
  const used = new Set(Object.values(AGENT_MARKS).map((m) => m.file))
  for (const f of used) assert.ok(files.includes(f), `missing ${f}`)
  for (const f of files) assert.ok(used.has(f), `unregistered ${f}`)
})

test('the notice names the source of every file', () => {
  const notice = readFileSync(join(ASSETS, 'NOTICE.md'), 'utf8')
  for (const f of files) assert.ok(notice.includes(`\`${f}\``), `NOTICE.md lacks ${f}`)
  assert.match(notice, /MIT License/)
})

test('the bundled files are small, local and inert', () => {
  for (const f of files) {
    const size = statSync(join(ASSETS, f)).size
    assert.ok(size <= 8 * 1024, `${f} is ${size} bytes`)
    if (!f.endsWith('.svg')) continue
    const svg = readFileSync(join(ASSETS, f), 'utf8')
    assert.match(svg, /^<svg [^>]*xmlns="http:\/\/www\.w3\.org\/2000\/svg"/, f)
    assert.match(svg, /viewBox="/, f)
    assert.doesNotMatch(svg, /[{}]|<script|href="http|on[a-z]+=/i, f)
  }
})

test('built-in agents and their other names get their mark', () => {
  for (const id of ['claude', 'codex', 'gemini', 'opencode', 'copilot', 'pi', 'omp']) assert.ok(agentMark(id), id)
  for (const id of ['cursor', 'kilo', 'hermes', 'openclaw', 'freebuff', 'grok', 'antigravity', 'kiro', 'droid', 'cline', 'devin']) assert.ok(agentMark(id), id)
  assert.equal(agentMark('Claude')?.file, 'claude.svg')
  assert.equal(agentMark('cursor-agent')?.file, 'cursor.png')
  assert.equal(agentMark('gemini-cli')?.file, 'gemini.svg')
  assert.equal(agentMark('codex')?.tone, 'mono')
  assert.equal(agentMark('claude')?.tone, 'color')
  assert.equal(agentMark('my-agent'), null)
  assert.equal(agentMark(''), null)
  assert.equal(agentMark(null), null)
})

test('an unknown or loading identity is neutral, never another agent', () => {
  assert.deepEqual(agentGlyph(null), { kind: 'unknown' })
  assert.deepEqual(agentGlyph(undefined), { kind: 'unknown' })
  // An id the status does not list yet: no letter guessed from the id, no logo.
  assert.deepEqual(agentGlyph('mystery'), { kind: 'unknown' })
  assert.deepEqual(agentGlyph('mystery', { label: 'Mystery' }), { kind: 'unknown' })
  // A V2 terminal row without an agent.
  assert.deepEqual(agentGlyph(''), { kind: 'terminal' })
})

test('custom agents show their image, else their letter', () => {
  assert.deepEqual(agentGlyph('my-agent', { label: 'My Agent', known: true }), { kind: 'letter', letter: 'M' })
  assert.deepEqual(agentGlyph('my-agent', { label: 'My Agent', known: true, image: 'data:image/png;base64,AA' }), { kind: 'image', src: 'data:image/png;base64,AA' })
  // The user's image wins over a bundled mark of the same id.
  assert.equal(agentGlyph('cursor', { image: 'data:image/png;base64,BB', known: true }).kind, 'image')
  assert.equal(agentGlyph('claude', { known: true, label: 'Claude Code' }).kind, 'mark')
})

test('letters and ids', () => {
  assert.equal(agentInitial('my agent'), 'M')
  assert.equal(agentInitial('  42 robots'), '4')
  assert.equal(agentInitial('— éclair'), 'É')
  assert.equal(agentInitial(''), '?')
  assert.equal(agentInitial(null), '?')
  // Same as settings.rs `slug`: lowercase, every other character a dash, dashes trimmed.
  assert.equal(agentSlug('  My Agent '), 'my-agent')
  assert.equal(agentSlug('Ünïcode'), 'n-code')
  assert.equal(agentSlug('a  b'), 'a--b')
  assert.equal(agentSlug('---'), '')
})

test('no two-letter marks are left in the thread rows', () => {
  const row = readFileSync(join(ROOT, 'modules/pilot/app/components/Layout/ThreadRow.vue'), 'utf8')
  assert.doesNotMatch(row, /MARKS|'Cl'|'Cx'/)
  assert.match(row, /<PilotAgentIcon /)
})

test('the agents round 2 adds have their marks under their ids', () => {
  for (const id of ['cursor-agent', 'kilo', 'hermes', 'openclaw', 'freebuff', 'grok', 'agy', 'kiro-cli', 'droid', 'cline', 'devin']) assert.ok(agentMark(id), id)
  assert.equal(agentMark('agy')?.file, 'antigravity.png')
})
