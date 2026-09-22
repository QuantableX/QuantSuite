import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import ts from 'typescript'
import { parse } from '@vue/compiler-sfc'
import { ref } from 'vue'

const read = path => readFileSync(new URL(`../modules/hud/app/${path}`, import.meta.url), 'utf8')
const transpile = source => ts.transpile(source, { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext })

// Run the real config loader and shared-file queue; only storage/IPC and Nuxt
// lifecycle auto-imports are substituted. No user config is accessed.
function configHarness(native, stored) {
  let raw = JSON.stringify(stored)
  let writes = 0
  const invoke = async (command, args) => {
    if (command === 'plugin:hud|load_config') return raw
    assert.equal(command, 'plugin:hud|save_config')
    raw = args.config
    writes++
  }
  const compile = source => transpile(source).replace(/export /g, '')
    .replaceAll('import("@tauri-apps/api/core")', 'Promise.resolve({ invoke: ipc })')
  const updateConfigFile = new Function('ipc', 'emitSync',
    `${compile(read('composables/useConfigFile.ts'))}; return updateConfigFile`)(invoke, () => {})
  const useConfig = new Function('ref', 'window', 'localStorage', 'ipc', 'updateConfigFile', 'onMounted', 'onUnmounted',
    `${compile(read('composables/useConfig.ts'))}; return useConfig`)(
    ref, { __TAURI__: native }, {
      getItem: () => raw,
      setItem: (_, value) => { raw = value; writes++ },
    }, invoke, updateConfigFile, () => {}, () => {},
  )
  return { api: useConfig(), stored: () => JSON.parse(raw), writes: () => writes }
}

for (const native of [false, true]) {
  for (const legacy of ['column', 'halfcircle']) {
    test(`${native ? 'native config' : 'localStorage'} migrates ${legacy} without losing settings`, async () => {
      const original = {
        triggerStyle: legacy, windowPosition: 'dual', monitorIndex: 2, activationMode: 'click',
        calcSettings: { accountSize: 12345 }, _clipboardHistory: ['keep'],
        _calendar: { events: ['appointment'] }, futureModule: { enabled: true },
      }
      const h = configHarness(native, original)
      await h.api.loadConfig()
      assert.equal(h.api.config.value.triggerStyle, 'tab')
      assert.equal(h.stored().triggerStyle, 'tab')
      for (const [key, value] of Object.entries(original)) {
        if (key !== 'triggerStyle') assert.deepEqual(h.stored()[key], value, key)
      }
      assert.equal(h.writes(), 1)
      await h.api.loadConfig()
      assert.equal(h.writes(), 1, 'sync/reload must not start a migration write loop')
    })
  }
}

const { descriptor } = parse(read('pages/hud/index.vue'))
const script = ts.createSourceFile('hud.ts', descriptor.scriptSetup.content, ts.ScriptTarget.Latest, true)
const handlers = script.statements.filter(node => ts.isFunctionDeclaration(node)
  && ['onMouseEnter', 'onMouseLeave', 'onTriggerClick'].includes(node.name?.text))
  .map(node => node.getText(script)).join('\n')

function controls(side, mode) {
  const isTucked = ref(true)
  const isPinned = ref(false)
  const calls = []
  const api = new Function('isTucked', 'isPinned', 'activationMode', 'windowPosition', 'config', 'isTauri', 'invoke',
    `${transpile(handlers)}; return { onMouseEnter, onMouseLeave, onTriggerClick }`)(
    isTucked, isPinned, ref(mode), ref(side), ref({ monitorIndex: 2 }), true,
    async (command, args) => { calls.push({ command, args }) },
  )
  return { ...api, isTucked, isPinned, calls }
}

for (const side of ['left', 'right', 'top']) {
  test(`${side}: hover opens and leaves close; click remains open until another click`, async () => {
    for (const mode of ['hover', 'click']) {
      const h = controls(side, mode)
      await (mode === 'hover' ? h.onMouseEnter() : h.onTriggerClick())
      assert.equal(h.isTucked.value, false)
      assert.deepEqual(h.calls[0], { command: 'plugin:hud|show_window', args: { position: side, monitorIndex: 2 } })
      await h.onMouseLeave()
      assert.equal(h.isTucked.value, mode === 'hover')
      if (mode === 'click') await h.onTriggerClick()
      assert.equal(h.isTucked.value, true)
      assert.equal(h.calls.at(-1).command, 'plugin:hud|tuck_window')
    }
  })

  test(`${side}: pinned HUD stays open on leave/click and repeated hover does not resize again`, async () => {
    const h = controls(side, 'hover')
    await h.onMouseEnter()
    await h.onMouseEnter()
    assert.equal(h.calls.length, 1)
    h.isPinned.value = true
    await h.onMouseLeave()
    await h.onTriggerClick()
    assert.equal(h.isTucked.value, false)
    assert.equal(h.calls.length, 1)
  })
}

