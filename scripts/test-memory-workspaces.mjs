import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import { createPinia, setActivePinia, defineStore } from 'pinia'
import { ref, computed } from 'vue'
import ts from 'typescript'
import { memoryLibraryRows, memoryPreview, parseStarred } from '../modules/memory/app/utils/library.ts'
import { scopeColor, graphNeighborhood, findGraphNodes, stepGraphZoom, DEFAULT_GRAPH_ZOOM } from '../modules/memory/app/utils/graph.ts'
import * as selection from '../packages/core/src/kanbanSelection.ts'
import { qualityFromDraft } from '../modules/memory/app/utils/quality.ts'

test('memory provenance handles numeric v-model values without implicit trust', () => {
  const draft = { basis: 'observed', confidence: 0.9, sources: ' CI run 42\nCI run 42\n', conflicts: '', replacement: '', lastVerified: null }
  assert.deepEqual(qualityFromDraft(draft), { basis: 'observed', confidence: 0.9, sources: ['CI run 42'], conflictsWith: [], supersededBy: null, lastVerified: null, reviewed: false })
  assert.equal(qualityFromDraft({ ...draft, confidence: '' }).confidence, null)
  assert.equal(qualityFromDraft({ ...draft, confidence: 0 }).confidence, 0)
  assert.throws(() => qualityFromDraft({ ...draft, confidence: 'not-a-number' }), /Confidence/)
  assert.throws(() => qualityFromDraft({ ...draft, confidence: 1.1 }), /Confidence/)
  assert.throws(() => qualityFromDraft({ ...draft, sources: '' }), /source/)
})

function load(path, dependencies, globals = {}) {
  const source = readFileSync(new URL(path, import.meta.url), 'utf8').replaceAll('import.meta.client', 'true')
  const js = ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS })
  const exports = {}
  new Function('require', 'exports', ...Object.keys(globals), js)(name => {
    if (!(name in dependencies)) throw new Error(`Unexpected import: ${name}`)
    return dependencies[name]
  }, exports, ...Object.values(globals))
  return exports
}
const storage = new Map()
globalThis.localStorage = { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) }
globalThis.window = { __TAURI_INTERNALS__: {}, addEventListener() {}, removeEventListener() {} }
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b }); return { promise, resolve, reject } }
const tick = () => new Promise(resolve => setImmediate(resolve))

test('graph zoom starts at 100 percent and advances in exact five-point steps', () => {
  assert.equal(DEFAULT_GRAPH_ZOOM, 1)
  assert.equal(stepGraphZoom(1, 1), 1.05)
  assert.equal(stepGraphZoom(1.05, 1), 1.1)
  assert.equal(stepGraphZoom(1.1, -1), 1.05)
  assert.equal(stepGraphZoom(1.05, -1), 1)
  assert.equal(stepGraphZoom(1, -1), 0.95)
  assert.equal(stepGraphZoom(0.15, -1), 0.15)
  assert.equal(stepGraphZoom(4, 1), 4)
  let zoom = 1
  for (let i = 0; i < 20; i++) zoom = stepGraphZoom(zoom, 1)
  assert.equal(zoom, 2)
  for (let i = 0; i < 20; i++) zoom = stepGraphZoom(zoom, -1)
  assert.equal(zoom, 1)
})

test('map and docked sidebar share selection, connections and navigation requests', () => {
  const store = vault()
  store.graph = { nodes: [
    { id: 'a', scope: 'general', title: 'Alpha', tags: [], kind: 'reference', incoming: 0, outgoing: 1 },
    { id: 'b', scope: 'general', title: 'Beta', tags: [], kind: 'decision', incoming: 1, outgoing: 0 },
  ], edges: [{ source: 'a', target: 'b', count: 1 }] }
  store.memories = [meta('a')]
  const { useMemoryGraphStore } = load('../modules/memory/app/stores/graph.ts', { pinia: { defineStore }, vue: { computed, ref }, '#memory/stores/vault': { useVaultStore: () => store }, '#memory/utils/graph': { scopeColor, graphNeighborhood } })
  const graph = useMemoryGraphStore()
  graph.kind = 'decision'
  graph.inspect('a')
  assert.equal(graph.selected.title, 'Alpha')
  assert.equal(graph.selectedMeta.id, 'a')
  assert.equal(graph.kind, null)
  assert.equal(graph.focusRequest, 1)
  assert.deepEqual(graph.connected.map(n => n.id), ['b'])
  graph.connectionsOnly = true
  graph.inspect('b')
  assert.equal(graph.connectionsOnly, false)
  assert.equal(graph.focusRequest, 2)
  graph.focusScope('general')
  assert.equal(graph.scopeToFocus, 'general')
  assert.equal(graph.scopeFocusRequest, 1)
  graph.inspect(null)
  assert.equal(graph.selected, null)
  assert.deepEqual(graph.connected, [])
})

