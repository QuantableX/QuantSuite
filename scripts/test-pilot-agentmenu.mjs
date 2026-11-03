// QuantPilot round 2, card 02: the agent menu — usable rows and reasons, recently used models, placement.
// Run: node --test scripts/test-pilot-agentmenu.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const { agentRowState, canChat, filterAgents, pushRecent, orderModels, setupLabel, placeMenu, placeSubmenu, RECENT_MAX, accountKey, splitAccountKey, accountKeyLabel } = await import('../modules/pilot/app/utils/agentMenu.ts')
const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

const ok = { id: 'claude', label: 'Claude Code', found: true, loggedIn: true, chat: true }

test('an agent row is usable or says why not, in the order a user fixes it', () => {
  assert.deepEqual(agentRowState(ok), { usable: true, reason: null, fix: null })
  assert.equal(agentRowState({ ...ok, found: false, loggedIn: false }).reason, 'Not installed')
  assert.equal(agentRowState({ ...ok, id: 'mine', chat: false }).reason, 'Terminal session only')
  assert.equal(agentRowState({ ...ok, loggedIn: false }).reason, 'Not signed in')
  // An unknown sign-in state (null) is not a problem.
  assert.equal(agentRowState({ ...ok, loggedIn: null }).usable, true)
  assert.equal(agentRowState(ok, { disabled: true }).reason, 'Off in Settings')
  assert.equal(agentRowState(ok, { projectEnabled: ['codex'] }).reason, 'Off in this project')
  assert.equal(agentRowState(ok, { projectEnabled: ['claude'] }).usable, true)
  // A working thread keeps its agent; the others wait, the own one stays usable.
  assert.equal(agentRowState(ok, { locked: 'Working — switch when done', current: 'codex' }).reason, 'Working — switch when done')
  assert.equal(agentRowState(ok, { locked: 'Working — switch when done', current: 'claude' }).usable, true)
  // Each fixable reason has its one-click fix; the others have none.
  assert.equal(agentRowState({ ...ok, found: false }).fix, 'Set up…')
  assert.equal(agentRowState({ ...ok, loggedIn: false }).fix, 'Sign in…')
  assert.equal(agentRowState({ ...ok, chat: false }).fix, null)
})

test('old status without the chat flag: only Claude and Codex chat', () => {
  assert.equal(canChat({ id: 'claude', label: '', found: true, loggedIn: null }), true)
  assert.equal(canChat({ id: 'codex', label: '', found: true, loggedIn: null }), true)
  assert.equal(canChat({ id: 'gemini', label: '', found: true, loggedIn: null }), false)
})

test('typing filters agents by every word of name or id', () => {
  const list = [ok, { ...ok, id: 'codex', label: 'Codex' }, { ...ok, id: 'gemini', label: 'Gemini CLI' }]
  assert.deepEqual(filterAgents(list, '').map((a) => a.id), ['claude', 'codex', 'gemini'])
  assert.deepEqual(filterAgents(list, 'co').map((a) => a.id), ['claude', 'codex'])
  assert.deepEqual(filterAgents(list, 'gem cli').map((a) => a.id), ['gemini'])
  assert.deepEqual(filterAgents(list, 'nope'), [])
})

test('recently used models come first, newest first, at most five, once each', () => {
  let r = {}
  for (const m of ['a', 'b', 'c', 'a', 'd', 'e', 'f']) r = pushRecent(r, 'claude', m)
  assert.deepEqual(r.claude, ['f', 'e', 'd', 'a', 'c'])
  assert.equal(r.claude.length, RECENT_MAX)
  r = pushRecent(r, 'codex', '')
  assert.deepEqual(r.codex, [''], 'the default counts as a model')
  const rows = [
    { id: '', label: 'Default', detail: '' },
    { id: 'opus', label: 'Opus', detail: '' },
    { id: 'haiku', label: 'Haiku', detail: '' },
    { id: 'my-id', label: 'Use “my-id”', detail: '', custom: true },
  ]
  const o = orderModels(rows, ['haiku', 'gone', 'my-id'])
  assert.deepEqual(o.recent.map((x) => x.id), ['haiku'], 'unknown and typed ids are not recent rows')
  assert.deepEqual(o.rest.map((x) => x.id), ['', 'opus', 'my-id'])
})

test('a setup reads as model · effort', () => {
  assert.equal(setupLabel('', ''), 'Default')
  assert.equal(setupLabel('opus', 'high', { opus: 'Opus 5.5' }), 'Opus 5.5 · high')
  assert.equal(setupLabel('x-1', ''), 'x-1')
})