for (const native of [false, true]) {
  test(`${native ? 'native' : 'browser'} persists top mode and preserves shared settings`, async () => {
    const h = configHarness(native, { triggerStyle: 'tab', windowPosition: 'left', _calendar: { keep: true } })
    await h.api.loadConfig()
    h.api.setWindowPosition('top')
    await new Promise(setImmediate)
    assert.equal(h.stored().windowPosition, 'top')
    assert.deepEqual(h.stored()._calendar, { keep: true })
    await h.api.loadConfig()
    assert.equal(h.api.config.value.windowPosition, 'top')
  })
}

// Exercise the real wheel router. The DOM boundary is substituted; browser
// interaction tests additionally cover clipping and nested scrolling.
class ScrollNode {
  parentElement = null
  style = { overflowY: 'visible' }
  scrollTop = 0
  clientHeight = 100
  scrollHeight = 100
  editable = false
  closest() { return this.editable ? this : null }
}
const routeWheel = new Function('Element', 'getComputedStyle',
  `${transpile(read('utils/horizontalScroll.ts')).replace('export ', '')}; return horizontalWheel`)(ScrollNode, node => node.style)
function wheelHarness() {
  const rail = Object.assign(new ScrollNode(), { clientWidth: 800, scrollWidth: 1800, scrollLeft: 0 })
  const target = Object.assign(new ScrollNode(), { parentElement: rail })
  const event = { target, deltaX: 0, deltaY: 120, deltaMode: 0, defaultPrevented: false,
    preventDefault() { this.defaultPrevented = true } }
  return { rail, target, event }
}
test('vertical wheel moves the rail in both directions, clamps and respects delta units', () => {
  const {rail, event} = wheelHarness()
  assert.equal(routeWheel(event, rail), true)
  assert.equal(rail.scrollLeft, 120)
  event.defaultPrevented = false; event.deltaY = -5; event.deltaMode = 1
  routeWheel(event, rail)
  assert.equal(rail.scrollLeft, 40)
  event.defaultPrevented = false; event.deltaY = 2; event.deltaMode = 2
  routeWheel(event, rail)
  assert.equal(rail.scrollLeft, 1000)
  event.defaultPrevented = false
  assert.equal(routeWheel(event, rail), false)
  assert.equal(event.defaultPrevented, false)
})
test('horizontal gestures, zoom and editable controls keep their own wheel behavior', () => {
  for (const update of [{deltaX: 150}, {ctrlKey: true}, {metaKey: true}, {shiftKey: true}]) {
    const {rail, event} = wheelHarness()
    Object.assign(event, update)
    assert.equal(routeWheel(event, rail), false)
    assert.equal(rail.scrollLeft, 0)
  }
  const {rail, target, event} = wheelHarness()
  target.editable = true
  assert.equal(routeWheel(event, rail), false)
  assert.equal(event.defaultPrevented, false)
})
test('a nested list consumes its own scroll until its boundary is reached', () => {
  const {rail, target, event} = wheelHarness()
  const list = Object.assign(new ScrollNode(), { parentElement: rail, style: {overflowY: 'auto'}, scrollHeight: 500 })
  target.parentElement = list
  assert.equal(routeWheel(event, rail), false)
  assert.equal(rail.scrollLeft, 0)
  list.scrollTop = 400
  assert.equal(routeWheel(event, rail), true)
  assert.equal(rail.scrollLeft, 120)
})

const transitions = script.statements.filter(node => ts.isFunctionDeclaration(node)
  && ['handlePositionChange', 'handleMonitorChange', 'reapplyGeometry'].includes(node.name?.text))
  .map(node => node.getText(script)).join('\n')
