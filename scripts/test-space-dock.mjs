import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { parse, compileScript } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as Vue from 'vue'

// Render the real shared dock and panel; replace only the PTY/emulator boundary.
function component(path, bindings) {
  const { descriptor } = parse(readFileSync(new URL(path, import.meta.url), 'utf8'))
  const compiled = compileScript(descriptor, { id: path, inlineTemplate: true })
  const source = compiled.content.replace(/^import .* from ['"]([^'"]+)['"];?$/gm, (line, module) => {
    if (module !== 'vue') return ''
    return line.replace(/^import\s*\{/, 'const {').replace(/\s+as\s+/g, ': ')
      .replace(/\}\s*from\s*['"]vue['"];?$/, '} = Vue')
  }).replace('export default', 'return')
  const js = ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext })
  return new Function('Vue', ...Object.keys(bindings), `const { ref, computed, watch, onMounted, nextTick } = Vue;\n${js}`)(Vue, ...Object.values(bindings))
}

const store = { errorCount: 0, warningCount: 0, allMarkers: [] }
const bindings = { useEditorStore: () => store, qs: { console: { listShells: async () => [] } } }
const Dock = component('../modules/code/app/components/Dock.vue', bindings)
const Panel = component('../modules/code/app/components/DockPanel.vue', bindings)

globalThis.document = { activeElement: null }
function element(type) { return { type, children: [], parent: null, style: {}, addEventListener() {}, focus() {} } }
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

test('QuantSpace docks retain their terminal across collapse, tabs and cached module switches', async () => {
  const modules = ['code', 'canvas', 'console']
  const current = Vue.ref('code')
  const panels = Vue.reactive(Object.fromEntries(modules.map(id => [id, null])))
  const sessions = new Map()
  let opened = 0
  let disposed = 0
  const Pane = Vue.defineComponent({
    setup() {
      const session = { id: ++opened, buffer: 'existing output', cwd: '/changed-directory' }
      sessions.set(current.value, session)
      Vue.onUnmounted(() => { disposed++ })
      return () => Vue.h('terminal', { session })
    },
  })
  const Stage = Vue.defineComponent({
    props: ['id'],
    setup: props => () => Vue.h(Dock, { panel: panels[props.id], root: '.', visible: current.value === props.id }),
  })
  const app = renderer.createApp({
    setup: () => () => Vue.h(Vue.KeepAlive, null, { default: () => Vue.h(Stage, { key: current.value, id: current.value }) }),
  })
  app.component('QDock', { setup: (_, { slots }) => () => Vue.h('dock', null, slots.default?.()) })
  app.component('CodeDockPanel', Panel)
  app.component('ConsolePane', Pane)
  app.mount(element('root'))
  assert.equal(opened, 0, 'closed docks must not start a shell')

  for (const id of modules) {
    current.value = id
    await Vue.nextTick()
    panels[id] = 'problems'
    await Vue.nextTick()
    assert.equal(sessions.has(id), false, 'non-terminal tabs must not start a shell')
    panels[id] = 'terminal'
    await Vue.nextTick()
    const session = sessions.get(id)
    for (const panel of [null, 'terminal', 'search', 'terminal', null, 'terminal']) {
      panels[id] = panel
      await Vue.nextTick()
      assert.equal(sessions.get(id), session)
      assert.equal(disposed, 0)
    }
  }
  for (const id of [...modules, ...modules].reverse()) {
    current.value = id
    await Vue.nextTick()
    assert.equal(sessions.get(id).buffer, 'existing output')
    assert.equal(sessions.get(id).cwd, '/changed-directory')
    assert.equal(opened, 3)
    assert.equal(disposed, 0)
  }
  app.unmount()
  assert.equal(disposed, 3, 'final teardown still releases the panes')
})
