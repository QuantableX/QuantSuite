import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { parse, compileScript } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as Vue from 'vue'

// Mount the actual Canvas wrapper and shared board. Only the native IPC,
// filesystem explorer and modal boundary are replaced; MCP uses its real client.
function load(path, dependencies, globals = {}) {
  let source = readFileSync(new URL(path, import.meta.url), 'utf8')
  if (path.endsWith('.vue')) {
    source = compileScript(parse(source).descriptor, { id: path, inlineTemplate: true }).content
  }
  const js = ts.transpile(source.replaceAll('import.meta.client', 'true'), {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
  })
  const exports = {}
  new Function('require', 'exports', ...Object.keys(globals), js)(name => {
    if (!(name in dependencies)) throw new Error(`Unexpected import: ${name}`)
    return dependencies[name]
  }, exports, ...Object.values(globals))
  return exports
}

function element(type) {
  return {
    type, children: [], parent: null, style: {},
    contains(child) { for (; child; child = child.parent) if (child === this) return true; return false },
    closest(selector) {
      const key = selector.slice(1, -1)
      for (let node = this; node; node = node.parent) if (node[key]) return node
      return null
    },
    get dataset() { return { dkbColumn: this['data-dkb-column'], dkbCardId: this['data-dkb-card-id'] } },
  }
}
const renderer = Vue.createRenderer({
  createElement: element,
  createText: text => ({ ...element('text'), text }),
  createComment: text => ({ ...element('comment'), text }),
  setText: (node, text) => { node.text = text },
  setElementText: (node, text) => { node.text = text },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  patchProp: (node, key, before, after) => { node[key] = after },
  insert(node, parent, anchor = null) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    const index = anchor ? parent.children.indexOf(anchor) : -1
    parent.children.splice(index < 0 ? parent.children.length : index, 0, node)
    node.parent = parent
  },
  remove(node) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = null
  },
})
const find = (node, predicate) => predicate(node) ? node : node.children.map(child => find(child, predicate)).find(Boolean)
const all = (node, predicate) => [...(predicate(node) ? [node] : []), ...node.children.flatMap(child => all(child, predicate))]
const tick = async () => { await new Promise(resolve => setImmediate(resolve)); await Vue.nextTick() }
const A = 'core:workspace:a'
const B = 'core:workspace:b'
const draft = (title, column = 'plan') => ({ title, description: 'Shared description', column, priority: 'high' })

