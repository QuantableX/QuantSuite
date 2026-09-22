/** Regression tests for asynchronous editor operations. No files or Python runs.
 * Run with Node 24+: node --test scripts/test-script-workbench.mjs
 */
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { mockIPC } from '@tauri-apps/api/mocks'
import { useWorkbenchStore } from '../modules/script/app/stores/workbench.ts'

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}
const version = (file, n = 1) => ({
  id: n,
  file,
  version: n,
  sha256: `sha-${n}`,
  size: 4,
  message: '',
  author: 'user',
  checked: true,
  created_at: '2026-09-12T10:00:00Z',
})
const doc = (file, content = 'original') => ({
  file,
  path: `C:/test/${file}`,
  kind: 'script',
  editable: true,
  content,
  sha256: 'sha-1',
  version: version(file),
  recorded: null,
})
const result = (file = 'a.py', depth = 'full') => ({
  file,
  depth,
  syntax: { ok: true },
  import: { ok: true },
  discovery_errors: {},
  indicators: [],
  registry_keys: 1,
  sandbox: null,
  stderr: '',
  ok: true,
  blocking: false,
})
const saved = (content = 'candidate') => ({
  saved: true,
  unchanged: false,
  checked: true,
  check: result('a.py', 'quick'),
  version: version('a.py', 2),
  sha256: 'sha-2',
  note: null,
})

function setup(t, handler = () => undefined) {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  globalThis.window = { crypto: globalThis.crypto }
  const memory = new Map()
  globalThis.localStorage = {
    getItem: (key) => memory.get(key) ?? null,
    setItem: (key, value) => memory.set(key, value),
  }
  setActivePinia(createPinia())
  const wb = useWorkbenchStore()
  wb.listing = {
    scripts: [],
    reference: [],
    archived: [],
    certified: [],
    python: { ok: true, command: 'test', error: null, source: 'test' },
  }
  mockIPC((cmd, args) => {
    const value = handler(cmd, args)
    if (value !== undefined) return value
    if (cmd.endsWith('read_script')) return doc(args.file)
    if (cmd.endsWith('list_scripts')) return wb.listing
    if (cmd.endsWith('lint_script')) return { ok: true, markers: [] }
    if (cmd.endsWith('list_versions')) return []
    throw new Error(`Unexpected command: ${cmd}`)
  })
  t.after(() => wb.$dispose())
  return wb
}

test('sidebar controls exit focus mode and choosing a script keeps navigation open', async (t) => {
  const wb = setup(t)
  assert.equal(wb.sidebarLeftOpen, true)
  assert.equal(wb.sidebarRightOpen, true)
  wb.focusMode = true
  wb.showInspector('guide')
  await nextTick()
  assert.equal(wb.focusMode, false)
  assert.equal(wb.sidebarRightOpen, true)
  wb.focusMode = true
  wb.toggleSidebar('right')
  assert.equal(wb.focusMode, false)
  assert.equal(wb.sidebarRightOpen, true, 'the inspector button exits focus mode and reveals the panel')
  wb.focusMode = true
  wb.toggleSidebar('left')
  assert.equal(wb.focusMode, false)
  assert.equal(wb.sidebarLeftOpen, true)
  await wb.openScript('a.py')
  assert.equal(wb.sidebarLeftOpen, true)
  assert.equal(wb.sidebarRightOpen, true)
})

test('manual sidebar choices persist and survive script and library navigation', async (t) => {
  const wb = setup(t)
  wb.toggleSidebar('left')
  wb.toggleSidebar('right')
  await nextTick()
  assert.equal(localStorage.getItem('qsc-sidebar-left'), '0')
  assert.equal(localStorage.getItem('qsc-inspector-open'), '0')
  await wb.openScript('a.py')
  wb.showLibrary()
  assert.equal(wb.sidebarLeftOpen, false)
  assert.equal(wb.sidebarRightOpen, false)
  wb.toggleSidebar('left')
  wb.toggleSidebar('right')
  assert.equal(wb.sidebarLeftOpen, true)
  assert.equal(wb.sidebarRightOpen, true)
})

