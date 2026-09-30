import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import * as vue from 'vue'
import ts from 'typescript'

const source = readFileSync(new URL('../modules/mcp/app/composables/useCodebaseIndexing.ts', import.meta.url), 'utf8')
const compiled = ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS })
const api = {}
new Function('require', 'exports', compiled)(name => {
  assert.equal(name, 'vue')
  return vue
}, api)
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const tick = () => new Promise(resolve => setImmediate(resolve))
const success = { status: 'ok', files_indexed: 3, total_entries: 12 }
const stats = (patch = {}) => ({ workspace_id: 'a', status: 'indexed', file_count: 3, indexed_at: 10, mode: 'both', chunk_count: 12, structural_indexed_at: 10, semantic_indexed_at: 10, structural_pending_count: 0, semantic_pending_count: 0, jobs: [], ...patch })
function fixture(t) {
  const calls = []
  const state = { stats: stats(), reads: 0, read: null }
  const controller = api.useCodebaseIndexing({
    getIndexStats: async wid => { state.reads++; return state.read ? state.read(wid) : { ...state.stats, workspace_id: wid } },
    indexWorkspaceCodebase: (wid, mode, force, filter, requestId) => {
      const pending = deferred()
      calls.push({ wid, mode, force, filter, requestId, ...pending })
      return pending.promise
    },
  })
  t.after(() => controller.dispose())
  const remote = (call, status, extra = {}) => ({ request_id: call.requestId, started_at: Date.now(), mode: call.mode, status, ...extra })
  return { controller, calls, state, remote }
}

test('job completion clears busy before optional statistics finish', async t => {
  const f = fixture(t)
  f.state.read = () => new Promise(() => {})
  const done = f.controller.start('a', 'semantic', true, 'smart')
  assert(f.controller.busy('a', 'semantic'))
  f.calls[0].resolve(success)
  await done
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert.match(f.controller.message('a', 'semantic'), /Indexed 3 files/)
})

test('polling completes a job even when its original invoke never replies', async t => {
  const f = fixture(t)
  void f.controller.start('a', 'semantic', false, 'smart')
  const call = f.calls[0]
  f.state.stats = stats({ semantic_indexed_at: null, semantic_pending_count: 2, jobs: [f.remote(call, 'running')] })
  await f.controller.refresh('a')
  assert(f.controller.busy('a', 'semantic'))
  assert.equal(f.controller.completed('a', 'semantic'), false)
  f.state.stats = stats({ jobs: [f.remote(call, 'succeeded', { result: success })] })
  await f.controller.refresh('a')
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert(f.controller.completed('a', 'semantic'))
  assert.match(f.controller.message('a', 'semantic'), /Entries: 12/)
})

test('a fresh page restores a running job, then its failure without claiming success', async t => {
  const f = fixture(t)
  const job = { request_id: 'before-reload', started_at: Date.now(), mode: 'semantic', status: 'running' }
  f.state.stats = stats({ semantic_indexed_at: null, semantic_pending_count: 2, jobs: [job] })
  await f.controller.refresh('a')
  assert(f.controller.busy('a', 'semantic'))
  f.state.stats = stats({ semantic_pending_count: 2, jobs: [{ ...job, status: 'failed', error: 'Embedding failed' }] })
  await f.controller.refresh('a')
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert.equal(f.controller.completed('a', 'semantic'), false)
  assert.equal(f.controller.message('a', 'semantic'), 'Error: Embedding failed')
})

test('configured modes and existing chunks are not evidence of a completed index', async t => {
  const f = fixture(t)
  f.state.stats = stats({ semantic_indexed_at: null, semantic_pending_count: 2 })
  await f.controller.refresh('a')
  assert.equal(f.controller.completed('a', 'semantic'), false)
  assert(f.controller.completed('a', 'structural'))
  assert.match(f.controller.message('a', 'semantic'), /incomplete: 2 files/)
  f.state.stats.semantic_indexed_at = 10
  await f.controller.refresh('a')
  assert.equal(f.controller.completed('a', 'semantic'), false, 'a failed pass can still have a timestamp')
})

test('failed scans cannot claim completion even when every previously known file is current', async t => {
  const f = fixture(t)
  f.state.stats = stats({ jobs: [{ request_id: 'unreadable-new-file', started_at: Date.now(), mode: 'semantic', status: 'failed', error: 'Cannot read new file' }] })
  await f.controller.refresh('a')
  assert.equal(f.controller.completed('a', 'semantic'), false)
  assert.equal(f.controller.message('a', 'semantic'), 'Error: Cannot read new file')
})