test('memory previews remove markdown framing and keep readable content bounded', () => {
  assert.equal(memoryPreview('---\ntitle: Hidden\n---\n# Note\n\nA **useful** [[Other|connection]] with [evidence](https://example.com).\n```js\nsecret code\n```'), 'A useful connection with evidence.')
  assert.equal(memoryPreview('A [title].\n\nReadable content.', 'A [title].'), 'Readable content.')
  assert.equal(memoryPreview('x'.repeat(400)).length, 280)
  assert.equal(memoryPreview(''), '')
})

test('starred memories survive reload and malformed stored values are ignored', () => {
  assert.deepEqual(parseStarred('["a", "a", 1, null, "", "b"]'), ['a', 'b'])
  for (const value of ['not json', '{}', 'null', null]) assert.deepEqual(parseStarred(value), [])
  setActivePinia(createPinia())
  storage.delete('qm-starred')
  const { useAppStore } = load('../modules/memory/app/stores/app.ts', { pinia: { defineStore }, '#memory/utils/library': { parseStarred } })
  const app = useAppStore()
  app.toggleStar('memory-a'); app.toggleStar('memory-b'); app.toggleStar('memory-a')
  assert.deepEqual(parseStarred(storage.get('qm-starred')), ['memory-b'])
  app.setLibraryLayout('list')
  setActivePinia(createPinia())
  assert.deepEqual([...useAppStore().starred], ['memory-b'])
  assert.equal(useAppStore().libraryLayout, 'list')
})

test('graph neighborhoods use real bidirectional edges and exclude dangling endpoints', () => {
  const nodes = ['a', 'b', 'c', 'd'].map(id => ({ id }))
  const edges = [{ source: 'a', target: 'b' }, { source: 'c', target: 'a' }, { source: 'b', target: 'd' }, { source: 'a', target: 'missing' }]
  assert.deepEqual([...graphNeighborhood(nodes, edges, 'a')].sort(), ['a', 'b', 'c'])
  assert.deepEqual([...graphNeighborhood(nodes, edges, 'unknown')], [])
})

