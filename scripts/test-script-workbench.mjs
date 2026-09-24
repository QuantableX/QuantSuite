/** Regression tests for asynchronous editor operations. No files or Python runs.
 * Run with Node 24+: node --test scripts/test-script-workbench.mjs
 */
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { mockIPC } from '@tauri-apps/api/mocks'
import { useWorkbenchStore } from '../modules/script/app/stores/workbench.ts'
import { decisionLabel, optimizeRowsFor, versionParamDiff, versionSlots, versionWhy } from '../modules/script/app/utils/forge.ts'
import { DEFAULT_COLLECTION_FILTERS, filterItems, itemState, planSummary, trackVerdict } from '../modules/script/app/utils/catalog.ts'

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

test('an optimize job folds into one row per indicator and track', () => {
  const job = {
    id: 'j', kind: 'optimize', indicators: ['dmi'], status: 'done', started_at: '', finished_at: null,
    exit_code: 0, error: null, request: { kind: 'optimize', indicators: ['dmi'], fast: false, timeframe: 'all' },
    summary: [],
    events: [
      { event: 'job', kind: 'optimize', indicators: ['dmi'], tracks: ['1h', '4h'] },
      { event: 'begin', indicator: 'dmi', name: 'DMITrend' },
      { event: 'walkforward', indicator: 'dmi', timeframe: '1h', wfe: 0.62 },
      { event: 'decision', indicator: 'dmi', timeframe: '1h', key: 'dmi_opt_1h', decision: 'promoted', score: 77,
        grade: 'A', written: true, candidates: [{ source: 'general', score: 71 }, { source: 'standard', score: 52 },
        { source: 'walkforward', score: 77 }] },
      { event: 'error', indicator: 'dmi', timeframe: '4h', message: 'walk-forward: boom' },
    ],
  }
  const rows = optimizeRowsFor(job)
  assert.deepEqual(rows.map((r) => r.key), ['dmi@1h', 'dmi@4h'])
  const [h1, h4] = rows
  assert.equal(h1.name, 'DMITrend')
  assert.equal(h1.wfe, 0.62)
  assert.deepEqual([h1.standard, h1.general, h1.winner, h1.score], [52, 71, 77, 77])
  assert.equal(h1.versionKey, 'dmi_opt_1h')
  assert.equal(decisionLabel(h1.decision), 'walk-forward winner')
  assert.equal(h4.error, 'walk-forward: boom')
  assert.equal(h4.decision, null)
})

test('the version slots resolve roles against the registry and explain themselves', () => {
  const entry = (key, params, extra = {}) => ({ key, name: key, hypothesis: '', params, param_space: {}, warmup_bars: 10, certification: null, ...extra })
  const base = entry('alpha', { length: 20, mult: 2 }, {
    role: 'standard',
    versions: { standard: 'alpha', optimized: 'alpha_opt', optimized_1h: null, optimized_4h: 'alpha_opt_4h', optimized_1d: 'alpha_opt_1d' },
  })
  const general = entry('alpha_opt', { length: 34, mult: 2 }, { base_key: 'alpha', role: 'optimized', variant: { base_key: 'alpha', label: 'Optimized', status: 'release', role: 'optimized', evidence: { decision: 'current' } } })
  const h4 = entry('alpha_opt_4h', { length: 20, mult: 2 }, { base_key: 'alpha', role: 'optimized_4h', variant: { base_key: 'alpha', label: 'Optimized 4H', status: 'release', role: 'optimized_4h', evidence: { decision: 'standard_retained' } } })
  const byKey = new Map([base, general, h4].map((e) => [e.key, e]))
  const slots = versionSlots(base, byKey)
  assert.deepEqual(slots.map((s) => s.role), ['standard', 'optimized', 'optimized_1h', 'optimized_4h', 'optimized_1d'])
  assert.deepEqual(slots.map((s) => s.key), ['alpha', 'alpha_opt', null, 'alpha_opt_4h', null], 'a slot whose key the registry lacks stays empty')
  assert.equal(slots[3].track, '4h')
  assert.equal(slots[0].track, null)
  assert.equal(versionWhy(slots[0]), 'the normal parameters')
  assert.equal(versionWhy(slots[1]), "today's parameters, frozen")
  assert.equal(versionWhy(slots[3]), 'Standard kept')
  assert.deepEqual(versionParamDiff(slots[1], base.params), { length: 34 })
  assert.deepEqual(versionParamDiff(slots[3], base.params), {})
  assert.deepEqual(versionParamDiff(slots[2], base.params), {})
  // A registry without version slots (an older engine): the Standard alone.
  assert.deepEqual(versionSlots({ key: 'beta', versions: null }, new Map([['beta', entry('beta', {})]])).map((s) => s.key), ['beta', null, null, null, null])
})

