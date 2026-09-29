import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import ts from 'typescript'

const source = readFileSync(new URL('../packages/core/src/workspaces.ts', import.meta.url), 'utf8')
const compiled = ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS })
const rows = () => ['a', 'b', 'c'].map((id, i) => ({
  id, name: id.toUpperCase(), path: `C:/fixtures/${id}`, pinned: false, lastOpenedAt: 30 - i,
}))
function fixture(native = true) {
  const state = { settings: {}, entities: rows().map(w => ({ id: w.id, title: w.name, subtitle: w.path, updated_at: 1, payload: { ...w, custom: 'preserve' } })), events: [], failWrite: false, failRead: false }
  const storage = new Map([['qss-workspaces-dev', JSON.stringify({ list: rows(), active: null, reopenLast: false })]])
  const localStorage = { getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value) }
  function session() {
    const handlers = new Map()
    const core = {
      async getSetting(scope, key) { if (state.failRead) throw new Error('read failed'); return structuredClone(state.settings[key] ?? null) },
      async setSetting(scope, key, value) { if (state.failWrite) throw new Error('write failed'); state.settings[key] = structuredClone(value) },
      async listEntities() { return structuredClone(state.entities) },
      async upsertEntity(entity) { state.entities = state.entities.filter(e => e.id !== entity.id).concat(structuredClone(entity)) },
    }
    const bus = {
      on(topic, handler) { handlers.set(topic, handler); return () => handlers.delete(topic) },
      async emit(topic, payload) { state.events.push(topic); handlers.get(topic)?.({ payload }) },
    }
    const api = {}
    new Function('require', 'exports', 'window', 'localStorage', compiled)(name => {
      if (name === './commands') return { qs: { core } }
      if (name === './bus') return bus
      throw new Error(`Unexpected import: ${name}`)
    }, api, native ? { __TAURI__: {} } : {}, localStorage)
    return api
  }
  return { state, storage, session, api: session() }
}
const ids = async api => api.sortWorkspaces(await api.listWorkspaces()).map(w => w.id)

test('manual order persists across sessions and does not rewrite workspace metadata', async () => {
  const f = fixture()
  const original = structuredClone(f.state.entities)
  await f.api.reorderWorkspace('c', 'a')
  assert.deepEqual(await ids(f.api), ['c', 'a', 'b'])
  assert.deepEqual(await ids(f.session()), ['c', 'a', 'b'])
  assert.deepEqual(f.state.entities, original)
  assert.equal(f.state.settings['workspace.active'], undefined)
})

test('recency updates keep manual positions and newly registered workspaces append', async () => {
  const f = fixture()
  await f.api.reorderWorkspace('a', null)
  const a = (await f.api.listWorkspaces()).find(w => w.id === 'a')
  await f.api.openWorkspace(a)
  f.state.entities.push({ id: 'd', title: 'D', subtitle: 'C:/fixtures/d', updated_at: Date.now(), payload: {} })
  assert.deepEqual(await ids(f.api), ['b', 'c', 'a', 'd'])
})

test('pinned groups remain first and can be reordered without changing pins', async () => {
  const f = fixture()
  f.state.entities[0].payload.pinned = true
  f.state.entities[2].payload.pinned = true
  await f.api.reorderWorkspace('c', 'a')
  assert.deepEqual(await ids(f.api), ['c', 'a', 'b'])
  await assert.rejects(f.api.reorderWorkspace('b', 'c'), /pinned/)
  await f.api.setWorkspacePinned('b', true)
  assert.deepEqual(await ids(f.api), ['c', 'a', 'b'])
})

test('stale IDs and duplicate saved ranks cannot hide or duplicate registry entries', async () => {
  const f = fixture()
  f.state.settings['workspace.order'] = ['gone', 'c', 'c', null, 1, '', 'b']
  assert.deepEqual(await ids(f.api), ['c', 'b', 'a'])
  await f.api.reorderWorkspace('a', 'c')
  assert.deepEqual(f.state.settings['workspace.order'], ['a', 'c', 'b'])
  await assert.rejects(f.api.reorderWorkspace('missing', null), /registered/)
  await assert.rejects(f.api.reorderWorkspace('a', 'removed'), /group/)
  assert.deepEqual(await ids(f.api), ['a', 'c', 'b'])
})

test('failed saves preserve the order and do not announce success', async () => {
  const f = fixture()
  f.state.failWrite = true
  await assert.rejects(f.api.reorderWorkspace('c', 'a'), /write failed/)
  assert.deepEqual(await ids(f.api), ['a', 'b', 'c'])
  assert.deepEqual(f.state.events, [])
  f.state.failRead = true
  assert.deepEqual(await ids(f.api), ['a', 'b', 'c'], 'preference read failure does not hide registered workspaces')
  await assert.rejects(f.api.reorderWorkspace('c', 'a'), /read failed/)
})

test('reordering announces a registry refresh without opening a workspace; no-op moves do not write', async () => {
  const f = fixture()
  let refreshes = 0
  f.api.onWorkspacesChanged(() => refreshes++)
  await f.api.reorderWorkspace('c', 'a')
  assert.deepEqual(f.state.events, ['core.workspace.reordered'])
  assert.equal(refreshes, 1)
  await f.api.reorderWorkspace('c', 'a')
  await f.api.reorderWorkspace('a', 'a')
  assert.equal(refreshes, 1)
})

test('browser development storage preserves order alongside existing workspace state', async () => {
  const f = fixture(false)
  await f.api.reorderWorkspace('b', 'a')
  assert.deepEqual(await ids(f.session()), ['b', 'a', 'c'])
  const saved = JSON.parse(f.storage.get('qss-workspaces-dev'))
  assert.deepEqual(saved.list, rows())
  assert.equal(saved.active, null)
  assert.equal(saved.reopenLast, false)
})
