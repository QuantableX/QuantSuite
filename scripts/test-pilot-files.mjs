// QuantPilot V3: Files tab helpers and terminal reading (card 13).
// Run: node --test scripts/test-pilot-files.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { parseFileRef, parseDelimited, languageOf, renderedOf } = await import('../modules/pilot/app/utils/files.ts')
const { serverUrls, atPrompt } = await import('../modules/pilot/app/utils/terminal.ts')
const { chordOf, usableChord, normalizeChord } = await import('../modules/pilot/app/utils/keys.ts')

test('file references in messages', () => {
  assert.deepEqual(parseFileRef('src/a.ts:12'), { path: 'src/a.ts', line: 12, column: null })
  assert.deepEqual(parseFileRef('`src/a.ts:12:5`'), { path: 'src/a.ts', line: 12, column: 5 })
  assert.deepEqual(parseFileRef('docs/README.md#L40-L52'), { path: 'docs/README.md', line: 40, column: null })
  assert.deepEqual(parseFileRef('Cargo.toml'), { path: 'Cargo.toml', line: null, column: null })
  assert.deepEqual(parseFileRef('C:\\Projects\\x\\main.rs:3'), { path: 'C:\\Projects\\x\\main.rs', line: 3, column: null })
  assert.deepEqual(parseFileRef('src/components'), { path: 'src/components', line: null, column: null })
  for (const notAFile of ['store.load', 'inspector.openFile', 'v1.2.3', 'npm', 'https://example.com/a.ts', 'foo(bar.js)', 'x.constructor', 'a b.ts']) {
    assert.equal(parseFileRef(notAFile), null, notAFile)
  }
})

test('languages and renderings', () => {
  assert.equal(languageOf('a/b/Comp.vue'), 'html')
  assert.equal(languageOf('lib.rs'), 'rust')
  assert.equal(languageOf('Dockerfile'), 'dockerfile')
  assert.equal(languageOf('x.constructor'), 'plaintext')
  assert.equal(languageOf('notes.unknown'), 'plaintext')
  assert.deepEqual(['README.md', 'index.html', 'logo.svg', 'data.tsv', 'a.ts'].map(renderedOf), ['markdown', 'html', 'svg', 'csv', null])
})

test('CSV and TSV parse with quotes and newlines', () => {
  const { rows, more } = parseDelimited('name,note\n"Doe, Jane","said ""hi""\nthen left"\r\nBob,ok\n', ',')
  assert.deepEqual(rows, [['name', 'note'], ['Doe, Jane', 'said "hi"\nthen left'], ['Bob', 'ok']])
  assert.equal(more, false)
  assert.deepEqual(parseDelimited('a\tb\n1\t2', '\t').rows, [['a', 'b'], ['1', '2']])
  const many = parseDelimited(Array.from({ length: 50 }, (_, i) => `r${i}`).join('\n'), ',', 10)
  assert.equal(many.rows.length, 10)
  assert.equal(many.more, true)
})

test('dev servers in terminal output', () => {
  const out = '\x1b[32m  ➜  Local:\x1b[39m   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m\n  ➜  Network: http://0.0.0.0:3000/app, see http://127.0.0.1:5173/'
  assert.deepEqual(serverUrls(out), [
    { port: 5173, url: 'http://localhost:5173/' },
    { port: 3000, url: 'http://localhost:3000/app' },
  ])
  assert.deepEqual(serverUrls('nothing here, https://example.com:443/x'), [])
})

test('a shell at its prompt', () => {
  assert.equal(atPrompt('npm run build\ndone\nPS C:\\Projects\\x> '), true)
  assert.equal(atPrompt('user@box:~/x$ '), true)
  assert.equal(atPrompt('C:\\x>'), true)
  assert.equal(atPrompt('  VITE v6 ready in 300 ms\n  ➜  Local: http://localhost:5173/'), false)
  assert.equal(atPrompt(''), false)
})

test('action shortcuts', () => {
  const ev = (key, code, mods = {}) => ({ key, code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods })
  assert.equal(chordOf(ev('R', 'KeyR', { ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+R')
  assert.equal(chordOf(ev('5', 'Digit5', { altKey: true })), 'Alt+5')
  assert.equal(chordOf(ev('F5', 'F5')), 'F5')
  assert.equal(chordOf(ev('Control', 'ControlLeft', { ctrlKey: true })), null)
  assert.equal(usableChord('Ctrl+Shift+R'), true)
  assert.equal(usableChord('F5'), true)
  assert.equal(usableChord('Shift+R'), false)
  assert.equal(usableChord('R'), false)
  assert.equal(normalizeChord('shift+ctrl+r'), 'Ctrl+Shift+R')
})