test('statistics failure preserves counts and still reconciles the terminal job', async t => {
  const f = fixture(t)
  await f.controller.refresh('a')
  void f.controller.start('a', 'semantic', true, 'smart')
  f.state.stats = stats({ file_count: 0, chunk_count: 0, stats_error: 'Timed out', jobs: [f.remote(f.calls[0], 'succeeded', { result: success })] })
  await f.controller.refresh('a')
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert.equal(f.controller.statuses.value.a.file_count, 3)
  assert.equal(f.controller.statuses.value.a.chunk_count, 12)
  assert.equal(f.controller.statuses.value.a.stats_error, 'Timed out')
})

test('an old invoke reply cannot finish or overwrite a newer retry', async t => {
  const f = fixture(t)
  const first = f.controller.start('a', 'semantic', false, 'smart')
  f.state.stats = stats({ jobs: [f.remote(f.calls[0], 'failed', { error: 'First failed' })] })
  await f.controller.refresh('a')
  const second = f.controller.start('a', 'semantic', false, 'everything')
  f.calls[0].resolve(success)
  await first
  assert(f.controller.busy('a', 'semantic'))
  assert.equal(f.controller.message('a', 'semantic'), null)
  f.calls[1].reject(new Error('Second failed'))
  await second
  assert.equal(f.controller.message('a', 'semantic'), 'Error: Second failed')
})

test('a stats read begun before a new request cannot restore the old job', async t => {
  const f = fixture(t)
  const read = deferred()
  f.state.read = () => read.promise
  const loaded = f.controller.refresh('a')
  void f.controller.start('a', 'semantic', false, 'smart')
  read.resolve(stats({ jobs: [{ request_id: 'old', started_at: Date.now() - 1000, mode: 'semantic', status: 'succeeded', result: success }] }))
  await loaded
  assert(f.controller.busy('a', 'semantic'))
  assert.equal(f.controller.message('a', 'semantic'), null)
})

test('a stale running snapshot cannot reopen a completed job', async t => {
  const f = fixture(t)
  const done = f.controller.start('a', 'semantic', false, 'smart')
  f.state.stats = stats({ jobs: [f.remote(f.calls[0], 'running')] })
  f.calls[0].resolve(success)
  await done
  await tick()
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert.match(f.controller.message('a', 'semantic'), /Indexed 3/)
})

test('an old backend success cannot erase a newer invoke failure', async t => {
  const f = fixture(t)
  f.state.stats = stats({ jobs: [{ request_id: 'old', started_at: Date.now() - 1000, mode: 'semantic', status: 'succeeded', result: success }] })
  await f.controller.refresh('a')
  const done = f.controller.start('a', 'semantic', true, 'smart')
  f.calls[0].reject(new Error('New request failed'))
  await done
  await tick()
  assert.equal(f.controller.message('a', 'semantic'), 'Error: New request failed')
  assert.equal(f.controller.completed('a', 'semantic'), false)
  f.state.stats = stats({ jobs: [{ request_id: 'new-external-job', started_at: Date.now() + 1, mode: 'semantic', status: 'succeeded', result: success }] })
  await f.controller.refresh('a')
  assert(f.controller.completed('a', 'semantic'), 'a newer backend job can recover the failed index')
})

test('workspace switching and independent modes retain their own progress', async t => {
  const f = fixture(t)
  const a = f.controller.start('a', 'semantic', false, 'smart')
  void f.controller.start('b', 'structural', false, 'everything')
  await f.controller.start('a', 'semantic', false, 'smart')
  assert.equal(f.calls.length, 2, 'duplicate clicks do not start another request')
  f.calls[0].resolve(success)
  await a
  assert.equal(f.controller.busy('a', 'semantic'), false)
  assert(f.controller.busy('b', 'structural'))
  assert.equal(f.controller.message('b', 'structural'), null)
  f.controller.poll('a')
  await tick()
  assert(f.state.reads >= 2, 'poll includes a job outside the selected workspace')
})

test('poll does not overlap status reads and disposing ignores late responses', async t => {
  const f = fixture(t)
  const read = deferred()
  f.state.read = () => read.promise
  f.controller.poll('a')
  f.controller.poll('a')
  assert.equal(f.state.reads, 1)
  f.controller.dispose()
  read.resolve(stats())
  await tick()
  assert.deepEqual(f.controller.statuses.value, {})
})

test('a missing stats response times out and later polls can recover', async t => {
  const f = fixture(t)
  f.state.read = () => new Promise(() => {})
  await f.controller.refresh('a')
  assert.match(f.controller.statuses.value.a.stats_error, /unavailable/)
  f.state.read = null
  await f.controller.refresh('a')
  assert.equal(f.controller.statuses.value.a.stats_error, undefined)
  assert(f.controller.completed('a', 'semantic'))
})