function transitionHarness(label = 'hud') {
  const config = ref({windowPosition: 'dual', monitorIndex: 0})
  const calls = []
  const api = new Function('config', 'windowLabel', 'isTauri', 'invoke', 'setWindowPosition', 'setMonitorIndex', 'windowPosition', 'isTucked', 'eventIpc',
    `${transpile(transitions).replace(/import\(['"]@tauri-apps\/api\/event['"]\)/g, 'Promise.resolve({emitTo: eventIpc})')}; return {handlePositionChange,handleMonitorChange}`)(
    config, ref(label), true, async (command, args) => calls.push({command,args}),
    position => { config.value.windowPosition = position }, index => { config.value.monitorIndex = index },
    ref('left'), ref(false), async (...args) => calls.push({emit:args}),
  )
  return {...api, config, calls}
}
test('the primary overlay leaves Dual for Top and recreates Dual in order', async () => {
  const h = transitionHarness()
  await h.handlePositionChange('top')
  assert.equal(h.config.value.windowPosition, 'top')
  assert.deepEqual(h.calls, [{command:'plugin:hud|close_dual_window',args:undefined},
    {command:'plugin:hud|set_window_position',args:{position:'top',monitorIndex:0}}])
  h.calls.length = 0
  await h.handlePositionChange('dual')
  assert.deepEqual(h.calls.map(c=>c.command), ['plugin:hud|close_dual_window','plugin:hud|set_window_position','plugin:hud|create_dual_window'])
  assert.equal(h.calls[1].args.position, 'left')
})
test('position and monitor changes from the right pane are delivered before it is closed', async () => {
  const h = transitionHarness('dual-right')
  await h.handlePositionChange('top')
  await h.handleMonitorChange(2)
  assert.deepEqual(h.calls, [{emit:['hud','hud:position-change','top']}, {emit:['hud','hud:monitor-change',2]}])
  assert.equal(h.config.value.windowPosition, 'dual')
  assert.equal(h.config.value.monitorIndex, 0)
})
test('monitor changes preserve the open panel and move the Dual companion too', async () => {
  const h = transitionHarness()
  await h.handleMonitorChange(2)
  assert.deepEqual(h.calls.map(c=>c.command), ['plugin:hud|show_window','plugin:hud|close_dual_window','plugin:hud|create_dual_window'])
  assert.equal(h.calls[0].args.monitorIndex, 2)
  assert.equal(h.calls[2].args.monitorIndex, 2)
})

const notesScript = ts.createSourceFile('NotesModule.ts', parse(read('components/NotesModule.vue')).descriptor.scriptSetup.content, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
const notesFunction = name => notesScript.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name).getText(notesScript)
function noteDraftHarness() {
  const pending = notesScript.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(d => d.name.getText(notesScript) === 'pendingContent')).getText(notesScript)
  const cleanup = notesScript.statements.find(node => ts.isExpressionStatement(node) && node.getText(notesScript).startsWith('onUnmounted(')).getText(notesScript)
  const sections = ref([{id:'section',notes:[{id:'a',content:''},{id:'b',content:''}]}])
  const saves = [], timers = new Map()
  let sequence = 0, unmount
  const edit = new Function('sections','updateNoteContent','setTimeout','clearTimeout','onUnmounted','document','onDocSectionDragOver','onDocSectionDrop',
    `${transpile(pending+'\n'+cleanup+'\n'+notesFunction('handleContentChange'))}; return handleContentChange`)(
    sections, (...args) => saves.push(args), fn => { timers.set(++sequence,fn); return sequence }, id => timers.delete(id),
    fn => { unmount=fn }, {removeEventListener(){}}, ()=>{}, ()=>{},
  )
  return {sections,saves,timers,edit:(id,value)=>edit('section',id,{target:{value}}),unmount:()=>unmount()}
}
test('editing adjacent notes keeps both drafts and saves each latest value', () => {
  const h=noteDraftHarness()
  h.edit('a','first');h.edit('b','second');h.edit('a','first revised')
  assert.deepEqual(h.sections.value[0].notes.map(n=>n.content),['first revised','second'])
  assert.equal(h.timers.size,2)
  for(const callback of [...h.timers.values()])callback()
  assert.deepEqual(h.saves,[['section','b','second'],['section','a','first revised']])
})
test('leaving Notes before the debounce expires flushes every draft', () => {
  const h=noteDraftHarness()
  h.edit('a','keep A');h.edit('b','keep B');h.unmount()
  assert.equal(h.timers.size,0)
  assert.deepEqual(h.saves,[['section','a','keep A'],['section','b','keep B']])
})
test('note reorder uses the horizontal midpoint only in Top mode', () => {
  const props={horizontal:true}, position=ref(null), section=ref(null), index=ref(null)
  const over=new Function('props','dragType','dropTargetSectionId','dropTargetNoteIdx','dropPosition',
    `${transpile(notesFunction('onNoteDragOver'))};return onNoteDragOver`)(props,ref('note'),section,index,position)
  const event={clientX:350,clientY:90,currentTarget:{getBoundingClientRect:()=>({left:300,top:50,width:200,height:100})}}
  over('s',2,event);assert.equal(position.value,'above')
  event.clientX=450;over('s',2,event);assert.equal(position.value,'below')
  props.horizontal=false;over('s',2,event);assert.equal(position.value,'above')
  assert.equal(section.value,'s');assert.equal(index.value,2)
})