test('the menu opens below its anchor, above when there is more room, never off screen', () => {
  const view = { width: 1400, height: 900 }
  const below = placeMenu({ left: 100, top: 50, width: 20, height: 24 }, { width: 340, height: 300 }, view)
  assert.equal(below.side, 'below')
  assert.equal(below.top, 50 + 24 + 6)
  const above = placeMenu({ left: 100, top: 820, width: 20, height: 24 }, { width: 340, height: 300 }, view)
  assert.equal(above.side, 'above')
  assert.ok(above.top + 300 <= 820)
  // Prefer above (the composer), but fall back below when the top is short.
  const comp = placeMenu({ left: 100, top: 60, width: 20, height: 24 }, { width: 340, height: 300 }, view, 'above')
  assert.equal(comp.side, 'below')
  // Near the right edge it moves left.
  const right = placeMenu({ left: 1300, top: 50, width: 20, height: 24 }, { width: 340, height: 300 }, view)
  assert.equal(right.left, 1400 - 8 - 340)
})

test('in a short window the menu may use the whole height', () => {
  const view = { width: 1000, height: 400 }
  const p = placeMenu({ left: 10, top: 180, width: 20, height: 24 }, { width: 340, height: 700 }, view)
  assert.equal(p.maxHeight, 400 - 16)
  assert.equal(p.top, 8)
})

test('the submenu sits right of the menu, left when the right is full, inside the window', () => {
  const view = { width: 1400, height: 900 }
  const menu = { left: 100, top: 100, width: 340, height: 400 }
  const s = placeSubmenu({ left: 100, top: 300, width: 340, height: 30 }, menu, { width: 400, height: 300 }, view)
  assert.equal(s.left, 100 + 340 + 4)
  assert.equal(s.top, 294)
  const r = placeSubmenu({ left: 1000, top: 800, width: 340, height: 30 }, { ...menu, left: 1000 }, { width: 400, height: 300 }, view)
  assert.equal(r.left, 1000 - 4 - 400)
  assert.equal(r.top, 900 - 8 - 300)
})

test('one agent menu: the old model picker is gone and every chooser uses the menu', () => {
  const read = (p) => readFileSync(join(ROOT, p), 'utf8')
  assert.throws(() => read('modules/pilot/app/components/Thread/ModelPicker.vue'))
  for (const p of [
    'modules/pilot/app/components/Thread/Controls.vue',
    'modules/pilot/app/components/Layout/LeftSidebar.vue',
    'modules/pilot/app/components/Board/CardExtra.vue',
    'modules/canvas/app/components/Windows/ChatWindow.vue',
  ]) assert.match(read(p), /<PilotAgentMenu\b/, p)
})

// Round 2, card 11: agents without a chat protocol, and the order of the rows.
test('an agent without chat starts as a terminal session where new threads are made', async () => {
  const { agentRowState, installedFirst } = await import('../modules/pilot/app/utils/agentMenu.ts')
  const freebuff = { id: 'freebuff', label: 'Freebuff', found: true, loggedIn: null, chat: false }
  assert.deepEqual(agentRowState(freebuff, { allowTerminal: true }), { usable: true, reason: null, fix: null, terminal: true })
  assert.equal(agentRowState(freebuff).reason, 'Terminal session only', 'a chat thread cannot switch to it')
  assert.equal(agentRowState({ ...freebuff, found: false }, { allowTerminal: true }).reason, 'Not installed')
  const list = [{ id: 'a', found: false }, { id: 'b', found: true }, { id: 'c', found: false }, { id: 'd', found: true }]
  assert.deepEqual(installedFirst(list).map((x) => x.id), ['b', 'd', 'a', 'c'])
})

// Round 3, card 02: other accounts of an agent are rows and usage keys of their own.
test('an account is agent@id, labelled with its name, and a removed one keeps its id', () => {
  assert.equal(accountKey('claude', ''), 'claude')
  assert.equal(accountKey('claude', null), 'claude')
  assert.equal(accountKey('claude', 'work'), 'claude@work')
  assert.deepEqual(splitAccountKey('codex@side'), { provider: 'codex', instance: 'side' })
  assert.deepEqual(splitAccountKey('codex'), { provider: 'codex', instance: '' })
  const label = (id) => ({ claude: 'Claude Code', codex: 'Codex' })[id] ?? id
  const accounts = [{ provider: 'claude', id: 'work', label: 'Work' }]
  assert.equal(accountKeyLabel('claude', label, accounts), 'Claude Code')
  assert.equal(accountKeyLabel('claude@work', label, accounts), 'Claude Code · Work')
  assert.equal(accountKeyLabel('codex@work', label, accounts), 'Codex · work', 'an account belongs to one agent')
  assert.equal(accountKeyLabel('claude@gone', label, accounts), 'Claude Code · gone', 'past usage never reads as the default account')
})