function setup(t) {
  const handlers = new Map()
  const browserHandlers = new Map()
  const storage = new Map()
  let hit = null
  globalThis.document = { elementFromPoint: () => hit }
  globalThis.window = {
    __TAURI_INTERNALS__: {},
    addEventListener(name, fn) { if (!browserHandlers.has(name)) browserHandlers.set(name, new Set()); browserHandlers.get(name).add(fn) },
    removeEventListener(name, fn) { browserHandlers.get(name)?.delete(fn) },
    dispatchEvent(event) { browserHandlers.get(event.type)?.forEach(fn => fn(event)) },
  }
  globalThis.localStorage = { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) }
  const on = (topic, fn) => {
    if (!handlers.has(topic)) handlers.set(topic, new Set())
    handlers.get(topic).add(fn)
    return () => handlers.get(topic).delete(fn)
  }
  const emit = (topic, payload) => handlers.get(topic)?.forEach(fn => fn({ payload }))
  const records = new Map()
  const calls = []
  let sequence = 0
  let delayedList = null
  const invoke = async (cmd, args) => {
    calls.push({ cmd, args })
    if (cmd === 'plugin:mcp|list_kanban_cards') {
      if (delayedList) { const deferred = delayedList; delayedList = null; return deferred }
      return structuredClone([...records.values()].filter(c => c.workspace_id === args.workspaceId && !c.archived_at))
    }
    if (cmd === 'plugin:mcp|list_archived_kanban_cards') return structuredClone([...records.values()].filter(c => c.workspace_id === args.workspaceId && c.archived_at))
    if (cmd === 'plugin:mcp|get_approval_mode') return 'auto_apply'
    let card = records.get(args.id)
    if (cmd === 'plugin:mcp|add_kanban_card') {
      card = { id: `card-${++sequence}`, workspace_id: args.workspaceId, title: args.title, description: args.description, column: args.column, priority: args.priority, status: 'backlog', order: sequence, created_at: 1, updated_at: 1, blocked_by: [] }
      records.set(card.id, card)
    } else if (cmd === 'plugin:mcp|update_kanban_card') Object.assign(card, { title: args.title, description: args.description, priority: args.priority })
    else if (cmd === 'plugin:mcp|move_kanban_card') card.column = args.column
    else if (cmd === 'plugin:mcp|archive_kanban_card') card.archived_at = 1
    else if (cmd === 'plugin:mcp|delete_kanban_card') {
      records.delete(args.id)
      emit('core.entity.deleted', { id: `core:kanban.card:mcp-${args.id}` })
      return
    } else throw new Error(`Unexpected command: ${cmd}`)
    emit('core.entity.upserted', { kind: 'kanban.card' })
    return structuredClone(card)
  }
  const selection = load('../packages/core/src/kanbanSelection.ts', {})
  const kanban = load('../packages/core/src/kanban.ts', { './bus': { on }, '@tauri-apps/api/core': { invoke } })
  const workspaces = [{ id: A, name: 'Project A', path: 'C:/A' }, { id: B, name: 'Project B', path: 'C:/B' }]
  const core = {
    ...selection, ...kanban,
    listWorkspaces: async () => workspaces,
    sortWorkspaces: rows => rows,
    getActiveWorkspace: async () => workspaces[0],
    workspaceIdFor: path => workspaces.find(w => w.path === path).id,
    onWorkspacesChanged: cb => on('workspace', cb),
  }
  const canvasStore = Vue.reactive({ contentWorkspaceId: A, GENERAL_CONTENT: '__general__', contentWorkspace: workspaces[0] })
  const Board = load('../packages/ui/src/components/QDrawerKanban.vue', { vue: Vue, '@quantsuite/core': core }).default
  const Canvas = load('../modules/canvas/app/components/Windows/KanbanWindow.vue', {
    vue: Vue, '@quantsuite/core': core, '../../../stores/workspaces': { useWorkspacesStore: () => canvasStore },
  }).default
  const mcp = load('../modules/mcp/app/composables/useMcpWorkspaces.ts', { '@quantsuite/core': core, '@tauri-apps/api/core': { invoke } }, {
    computed: Vue.computed, useState: (_, init) => Vue.ref(init()),
  }).useMcpWorkspaces()
  const unsubscribe = mcp.subscribe()
  t.after(unsubscribe)

  function mount(component, props = {}) {
    const root = element('root')
    const app = renderer.createApp({ render: () => Vue.h(component, props) })
    app.component('QDrawerKanban', Board)
    app.component('QKanbanCardModal', { inheritAttrs: false, setup: (_, { attrs }) => () => Vue.h('card-editor', attrs) })
    app.component('CanvasWindowsSpecWindow', { render: () => Vue.h('legacy-specs') })
    app.mount(root)
    t.after(() => app.unmount())
    return root
  }
  const canvas = mount(Canvas, { window: { id: 'restored', type: 'spec' } })
  const drawer = mount(Board)
  return { canvas, drawer, canvasStore, mcp, core, records, calls, invoke, browserHandlers,
    hit: node => { hit = node }, delayList: promise => { delayedList = promise },
  }
}
const editor = root => find(root, n => n.type === 'card-editor')
const titles = root => all(root, n => n.class === 'dkb-card-title').map(n => n.text)
const button = (root, label) => find(root, n => n.type === 'button' && n.text === label)

test('Canvas, drawer and MCP add, edit, move, archive and delete the same cards', async t => {
  const s = setup(t)
  await s.mcp.selectWorkspace(A)
  await tick()
  await editor(s.canvas).onSave(draft('From Canvas'), null)
  await tick()
  const id = s.mcp.cards.value[0].id
  assert.deepEqual(titles(s.drawer), ['From Canvas'])
  assert.equal(s.records.get(id).workspace_id, A)

  await s.mcp.updateCard(id, 'Edited in MCP', 'Full spec', 'critical')
  await s.mcp.moveCard(id, 'work')
  await tick()
  assert.deepEqual(titles(s.canvas), ['Edited in MCP'])
  const work = find(s.canvas, n => n['data-dkb-column'] === 'work')
  assert.deepEqual(titles(work), ['Edited in MCP'])

  await editor(s.drawer).onSave(draft('Done in drawer', 'done'), structuredClone(s.records.get(id)))
  await tick()
  assert.equal(s.mcp.cards.value[0].column, 'done')
  assert.deepEqual(titles(s.canvas), ['Done in drawer'])
  await editor(s.canvas).onArchive(structuredClone(s.records.get(id)))
  await tick()
  assert.deepEqual(titles(s.drawer), [])
  assert.equal(s.mcp.archivedCards.value[0].id, id)

  const added = await s.mcp.addCard(A, 'Delete me', '', 'plan')
  await tick()
  await editor(s.canvas).onDelete(structuredClone(added))
  await tick()
  assert.deepEqual(s.mcp.cards.value, [])
  assert.deepEqual(titles(s.drawer), [])
})

