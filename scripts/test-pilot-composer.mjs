// QuantPilot V3: the composer's editing and key rules (card 4).
// Run: node --test scripts/test-pilot-composer.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { newLine, indent, inCodeFence, enterAction, highlight, sizeLabel } = await import('../modules/pilot/app/utils/composer.ts')

const at = (text) => {
  const i = text.indexOf('|')
  return [text.replace('|', ''), i]
}

test('Shift+Enter continues bullets, numbers and task lists', () => {
  let [t, c] = at('- one|')
  assert.equal(newLine(t, c, c).text, '- one\n- ')
  ;[t, c] = at('1. first|')
  assert.equal(newLine(t, c, c).text, '1. first\n2. ')
  ;[t, c] = at('- [x] done|')
  assert.equal(newLine(t, c, c).text, '- [x] done\n- [ ] ')
  ;[t, c] = at('> quoted|')
  assert.equal(newLine(t, c, c).text, '> quoted\n> ')
})

test('an empty item steps out one level, then leaves the list', () => {
  let [t, c] = at('- a\n  - |')
  let e = newLine(t, c, c)
  assert.equal(e.text, '- a\n- ')
  ;[t, c] = at('- a\n- |')
  e = newLine(t, c, c)
  assert.equal(e.text, '- a\n')
  assert.equal(e.start, 4)
})

test('inside a code block Enter keeps the indentation; the fence is tracked', () => {
  const [t, c] = at('```ts\n  const x = 1|')
  assert.equal(inCodeFence(t, c), true)
  assert.equal(newLine(t, c, c).text, '```ts\n  const x = 1\n  ')
  assert.equal(inCodeFence('```\na\n```\nb', 11), false)
})

test('Tab nests list items and code lines, and is left alone elsewhere', () => {
  let [t, c] = at('- a\n- b|')
  let e = indent(t, c, c, 1)
  assert.equal(e.text, '- a\n  - b')
  e = indent(e.text, e.start, e.end, -1)
  assert.equal(e.text, '- a\n- b')
  assert.equal(indent('plain text', 3, 3, 1), null)
  ;[t, c] = at('```\nfoo|')
  assert.equal(indent(t, c, c, 1).text, '```\n  foo')
})

const base = { shift: false, mod: false, alt: false, shortcut: 'enter', followUp: 'queue', running: false, canSteer: true, text: 'hi', caret: 2 }

test('Enter sends; while working it follows the setting and Ctrl flips it', () => {
  assert.equal(enterAction(base), 'send')
  assert.equal(enterAction({ ...base, shift: true }), 'newline')
  assert.equal(enterAction({ ...base, running: true }), 'queue')
  assert.equal(enterAction({ ...base, running: true, mod: true }), 'steer')
  assert.equal(enterAction({ ...base, running: true, followUp: 'steer' }), 'steer')
  assert.equal(enterAction({ ...base, running: true, followUp: 'steer', mod: true }), 'queue')
  assert.equal(enterAction({ ...base, running: true, followUp: 'steer', canSteer: false }), 'queue', 'no steer without support')
  assert.equal(enterAction({ ...base, running: true, mod: true, shift: true }), 'steerQueued')
  assert.equal(enterAction({ ...base, mod: true, alt: true }), 'sendNewThread')
})

test('Ctrl+Enter shortcuts: multiline-only and always', () => {
  const multi = { ...base, text: 'a\nb', caret: 3 }
  assert.equal(enterAction({ ...multi, shortcut: 'modEnterMultiline' }), 'newline')
  assert.equal(enterAction({ ...multi, shortcut: 'modEnterMultiline', mod: true }), 'send')
  assert.equal(enterAction({ ...base, shortcut: 'modEnterMultiline' }), 'send', 'a single line still sends on Enter')
  assert.equal(enterAction({ ...base, shortcut: 'modEnter' }), 'newline')
  assert.equal(enterAction({ ...base, shortcut: 'modEnter', mod: true, running: true }), 'queue')
  assert.equal(enterAction({ ...base, shortcut: 'modEnter', mod: true, shift: true, running: true }), 'steer')
})

test('Enter inside an open code block makes a new line', () => {
  const [t, c] = at('```\ncode|')
  assert.equal(enterAction({ ...base, text: t, caret: c }), 'newline')
  assert.equal(enterAction({ ...base, text: t, caret: c, mod: true }), 'send')
})

test('the backdrop keeps every character and escapes HTML', () => {
  const src = '# Title\n- **bold** and `x<y`\n```\n<b>\n```\n/compact now'
  const html = highlight(src)
  const textOnly = html.replace(/<[^>]+>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
  assert.equal(textOnly, src)
  assert.ok(!html.includes('<b>'))
  assert.ok(html.includes('qp-hl-heading'))
  assert.ok(highlight('a\n').endsWith(' '), 'a trailing newline keeps its line')
})

test('size labels', () => {
  assert.equal(sizeLabel(48 * 1024), '48 KB')
  assert.equal(sizeLabel(3 * 1024 * 1024), '3.0 MB')
})
