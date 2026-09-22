import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { parse, compileScript } from '@vue/compiler-sfc'

const root = fileURLToPath(new URL('../', import.meta.url))
const componentPath = `${root}packages/ui/src/components/QWindowControls.vue`
const { descriptor } = parse(readFileSync(componentPath, 'utf8'), { filename: componentPath })
const component = compileScript(descriptor, { id: 'window-controls', inlineTemplate: true }).content
const bundle = await build({
  stdin: { resolveDir: root, contents: `
    export { useAgentBroker } from './apps/shell/app/composables/useAgentBroker';
    export { isSuiteWindow } from './packages/core/src/windows';
    export { default as Controls } from 'test-controls';
    export { mock, bus } from 'test-mock';
    export { createRenderer, nextTick } from 'vue';
  ` },
  bundle: true, write: false, format: 'esm', platform: 'node',
  plugins: [{ name: 'suite-test', setup(b) {
    b.onResolve({ filter: /^(test-controls|test-mock|@quantsuite\/core|@tauri-apps\/api\/(core|window))$/ }, ({ path }) => ({ path, namespace: 'suite-test' }))
    b.onLoad({ filter: /.*/, namespace: 'suite-test' }, ({ path }) => ({
      resolveDir: root, loader: 'ts', contents:
        path === 'test-controls' ? component :
        path === '@tauri-apps/api/core' ? `import { mock } from 'test-mock'; export const invoke = (...args) => mock.invoke(...args);` :
        path === '@tauri-apps/api/window' ? `import { mock } from 'test-mock'; export const getCurrentWindow = () => mock.window;` :
        path === '@quantsuite/core' ? `
          export { bus } from 'test-mock';
          import { mock } from 'test-mock';
          export const isModuleEnabled = () => true;
          export const currentWindowLabel = () => mock.window.label;
          export const qs = { core: {
            windowNew: () => mock.open(),
            agentPendingCalls: () => mock.pending(),
            agentCallClaim: async (id) => {
              if (!mock.calls.has(id) || mock.claimed.has(id)) return false;
              mock.claimed.add(id); mock.bus.emit('agent.call.claimed', { callId: id }); return true;
            },
            agentCallComplete: async (id, result, error) => {
              mock.results.push({ id, result, error }); mock.calls.delete(id);
              mock.bus.emit('agent.call.completed', { callId: id });
            },
          } };` : `
          const listeners = new Map();
          export const bus = {
            on(topic, fn) { const set = listeners.get(topic) ?? new Set(); listeners.set(topic, set); set.add(fn); return () => set.delete(fn); },
            emit(topic, payload) { for (const fn of listeners.get(topic) ?? []) fn({ payload }); },
          };
          export const mock = {
            bus, calls: new Map(), claimed: new Set(), results: [], invoked: [], opens: 0,
            pending: async () => [...mock.calls.values()],
            invoke: async (...args) => { mock.invoked.push(args); return { saved: true }; },
            open: async () => { mock.opens++; return 'suite-new'; },
            window: { label: 'main', isMaximized: async () => false, onResized: async () => () => {},
              minimize: async () => {}, toggleMaximize: async () => {}, close: async () => {} },
          };
        `,
    }))
  } }],
})
let sequence = 0
async function fresh() {
  globalThis.useState = (_key, initial) => ({ value: initial() })
  globalThis.window = { __TAURI_INTERNALS__: {} }
  return import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text + `\n// ${sequence++}`).toString('base64')}`)
}
const tick = () => new Promise((resolve) => setImmediate(resolve))
function call(id, decision = 'prompt') {
  return { callId: id, decision, tool: 'quantsuite.notes.create_note', command: 'plugin:notes|create_note', args: {} }
}

test('suite windows keep the shell; HUD helper windows remain overlays', async () => {
  const m = await fresh()
  for (const label of ['main', 'suite-1', 'suite-abc']) assert.equal(m.isSuiteWindow(label), true)
  for (const label of ['hud', 'dual-right', 'region-selector', 'screenshot-preview']) assert.equal(m.isSuiteWindow(label), false)
})

test('three windows dispatch an allowed call only once, through the resident window', async () => {
  const m = await fresh()
  const windows = [m.useAgentBroker(true), m.useAgentBroker(false), m.useAgentBroker(false)]
  const pending = call('allowed', 'allow')
  m.mock.calls.set(pending.callId, pending)
  await Promise.all(windows.map((window) => window.start()))
  m.bus.emit('agent.call.requested', pending)
  await tick()
  assert.equal(m.mock.invoked.length, 1)
  assert.equal(m.mock.results.length, 1)
  assert.ok(windows.every((window) => window.queue.value.length === 0))
  windows.forEach((window) => window.stop())
})

test('approving in two windows cannot duplicate a write; both queues clear', async () => {
  const m = await fresh()
  const windows = [m.useAgentBroker(true), m.useAgentBroker(false)]
  await Promise.all(windows.map((window) => window.start()))
  const pending = call('prompted')
  m.mock.calls.set(pending.callId, pending)
  m.bus.emit('agent.call.requested', pending)
  assert.ok(windows.every((window) => window.queue.value.length === 1))
  await Promise.all(windows.map((window) => window.approve(pending)))
  assert.equal(m.mock.invoked.length, 1)
  assert.equal(m.mock.results.length, 1)
  assert.ok(windows.every((window) => window.queue.value.length === 0))
})

test('a rejection in a secondary window wins over a concurrent approval', async () => {
  const m = await fresh()
  const main = m.useAgentBroker(true), extra = m.useAgentBroker(false)
  await Promise.all([main.start(), extra.start()])
  const pending = call('rejected')
  m.mock.calls.set(pending.callId, pending)
  m.bus.emit('agent.call.requested', pending)
  await Promise.all([extra.reject(pending), main.approve(pending)])
  assert.equal(m.mock.invoked.length, 0)
  assert.equal(m.mock.results[0].error, 'rejected by the user')
  assert.equal(main.queue.value.length, 0)
})

test('a stale startup snapshot cannot resurrect a resolved prompt', async () => {
  const m = await fresh()
  let finish
  m.mock.pending = () => new Promise((resolve) => { finish = resolve })
  const extra = m.useAgentBroker(false)
  const starting = extra.start()
  m.bus.emit('agent.call.claimed', { callId: 'finished' })
  finish([call('finished')])
  await starting
  assert.equal(extra.queue.value.length, 0)
})

function mount(m) {
  const node = (type, text = '') => ({ type, text, props: {}, children: [] })
  const renderer = m.createRenderer({
    createElement: node, createText: (text) => node('#text', text), createComment: (text) => node('#comment', text),
    setText: (n, text) => { n.text = text }, setElementText: (n, text) => { n.text = text },
    patchProp: (n, key, _old, value) => { n.props[key] = value },
    insert(n, parent, anchor) { n.parent = parent; const index = parent.children.indexOf(anchor); parent.children.splice(index < 0 ? parent.children.length : index, 0, n) },
    remove(n) { const list = n.parent.children; list.splice(list.indexOf(n), 1) },
    parentNode: (n) => n.parent, nextSibling: () => null,
  })
  const root = node('root')
  const app = renderer.createApp(m.Controls)
  app.mount(root)
  return { root, app }
}
function find(root, predicate) {
  return [root, ...root.children.flatMap((child) => find(child, () => true))].filter(predicate)
}

test('New window is left of all three controls, guards repeat clicks and shows failures', async () => {
  const m = await fresh()
  const { root, app } = mount(m)
  await tick()
  const buttons = find(root, (n) => n.type === 'button')
  assert.deepEqual(buttons.map((n) => n.props.title), ['New window', 'Minimize', 'Maximize', 'Hide to tray'])
  let finish
  m.mock.open = () => { m.mock.opens++; return new Promise((resolve) => { finish = resolve }) }
  const opening = buttons[0].props.onClick()
  await m.nextTick()
  assert.equal(buttons[0].props.disabled, true)
  await buttons[0].props.onClick()
  assert.equal(m.mock.opens, 1)
  finish('suite-1')
  await opening
  m.mock.open = async () => { throw new Error('Window creation failed') }
  await buttons[0].props.onClick()
  await m.nextTick()
  const alert = find(root, (n) => n.props.role === 'alert')[0]
  assert.ok(alert)
  assert.match(find(alert, (n) => n.type === 'span')[0].text, /Window creation failed/)
  assert.equal(buttons[0].props.disabled, false)
  app.unmount()
})

test('additional window controls close their own view', async () => {
  const m = await fresh()
  m.mock.window.label = 'suite-2'
  let closed = false
  m.mock.window.close = async () => { closed = true }
  const { root, app } = mount(m)
  await tick()
  const close = find(root, (n) => n.props.title === 'Close window')[0]
  await close.props.onClick()
  await tick()
  assert.equal(closed, true)
  app.unmount()
})
