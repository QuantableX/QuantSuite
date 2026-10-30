// QuantPilot V3 card 17: keybindings — key normalization, when expressions, precedence, conflicts.
// Run: node --test scripts/test-pilot-keys.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const K = await import('../modules/pilot/app/utils/keybindings.ts')

const ev = (key, code, mods = {}) => ({ key, code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods })

test('keydowns become chords: letters by layout, digits and punctuation by key', () => {
  assert.equal(K.chordOf(ev('m', 'KeyM', { ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+M')
  // A German keyboard: the key labelled Z sits where US has Y.
  assert.equal(K.chordOf(ev('z', 'KeyY', { ctrlKey: true })), 'Ctrl+Z')
  // A Cyrillic layout falls back to the key's place.
  assert.equal(K.chordOf(ev('я', 'KeyZ', { ctrlKey: true })), 'Ctrl+Z')
  assert.equal(K.chordOf(ev('!', 'Digit1', { ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+1')
  assert.equal(K.chordOf(ev('{', 'BracketLeft', { ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+[')
  assert.equal(K.chordOf(ev('[', 'BracketLeft', { ctrlKey: true })), 'Ctrl+[')
  assert.equal(K.chordOf(ev('F6', 'F6', { shiftKey: true })), 'Shift+F6')
  assert.equal(K.chordOf(ev(',', 'Comma', { metaKey: true })), 'Ctrl+,')
  assert.equal(K.chordOf(ev('Shift', 'ShiftLeft', { shiftKey: true })), null)
})

test('written keys normalize, wrong ones are refused', () => {
  assert.equal(K.normalizeKey('shift+mod+m'), 'Ctrl+Shift+M')
  assert.equal(K.normalizeKey('ctrl+['), 'Ctrl+[')
  assert.equal(K.normalizeKey('Option+f6'), 'Alt+F6')
  assert.equal(K.normalizeKey('mod++'), 'Ctrl++')
  assert.equal(K.normalizeKey('ctrl+arrowup'), 'Ctrl+ArrowUp')
  assert.equal(K.normalizeKey('hyper+x'), null)
  assert.equal(K.normalizeKey('ctrl+'), null)
  assert.equal(K.usableKey('Ctrl+Shift+R'), true)
  assert.equal(K.usableKey('F5'), true)
  assert.equal(K.usableKey('Shift+R'), false)
})

test('when expressions: !, &&, ||, parentheses, unknown keys are false', () => {
  const on = (text, ctx) => K.evalWhen(K.parseWhen(text), ctx)
  assert.equal(on('!terminalFocus', {}), true)
  assert.equal(on('terminalOpen && !terminalFocus', { terminalOpen: true }), true)
  assert.equal(on('a || b && c', { a: false, b: true, c: false }), false, '&& binds tighter than ||')
  assert.equal(on('(a || b) && c', { a: true, c: true }), true)
  assert.equal(on('!!a', { a: true }), true)
  assert.equal(on('notAContextKey', {}), false)
  for (const bad of ['', 'a &&', '(a', 'a b', 'a & b', ')', '!']) assert.throws(() => K.parseWhen(bad), bad)
  assert.throws(() => K.parseWhen('('.repeat(70) + 'a' + ')'.repeat(70)), /deep/)
})

test('the last matching rule wins, whatever its command', () => {
  const rules = K.compile([
    { key: 'Ctrl+J', command: 'chat.find', when: 'composerFocus' },
  ])
  assert.equal(K.resolve(rules, 'Ctrl+J', {}), 'terminal.toggle')
  assert.equal(K.resolve(rules, 'Ctrl+J', { composerFocus: true }), 'chat.find')
  assert.equal(K.resolve(rules, 'Ctrl+D', { terminalFocus: true }), null, 'the terminal keeps Ctrl+D')
  assert.equal(K.resolve(rules, 'Ctrl+1', {}), 'thread.jump.1')
  assert.equal(K.resolve(rules, 'Ctrl+1', { paletteOpen: true }), null, 'the palette picks its own rows')
  assert.equal(K.resolve(rules, 'Ctrl+W', { inspectorOpen: false }), null)
})

test('rebinding, unbinding and resetting a command', () => {
  let user = K.rebind([], 'diff.toggle', 'Ctrl+Alt+D')
  let rules = K.compile(user)
  assert.deepEqual(K.keysOf(rules, 'diff.toggle'), ['Ctrl+Alt+D'])
  assert.equal(K.resolve(rules, 'Ctrl+D', {}), null, 'the old key is free')
  assert.equal(K.resolve(rules, 'Ctrl+Alt+D', { terminalFocus: true }), null, 'the default condition stays')
  user = K.unbind(user, 'chat.find')
  assert.deepEqual(K.keysOf(K.compile(user), 'chat.find'), [])
  user = K.resetBinding(user, 'diff.toggle')
  assert.deepEqual(K.keysOf(K.compile(user), 'diff.toggle'), ['Ctrl+D'])
})

test("the user's file: invalid rules are skipped and named, bad JSON means the defaults", () => {
  const text = JSON.stringify([
    { key: 'ctrl+alt+d', command: 'diff.toggle' },
    { key: 'ctrl+q', command: 'no.such.command' },
    { key: 'bogus+q', command: 'chat.find' },
    { key: 'ctrl+q', command: 'chat.find', when: 'a &&' },
    { key: 'ctrl+d', command: '-diff.toggle' },
  ])
  const { rules, problems } = K.parseUserRules(text)
  assert.deepEqual(rules.map((r) => `${r.key} ${r.command}`), ['Ctrl+Alt+D diff.toggle', 'Ctrl+D -diff.toggle'])
  assert.equal(problems.length, 3)
  assert.match(problems[0], /unknown command/)
  assert.deepEqual(K.parseUserRules('{nope').rules, [])
  assert.match(K.parseUserRules('{nope').problems[0], /not valid JSON/)
  assert.deepEqual(K.parseUserRules(null), { rules: [], problems: [], skipped: [] })
  assert.equal(K.parseUserRules(text).skipped.length, 3, 'kept for the user to fix')
})

test('conflicts: other commands on the key, the suite and project actions', () => {
  const rules = K.compile([])
  assert.deepEqual(K.conflicts(rules, 'chat.find', 'Ctrl+D', '!terminalFocus'), ['Diff'])
  assert.deepEqual(K.conflicts(rules, 'chat.find', 'Ctrl+D', 'terminalFocus'), [], 'exact opposites never meet')
  assert.deepEqual(K.conflicts(rules, 'chat.find', 'Ctrl+Shift+N'), ['QuantSuite quick note'])
  assert.deepEqual(K.conflicts(rules, 'chat.find', 'Ctrl+Alt+R', undefined, { 'Ctrl+Alt+R': 'Project action "Test"' }), ['Project action "Test"'])
  assert.deepEqual(K.conflicts(rules, 'commandPalette.toggle', 'Ctrl+K', '!terminalFocus'), [])
})

test('every default names a known command, and every command a title', () => {
  const ids = new Set(K.COMMANDS.map((c) => c.id))
  for (const r of K.DEFAULT_RULES) {
    assert.ok(ids.has(r.command), r.command)
    assert.ok(K.normalizeKey(r.key) === r.key, `${r.key} is canonical`)
    if (r.when) K.parseWhen(r.when)
  }
  assert.equal(ids.size, K.COMMANDS.length, 'no duplicate ids')
})

const P = await import('../modules/pilot/app/utils/palette.ts')

test('palette: every word must match; title starts beat other fields', () => {
  const now = Date.UTC(2026, 9, 30)
  const t = (id, title, extra = {}) => ({ id, title, branch: '', project: 'QuantSuite', pr: '', lastActivityAt: now - 3_600_000, ...extra })
  const threads = [t('a', 'Fix the parser'), t('b', 'Parser tests', { branch: 'feat/lexer' }), t('c', 'Sidebar rows', { pr: '#42 Faster rows' })]
  assert.deepEqual(P.rankThreads(threads, 'parser', {}, {}, now).map((x) => x.id), ['b', 'a'])
  assert.deepEqual(P.rankThreads(threads, 'lexer', {}, {}, now).map((x) => x.id), ['b'], 'the branch counts')
  assert.deepEqual(P.rankThreads(threads, '#42', {}, {}, now).map((x) => x.id), ['c'], 'the pull request counts')
  assert.deepEqual(P.rankThreads(threads, 'parser nope', {}, {}, now), [])
})

test('palette: frecency lifts often-used threads; the last pick for a query wins for three weeks', () => {
  const now = Date.UTC(2026, 9, 30)
  const t = (id, title) => ({ id, title, branch: '', project: '', pr: '', lastActivityAt: now - 86_400_000 * 20 })
  const threads = [t('a', 'Parser one'), t('b', 'Parser two')]
  assert.equal(P.rankThreads(threads, 'parser', { b: { n: 9, at: now - 3_600_000 } }, {}, now)[0].id, 'b')
  const picks = { parser: { id: 'a', at: now - 86_400_000 * 20 } }
  assert.equal(P.rankThreads(threads, 'Parser ', { b: { n: 9, at: now } }, picks, now)[0].id, 'a', 'picked 20 days ago')
  const old = { parser: { id: 'a', at: now - 86_400_000 * 22 } }
  assert.equal(P.rankThreads(threads, 'parser', { b: { n: 9, at: now } }, old, now)[0].id, 'b', 'the boost ends after three weeks')
  // No query: the most used and recent first.
  assert.equal(P.rankThreads(threads, '', { a: { n: 3, at: now } }, {}, now)[0].id, 'a')
})
