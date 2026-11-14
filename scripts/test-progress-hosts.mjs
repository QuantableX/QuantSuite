// Run the actual host polling code against a controlled clock and failed transport.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import vm from 'node:vm'
import { test } from 'node:test'
import ts from 'typescript'
import { renderBoard } from '../packages/ui/src/progress/board.js'

const NOW = 1_800_000_000_000
const source = file => fs.readFileSync(new URL(`../${file}`, import.meta.url), 'utf8')
const snapshot = now => ({ now, lanes: [{ key: 'a', agent: 'a', tasks: [{ id: 't', title: 'Job', state: 'running', startedAt: NOW, updatedAt: now }], shells: [] }] })

for (const host of ['drawer', 'page']) test(`${host}: disconnected snapshots age out, preserve evidence, and recover`, async () => {
  let now = NOW, offline = false, rendered = '', updates = 0
  const root = {}, conn = {}
  const transport = async () => { if (offline) throw new Error('Offline'); return snapshot(now) }
  const updateBoard = (_root, view, options) => { updates++; rendered = renderBoard(view, options) }
  const context = vm.createContext({
    Date: { now: () => now }, window: { __TAURI_INTERNALS__: {} },
    document: { getElementById: id => id === 'board' ? root : conn },
    setInterval: () => 1, clearInterval: () => {}, updateBoard,
    fetch: async () => ({ ok: true, json: transport }),
    defineProps: () => ({ queue: [] }), defineEmits: () => () => {},
    require: id => {
      if (id === 'vue') return { ref: value => ({ value }), nextTick: async () => {}, watch: () => {}, onMounted: () => {}, onUnmounted: () => {} }
      if (id === '@quantsuite/core') return { qs: { core: {} } }
      if (id === '@tauri-apps/api/core') return { invoke: transport }
      if (id.endsWith('board.js')) return { updateBoard }
      if (id.endsWith('.css')) return {}
      throw new Error(`Unexpected import: ${id}`)
    }, exports: {},
  })
  if (host === 'drawer') {
    const script = source('packages/ui/src/components/QDrawerAgent.vue').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
    vm.runInContext(ts.transpileModule(script, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText + '\nglobalThis.harness={tick:loadBoard,board,boardRoot,boardLive}', context)
    context.harness.boardRoot.value = root
  } else {
    const page = source('modules/mcp/crate/src/progress.rs').match(/const PAGE_HTML: &str = r#"([\s\S]*?)"#;/)[1]
    const script = page.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
      .replace(/import \{ updateBoard \} from '[^']+'/,'').replace(/^\s*tick\(\)\s*$/m, '')
    vm.runInContext(script + '\nglobalThis.harness={tick}', context)
  }
  await context.harness.tick()
  assert.match(rendered, /1 task reporting progress/)
  offline = true
  now += 20 * 60_000
  await context.harness.tick()
  assert.match(rendered, /No recent agent activity/)
  assert.match(rendered, /Activity unconfirmed/)
  assert.match(rendered, /Job/)
  assert.doesNotMatch(rendered, /reporting progress/)
  if (host === 'drawer') {
    assert.equal(context.harness.boardLive.value, false)
    assert.equal(context.harness.board.value.now, NOW, 'failed requests must not rewrite the evidence timestamp')
  } else assert.equal(conn.className, 'is-off')
  offline = false
  await context.harness.tick()
  assert.match(rendered, /1 task reporting progress/)
  assert.doesNotMatch(rendered, /qpb-history/)
  assert.equal(updates, 3)
  if (host === 'drawer') assert.equal(context.harness.boardLive.value, true)
  else assert.equal(conn.className, '')
})