test('fast opens are deduplicated and the latest click wins, including its class line', async (t) => {
  const a = deferred(),
    b = deferred()
  const calls = []
  const wb = setup(t, (cmd, args) => {
    if (!cmd.endsWith('read_script')) return
    calls.push(args.file)
    return args.file === 'a.py' ? a.promise : b.promise
  })
  const first = wb.openScript('a.py')
  const duplicate = wb.openScript('a.py')
  const latest = wb.openScript('b.py', 42)
  b.resolve(doc('b.py'))
  await latest
  a.resolve(doc('a.py'))
  await Promise.all([first, duplicate])
  assert.deepEqual(calls, ['a.py', 'b.py'])
  assert.equal(wb.open.length, 2)
  assert.equal(wb.activeFile, 'b.py')
  assert.equal(wb.revealRequest.line, 42)
  assert.equal(wb.openingFile, null)
})

test('returning to the library cancels pending activation without dropping the file', async (t) => {
  const pending = deferred()
  const wb = setup(t, (cmd) => (cmd.endsWith('read_script') ? pending.promise : undefined))
  const opening = wb.openScript('a.py')
  wb.showLibrary()
  pending.resolve(doc('a.py'))
  await opening
  assert.equal(wb.libraryOpen, true)
  assert.equal(wb.activeFile, null)
  assert.equal(wb.open.length, 1)
})

test('check results are stale when typing continues and become current after undo', async (t) => {
  const pending = deferred()
  const wb = setup(t, (cmd) => (cmd.endsWith('check_script') ? pending.promise : undefined))
  await wb.openScript('a.py')
  const checking = wb.check()
  wb.setContent('a.py', 'new edit')
  pending.resolve(result())
  await checking
  assert.equal(wb.checkStale, true)
  assert.equal(wb.active.checkContent, 'original')
  assert.equal(wb.resultsTab, 'check')
  assert.match(wb.notice.text, /earlier edit/)
  wb.setContent('a.py', 'original')
  assert.equal(wb.checkStale, false)
})

test('save persists only its snapshot and refuses overlapping check/close operations', async (t) => {
  const pending = deferred()
  let request
  const wb = setup(t, (cmd, args) => {
    if (cmd.endsWith('save_script')) {
      request = args
      return pending.promise
    }
  })
  await wb.openScript('a.py')
  wb.setContent('a.py', 'candidate')
  const saving = wb.save()
  assert.equal(await wb.check(), null)
  assert.equal(wb.closeScript('a.py', true), false)
  wb.setContent('a.py', 'newer edit')
  pending.resolve(saved())
  await saving
  assert.equal(request.content, 'candidate')
  assert.equal(wb.active.saved, 'candidate')
  assert.equal(wb.active.content, 'newer edit')
  assert.equal(wb.isDirty('a.py'), true)
  assert.equal(wb.checkStale, true)
})

test('restoring an unchanged disk version still resets the dirty editor', async (t) => {
  const wb = setup(t, (cmd) => {
    if (cmd.endsWith('read_version')) return { meta: version('a.py'), content: 'original' }
    if (cmd.endsWith('restore_version'))
      return { ...saved(), saved: false, unchanged: true, version: null, sha256: 'sha-1' }
  })
  await wb.openScript('a.py')
  wb.setContent('a.py', 'discard me')
  assert.equal(await wb.restore('a.py', 1), true)
  assert.equal(wb.active.content, 'original')
  assert.equal(wb.isDirty('a.py'), false)
})

test('restoring a version preserves text typed while the restore runs', async (t) => {
  const pending = deferred()
  const wb = setup(t, (cmd) => {
    if (cmd.endsWith('read_version')) return { meta: version('a.py'), content: 'restored' }
    if (cmd.endsWith('restore_version')) return pending.promise
  })
  await wb.openScript('a.py')
  const restoring = wb.restore('a.py', 1)
  await Promise.resolve()
  wb.setContent('a.py', 'typed while restoring')
  pending.resolve(saved())
  await restoring
  assert.equal(wb.active.content, 'typed while restoring')
  assert.equal(wb.active.saved, 'restored')
  assert.match(wb.notice.text, /newer editor changes/)
})