test('Canvas stays on its own workspace, maps General correctly and opens that exact MCP board', async t => {
  const s = setup(t)
  await s.mcp.selectWorkspace(B)
  await tick()
  await editor(s.canvas).onSave(draft('Canvas A'), null)
  await tick()
  assert.deepEqual(titles(s.drawer), [])
  assert.deepEqual(titles(s.canvas), ['Canvas A'])
  assert.equal(s.core.getSelectedKanbanBoard(), B)

  const link = find(s.canvas, n => n.class === 'dkb-title dkb-mcp-link')
  link.onClick()
  await tick()
  assert.equal(s.mcp.selectedWorkspaceId.value, A)
  assert.deepEqual(titles(s.drawer), ['Canvas A'])

  s.canvasStore.contentWorkspaceId = '__general__'
  s.canvasStore.contentWorkspace = undefined
  await tick()
  assert.deepEqual(titles(s.canvas), [])
  await editor(s.canvas).onSave(draft('General task'), null)
  await tick()
  assert.equal([...s.records.values()].find(c => c.title === 'General task').workspace_id, 'general')
  assert.equal(s.core.getSelectedKanbanBoard(), A, 'Canvas does not change another surface’s selection')
  assert.equal(button(s.canvas, 'Legacy specs'), undefined)
})

test('workspace switches discard late responses and close a draft for the previous board', async t => {
  const s = setup(t)
  await tick()
  let resolve
  s.delayList(new Promise(done => { resolve = done }))
  s.canvasStore.contentWorkspaceId = B
  await tick()
  button(s.canvas, '+ Add card').onClick()
  await tick()
  assert.equal(editor(s.canvas).modelValue, true)
  s.canvasStore.contentWorkspaceId = A
  await tick()
  assert.equal(editor(s.canvas).modelValue, false)
  resolve([{ id: 'late', title: 'Wrong workspace', column: 'plan', order: 0 }])
  await tick()
  assert.deepEqual(titles(s.canvas), [])
})

test('legacy specs remain accessible without replacing the shared board or creating cards', async t => {
  const s = setup(t)
  await tick()
  button(s.canvas, 'Legacy specs').onClick()
  await tick()
  assert.ok(find(s.canvas, n => n.type === 'legacy-specs'))
  assert.equal(editor(s.canvas), undefined)
  button(s.canvas, 'Back to kanban').onClick()
  await tick()
  assert.ok(editor(s.canvas))
  assert.equal(s.records.size, 0)
  assert.ok(s.calls.every(c => !c.cmd.startsWith('plugin:canvas|')), 'the shared board never writes spec files')
})

test('dragging from Canvas cannot drop into a different board instance', async t => {
  const s = setup(t)
  await tick()
  await editor(s.canvas).onSave(draft('Keep in Plan'), null)
  await s.mcp.selectWorkspace(B)
  await tick()
  const card = find(s.canvas, n => n['data-dkb-card-id'])
  card.onPointerdown({ button: 0, clientX: 0, clientY: 0, target: { closest: () => null }, preventDefault() {} })
  s.hit(find(s.drawer, n => n['data-dkb-column'] === 'done'))
  s.browserHandlers.get('pointermove').forEach(fn => fn({ clientX: 100, clientY: 100 }))
  await Promise.all([...s.browserHandlers.get('pointerup')].map(fn => fn({ clientX: 100, clientY: 100 })))
  assert.equal([...s.records.values()][0].column, 'plan')
  assert.equal(s.calls.filter(c => c.cmd === 'plugin:mcp|move_kanban_card').length, 0)
})