test('graph search matches titles, types and all query terms without inventing nodes', () => {
  const nodes = [
    { id: 'a', title: 'Search strategy', kind: 'decision', tags: ['retrieval'], incoming: 9, outgoing: 2 },
    { id: 'b', title: 'Search', kind: 'reference', tags: ['retrieval'], incoming: 0, outgoing: 0 },
  ]
  assert.deepEqual(findGraphNodes(nodes, ' SEARCH ').map(n => n.id), ['b', 'a'])
  assert.deepEqual(findGraphNodes(nodes, 'retrieval decision').map(n => n.id), ['a'])
  assert.deepEqual(findGraphNodes(nodes, 'unknown'), [])
  assert.deepEqual(findGraphNodes(nodes, '  '), [])
  assert.match(scopeColor('general'), /^#[0-9a-f]{6}$/)
  assert.equal(scopeColor('core:workspace:a'), scopeColor('core:workspace:a'))
  assert.notEqual(scopeColor('general'), scopeColor('core:workspace:a'))
})

test('graph loads reject stale results, clear on scope change and expose retryable errors', async () => {
  const slow = deferred()
  let requests = 0
  const store = vault(cmd => cmd.endsWith('get_graph') ? (++requests === 1 ? slow.promise : { nodes: [{ id: 'fresh' }], edges: [] }) : [])
  const first = store.loadGraph()
  await store.loadGraph()
  slow.resolve({ nodes: [{ id: 'stale' }], edges: [] }); await first
  assert.equal(store.graph.nodes[0].id, 'fresh')
  assert.equal(store.graphLoading, false)
  store.setScope('core:workspace:a')
  assert.equal(store.graph, null)
  const failure = vault(() => { throw new Error('graph offline') })
  await failure.loadGraph()
  assert.match(failure.graphError, /graph offline/)
  assert.equal(failure.error, null)
  assert.equal(failure.graphLoading, false)
})

const meta = (id, scope = 'general', extra = {}) => ({ id, scope, title: id, slug: id, relPath: `${id}.md`, tags: [], kind: 'reference', author: 'operator', wordCount: 10, incomingLinks: 0, outgoingLinks: 0, createdAt: '2026-01-01', updatedAt: '2026-01-01', managed: false, ...extra })
function vault(handler = () => null) {
  storage.delete('qm-scope')
  setActivePinia(createPinia())
  return load('../modules/memory/app/stores/vault.ts', { pinia: { defineStore }, '#memory/utils/invoke': { getInvoke: async () => handler } }).useVaultStore()
}

test('Kanban selection is shared, persisted and does not echo or accept folder paths', () => {
  const received = []
  const off = selection.onKanbanBoardSelected(id => received.push(id))
  selection.selectKanbanBoard('core:workspace:a')
  selection.selectKanbanBoard('core:workspace:a')
  selection.selectKanbanBoard('C:/unrelated')
  assert.deepEqual(received, ['core:workspace:a'])
  assert.equal(selection.getSelectedKanbanBoard(), 'core:workspace:a')
  assert.equal(storage.get('qss-kanban-board'), 'core:workspace:a')
  assert.equal(storage.has('workspace.active'), false)
  off(); selection.selectKanbanBoard('general')
  assert.equal(received.length, 1)
})

test('library filters combine scope, type and tag without mutating the source', () => {
  const memories = [meta('Zulu', 'core:workspace:a', { tags: ['design'] }), meta('Alpha', 'general', { kind: 'decision', tags: ['design'] }), meta('Beta', 'core:workspace:b')]
  const base = { scope: 'general', tag: null, kind: null, unlinked: false, searching: false, hits: [], sort: 'title' }
  assert.deepEqual(memoryLibraryRows(memories, base).map(x => x.meta.id), ['Alpha', 'Beta', 'Zulu'])
  assert.deepEqual(memoryLibraryRows(memories, { ...base, scope: 'core:workspace:a', tag: 'design', kind: 'reference' }).map(x => x.meta.id), ['Zulu'])
  assert.equal(memories[0].id, 'Zulu')
})

test('search keeps backend ranking and snippets, and excludes stale or foreign hits', () => {
  const memories = [meta('a'), meta('b'), meta('foreign', 'core:workspace:b')]
  const options = { scope: 'general', tag: null, kind: null, unlinked: false, searching: true, sort: 'relevance', hits: [{ id: 'b', snippet: 'matched body' }, { id: 'gone' }, { id: 'a', snippet: 'a' }] }
  const rows = memoryLibraryRows(memories, options)
  assert.deepEqual(rows.map(x => x.meta.id), ['b', 'a'])
  assert.equal(rows[0].snippet, 'matched body')
  assert.equal(memoryLibraryRows(memories, { ...options, scope: 'core:workspace:b' }).length, 0)
})

test('unlinked means no incoming or outgoing links; most-connected sorting works', () => {
  const options = { scope: 'general', tag: null, kind: null, unlinked: true, searching: false, hits: [], sort: 'links' }
  const memories = [meta('a', 'general', { incomingLinks: 2 }), meta('b'), meta('c', 'general', { outgoingLinks: 5 })]
  assert.deepEqual(memoryLibraryRows(memories, options).map(x => x.meta.id), ['b'])
  assert.deepEqual(memoryLibraryRows(memories, { ...options, unlinked: false }).map(x => x.meta.id), ['c', 'a', 'b'])
})

test('scope changes re-run search and repair data, clearing invalid filters immediately', async () => {
  const calls = []
  const store = vault((cmd, args) => { calls.push({ cmd, args }); return cmd.endsWith('get_stats') ? { memories: 2 } : [] })
  store.query = 'architecture'; store.filterTag = 'old'; store.filterKind = 'old'
  store.setScope('core:workspace:a')
  assert.equal(store.filterTag, null); assert.equal(store.filterKind, null)
  await tick()
  for (const command of ['get_stats', 'get_orphans', 'get_unresolved_links', 'search_memories']) {
    assert.equal(calls.find(c => c.cmd.endsWith(command)).args.scope, 'core:workspace:a')
  }
})

test('slow scope statistics and broken-link responses cannot overwrite the new scope', async () => {
  const slow = deferred()
  const store = vault((cmd, args) => args.scope === 'all' ? slow.promise : cmd.endsWith('get_stats') ? { memories: 7 } : [{ target: 'new' }])
  const first = Promise.all([store.loadView(), store.loadHygiene()])
  store.setScope('core:workspace:a'); await tick()
  slow.resolve([{ target: 'stale' }]); await first
  assert.equal(store.stats.memories, 7)
  assert.equal(store.unresolved[0].target, 'new')
})

test('newest query wins even when an old request finishes last', async () => {
  const slow = deferred()
  const store = vault((_, args) => args.query === 'old' ? slow.promise : [{ id: 'fresh' }])
  const first = store.search('old'); await store.search('new')
  slow.resolve([{ id: 'stale' }]); await first
  assert.equal(store.hits[0].id, 'fresh'); assert.equal(store.searching, false)
})

test('clearing search invalidates pending hits and failures are visible', async () => {
  const slow = deferred()
  const store = vault(() => slow.promise)
  const first = store.search('old'); await store.search('')
  slow.resolve([{ id: 'old' }]); await first
  assert.deepEqual(store.hits, [])
  const failed = vault(() => { throw new Error('index offline') })
  await failed.search('term')
  assert.match(failed.searchError, /index offline/); assert.equal(failed.searching, false)
})

test('type choices remain available after applying a type filter', () => {
  const store = vault(); store.memories = [meta('a'), meta('b', 'general', { kind: 'decision' })]
  store.filterKind = 'reference'
  assert.deepEqual(store.kinds, ['decision', 'reference'])
})

test('new memories use the explicit creation scope and include content and metadata', async () => {
  let created
  const store = vault((cmd, args) => {
    if (cmd.endsWith('create_memory')) { created = args; return meta('created', args.scope) }
    return cmd.endsWith('list_memories') || cmd.endsWith('list_scopes') ? [] : null
  })
  const result = await store.create({ title: 'Decision', body: 'Use SQLite', scope: 'core:workspace:b', kind: 'decision', tags: ['database'] })
  assert.equal(created.scope, 'core:workspace:b'); assert.equal(created.body, 'Use SQLite')
  assert.equal(created.kind, 'decision'); assert.deepEqual(created.tags, ['database'])
  assert.equal(result.scope, 'core:workspace:b')
})

test('a failed refresh keeps the library and reports the error', async () => {
  const store = vault(() => { throw new Error('vault offline') }); store.memories = [meta('keep')]
  await store.loadAll()
  assert.equal(store.memories[0].id, 'keep'); assert.match(store.error, /vault offline/)
  assert.equal(store.loading, false)
})

test('the newest opened document wins over late responses', async () => {
  const slow = deferred()
  const store = vault((cmd, args) => cmd.endsWith('suggest_connections') ? [] : args.identifier === 'old' ? slow.promise : { meta: meta('new'), body: 'new' })
  const first = store.open('old'); await store.open('new')
  slow.resolve({ meta: meta('old'), body: 'old' }); await first
  assert.equal(store.activeDoc.meta.id, 'new')
})

function mcp(handler) {
  const state = new Map()
  const core = { ...selection, GENERAL_BOARD_ID: 'general', KANBAN_COLUMNS: [], sortWorkspaces: x => x,
    listWorkspaces: async () => [{ id: 'core:workspace:a', path: 'C:/a' }, { id: 'core:workspace:b', path: 'C:/b' }],
    getActiveWorkspace: async () => ({ path: 'C:/a' }), samePath: (a, b) => a === b,
    listArchivedKanbanCards: id => handler('archive', { workspaceId: id }),
    onWorkspacesChanged: () => () => {}, onKanbanChanged: () => () => {},
  }
  return load('../modules/mcp/app/composables/useMcpWorkspaces.ts', { '@quantsuite/core': core, '@tauri-apps/api/core': { invoke: handler } }, {
    computed, useState: (key, init) => { if (!state.has(key)) state.set(key, ref(init())); return state.get(key) },
  }).useMcpWorkspaces()
}

test('MCP follows drawer selection and publishes its own selection back', async () => {
  selection.selectKanbanBoard('core:workspace:a')
  const store = mcp((_, { workspaceId }) => [{ id: workspaceId }])
  const off = store.subscribe(); await store.refresh()
  selection.selectKanbanBoard('core:workspace:b'); await tick()
  assert.equal(store.selectedWorkspaceId.value, 'core:workspace:b')
  assert.equal(store.cards.value[0].id, 'core:workspace:b')
  await store.selectWorkspace('general')
  assert.equal(selection.getSelectedKanbanBoard(), 'general')
  off()
})

test('late MCP board and archive loads cannot mix cards between workspaces', async () => {
  const slow = deferred()
  const store = mcp((_, { workspaceId }) => workspaceId === 'core:workspace:a' ? slow.promise : [{ id: 'new' }])
  const first = store.selectWorkspace('core:workspace:a')
  await store.selectWorkspace('core:workspace:b'); slow.resolve([{ id: 'stale' }]); await first
  assert.equal(store.cards.value[0].id, 'new'); assert.equal(store.archivedCards.value[0].id, 'new')
})