test('the Collection lists, filters and badges catalog items and sums up a plan', () => {
  const local = (over = {}) => ({ installed_version: null, installed_commit: null, update: false, modified: false, present: false, conflict: false, too_new: false, ...over })
  const verdict = (score, certified = score >= 70) => ({ score, grade: score >= 70 ? 'A' : 'B', certified })
  const items = [
    { type: 'indicator', key: 'alpha', name: 'Alpha', summary: 'fast trend', tags: ['indicator'], version: '1.2.0', contract: 2, requires: ['helper'],
      scores: { standard: { '1d': verdict(62) }, optimized: { '1d': verdict(74) }, optimized_4h: { '4h': verdict(81) } }, manifest: {}, local: local({ installed_version: '1.1.0', update: true }) },
    { type: 'indicator', key: 'beta', name: 'Beta', summary: 'slow', tags: ['indicator', 'ensemble'], version: '1.0.0', contract: 2, requires: [],
      scores: { standard: { '1d': verdict(55), '4h': verdict(58) } }, manifest: {}, local: local({ conflict: true, present: true }) },
    { type: 'library', key: 'helper', name: 'helper', summary: 'helpers', tags: ['library'], version: '1.0.0', contract: 2, requires: [],
      scores: {}, manifest: {}, local: local({ installed_version: '1.0.0' }) },
  ]
  const keys = (f) => filterItems(items, { ...DEFAULT_COLLECTION_FILTERS, ...f }).map((i) => i.key)
  assert.deepEqual(keys({}), ['alpha', 'beta', 'helper'])
  assert.deepEqual(keys({ search: 'SLOW' }), ['beta'])
  assert.deepEqual(keys({ tag: 'ensemble' }), ['beta'])
  assert.deepEqual(keys({ state: 'installed' }), ['alpha', 'helper'])
  assert.deepEqual(keys({ state: 'update' }), ['alpha'])
  assert.deepEqual(keys({ state: 'available' }), ['beta'])
  assert.deepEqual(keys({ track: '1d', minScore: 70 }), ['alpha'], 'the best version on the track counts; a library has no score')
  assert.deepEqual(keys({ track: '4h', certifiedOnly: true }), ['alpha'])
  assert.deepEqual(keys({ track: '1h', minScore: 1 }), [])
  assert.deepEqual(trackVerdict(items[0], '4h'), { score: 81, grade: 'A', certified: true, role: 'optimized_4h' })

  assert.equal(itemState(items[0].local, '1.2.0').label, 'Update 1.1.0 → 1.2.0')
  assert.equal(itemState(items[1].local, '1.0.0').label, 'In your library')
  assert.equal(itemState(items[2].local, '1.0.0').label, 'Installed 1.0.0')
  assert.equal(itemState(local({ installed_version: '1.0.0', modified: true, update: true }), '1.1.0').label, 'Modified')
  assert.equal(itemState(local({ too_new: true }), '1.0.0').tone, 'is-error')
  assert.equal(itemState(null, '1.0.0').label, 'Available')

  const plan = { source: 's', commit: 'c', conflicts: 1, items: [
    { key: 'helper', action: 'skip', requested: false },
    { key: 'gamma', action: 'create', requested: false },
    { key: 'beta', action: 'conflict', requested: true },
  ] }
  assert.deepEqual(planSummary(plan), { create: 1, replace: 0, skip: 1, conflict: 1, dependencies: 1, empty: false })
  assert.equal(planSummary({ ...plan, conflicts: 0, items: [plan.items[0]] }).empty, true)
})
