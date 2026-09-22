import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { parse, compileScript } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as Vue from 'vue'

// Compile the real card dialog and core IPC client. Only native IPC and DOM
// hosting are replaced, so both board surfaces exercise the same workflow.
function load(path, dependencies) {
  let source = readFileSync(new URL(path, import.meta.url), 'utf8')
  if (path.endsWith('.vue')) {
    source = source.replaceAll('<Teleport to="body">', '<div>').replaceAll('</Teleport>', '</div>')
      .replace(/<Transition[^>]*>/g, '<div>').replaceAll('</Transition>', '</div>')
    source = compileScript(parse(source).descriptor, { id: path, inlineTemplate: true }).content
  }
  const js = ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS })
  const exports = {}
  new Function('require', 'exports', js)(name => {
    if (!(name in dependencies)) throw new Error(`Unexpected import: ${name}`)
    return dependencies[name]
  }, exports)
  return exports
}

const element = type => ({ type, children: [], parent: null, style: {},
  focus() {}, setSelectionRange() {}, addEventListener() {}, removeEventListener() {}, setAttribute() {},
  getRootNode() { return globalThis.document },
})
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
  remove(node) { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1) },
})
const find = (node, predicate) => predicate(node) ? node : node.children.map(child => find(child, predicate)).find(Boolean)
const button = (node, label) => find(node, n => n.type === 'button' && n.text?.trim() === label)
const tick = async () => { await new Promise(resolve => setImmediate(resolve)); await Vue.nextTick() }
const card = extra => ({ id: 'card-1', workspace_id: 'ws', title: 'Requirement', description: 'Acceptance criteria',
  column: 'plan', status: 'backlog', order: 1, priority: 'medium', blocked_by: [], created_at: 1, updated_at: 1, ...extra })

async function setup(t, props = {}, invokeResult = async () => 'OK') {
  globalThis.Document = class Document {}
  globalThis.ShadowRoot = class ShadowRoot {}
  globalThis.window = { addEventListener() {}, removeEventListener() {} }
  globalThis.document = { body: { style: {} }, addEventListener() {}, removeEventListener() {} }
  globalThis.localStorage = { getItem() { return null }, setItem() {} }
  const calls = []
  const invoke = async (cmd, args) => { calls.push({ cmd, args }); return invokeResult(cmd, args) }
  const core = load('../packages/core/src/kanban.ts', { './bus': {}, '@tauri-apps/api/core': { invoke } })
  const Modal = load('../packages/ui/src/components/QKanbanCardModal.vue', { vue: Vue, '@quantsuite/core': core }).default
  const state = Vue.reactive({ modelValue: true, approvalMode: 'approval', card: card(), ...props })
  const root = element('root')
  const app = renderer.createApp({ render: () => Vue.h(Modal, { ...state, 'onUpdate:modelValue': value => { state.modelValue = value } }) })
  app.component('QMarkdownPreview', { render: () => Vue.h('preview') })
  app.mount(root)
  t.after(() => app.unmount())
  await tick()
  return { root, state, calls }
}

test('a saved Plan card offers work approval and calls only the native review command', async t => {
  const s = await setup(t)
  assert.equal(button(s.root, 'Approve work').disabled, false)
  assert.equal(button(s.root, 'Approve result'), undefined)
  await button(s.root, 'Approve work').onClick()
  assert.deepEqual(s.calls, [{ cmd: 'plugin:mcp|review_kanban_card', args: { id: 'card-1', action: 'start', reason: '' } }])
  assert.equal(s.state.modelValue, false)
})

test('Auto Apply, General and an already approved plan offer no work approval', async t => {
  for (const props of [{ approvalMode: 'auto_apply' }, { card: card({ workspace_id: 'general' }) }, { card: card({ start_approved_at: 10 }) }]) {
    const s = await setup(t, props)
    assert.equal(button(s.root, 'Approve work'), undefined)
    assert.equal(s.calls.length, 0)
  }
})

test('unsaved scope changes prevent approval and a pending call cannot be submitted twice', async t => {
  let finish
  const pending = new Promise(resolve => { finish = resolve })
  const s = await setup(t, {}, () => pending)
  const title = find(s.root, n => n.type === 'input')
  title['onUpdate:modelValue']('Different scope')
  await tick()
  assert.equal(button(s.root, 'Approve work').disabled, true)
  await button(s.root, 'Approve work').onClick()
  assert.equal(s.calls.length, 0)
  title['onUpdate:modelValue']('Requirement')
  await tick()
  const approval = button(s.root, 'Approve work').onClick()
  await tick()
  await button(s.root, 'Approve work').onClick()
  assert.equal(s.calls.length, 1)
  finish('OK')
  await approval
})

test('Review exposes separate result approval even on a read-only claimed card', async t => {
  const s = await setup(t, { readonly: true, card: card({ column: 'review', status: 'awaiting_review', agent_id: 'agent' }) })
  assert.equal(button(s.root, 'Approve work'), undefined)
  await button(s.root, 'Approve result').onClick()
  assert.equal(s.calls[0].args.action, 'approve')
  assert.equal(s.state.modelValue, false)
})

test('a rejected result includes the requested changes; errors preserve the dialog', async t => {
  const s = await setup(t, { readonly: true, card: card({ column: 'review', status: 'awaiting_review', agent_id: 'agent' }) }, async () => { throw new Error('Merge conflict') })
  await button(s.root, 'Approve result').onClick()
  await tick()
  assert.equal(s.state.modelValue, true)
  assert.match(find(s.root, n => n.role === 'alert').text, /Merge conflict/)
  await button(s.root, 'Request changes').onClick()
  await tick()
  assert.equal(button(s.root, 'Send changes').disabled, true)
  find(s.root, n => n.type === 'textarea')['onUpdate:modelValue']('Fix the empty state')
  await tick()
  assert.equal(button(s.root, 'Send changes').disabled, false)
  await button(s.root, 'Send changes').onClick()
  assert.deepEqual(s.calls.at(-1).args, { id: 'card-1', action: 'reject', reason: 'Fix the empty state' })
})