test('an older lint cannot clear or replace a newer in-flight lint', async (t) => {
  const first = deferred(),
    second = deferred()
  let calls = 0
  const wb = setup(t, (cmd) =>
    cmd.endsWith('lint_script') ? (++calls === 1 ? first.promise : second.promise) : undefined,
  )
  await wb.openScript('a.py')
  const lint1 = wb.lint('a.py')
  wb.setContent('a.py', 'edited')
  const lint2 = wb.lint('a.py')
  first.resolve({ ok: true, markers: [] })
  await lint1
  assert.equal(wb.active.linting, true)
  assert.equal(wb.active.lint, null)
  second.resolve({ ok: false, markers: [{ severity: 'error', line: 1, column: 1, message: 'Syntax error' }] })
  await lint2
  assert.equal(wb.active.lint.markers.length, 1)
  assert.equal(wb.active.linting, false)
})

test('library filters combine with case-insensitive name, file and description search', async (t) => {
  const wb = setup(t)
  wb.listing.scripts = [
    {
      file: 'a.py',
      kind: 'script',
      summary: 'Adaptive trend',
      registration: 'discovered',
      classes: [{ key: 'TREND', class_name: 'ATrend', name: 'Trend' }],
      versions: { external: false },
    },
    {
      file: 'b.py',
      kind: 'script',
      summary: 'Momentum',
      registration: 'explicit',
      classes: [],
      versions: { external: false },
    },
  ]
  wb.toggleFavorite('a.py')
  wb.libraryFilter = 'favorites'
  wb.search = 'adaptive'
  assert.deepEqual(
    wb.filtered.map((s) => s.file),
    ['a.py'],
  )
  wb.search = 'trend'
  assert.equal(wb.filtered.length, 1)
  wb.libraryFilter = 'attention'
  assert.equal(wb.filtered.length, 0)
  await wb.openScript('a.py')
  wb.setContent('a.py', 'changed')
  assert.equal(wb.filtered.length, 1)
  await nextTick()
  assert.deepEqual(JSON.parse(localStorage.getItem('qsc-favorites')), ['a.py'])
  assert.deepEqual(JSON.parse(localStorage.getItem('qsc-recent')), ['a.py'])
})

test('a blocked save keeps the editor dirty and opens its check report', async (t) => {
  const wb = setup(t, (cmd) => cmd.endsWith('save_script') ? {
    ...saved(), saved: false, version: null,
    check: { ...result(), ok: false, blocking: true, syntax: { ok: false, line: 1, column: 1, message: 'Invalid syntax', text: 'broken' } },
    note: 'Cannot save a syntax error',
  } : undefined)
  await wb.openScript('a.py')
  wb.setContent('a.py', 'broken')
  await wb.save()
  assert.equal(wb.active.content, 'broken')
  assert.equal(wb.active.saved, 'original')
  assert.equal(wb.active.version.version, 1)
  assert.equal(wb.problemsOpen, true)
  assert.equal(wb.resultsTab, 'check')
  assert.equal(wb.checkStale, false)
  assert.equal(wb.notice.tone, 'error')
})

test('lint failures are reported instead of presenting a clean result', async (t) => {
  t.mock.method(console, 'error', () => {})
  const wb = setup(t, (cmd) => cmd.endsWith('lint_script') ? Promise.reject(new Error('Interpreter unavailable')) : undefined)
  await wb.openScript('a.py')
  await wb.lint('a.py')
  assert.equal(wb.active.lint, null)
  assert.equal(wb.active.linting, false)
  assert.match(wb.active.lintError, /Interpreter unavailable/)
})

test('switching tabs resets the cursor and panel without losing dirty content', async (t) => {
  const wb = setup(t)
  await wb.openScript('a.py')
  wb.setContent('a.py', 'unfinished')
  wb.cursor = { line: 40, column: 8 }
  wb.showResults('check')
  await wb.openScript('b.py')
  assert.deepEqual(wb.cursor, { line: 1, column: 1 })
  assert.equal(wb.problemsOpen, false)
  assert.equal(wb.closeScript('a.py'), false)
  wb.activate('a.py')
  assert.equal(wb.active.content, 'unfinished')
})
