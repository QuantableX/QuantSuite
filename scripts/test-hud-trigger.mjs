import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import ts from 'typescript'
import { parse } from '@vue/compiler-sfc'
import { computed, ref } from 'vue'

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
const pageNames = ['tuckedForCapture', 'tuckForCapture', 'onMouseEnter', 'onMouseLeave', 'onTriggerClick', 'handleCapture']
const handlers = script.statements.filter(node => (ts.isFunctionDeclaration(node) && pageNames.includes(node.name?.text))
  || (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => pageNames.includes(d.name.getText(script)))))
  .map(node => node.getText(script)).join('\n')

// `extract` stands in for useFibExtractor's captureAndExtract.
function controls(side, mode, extract = async () => false) {
  const isTucked = ref(true)
  const isPinned = ref(false)
  const calls = []
  const savedRegions = []
  const api = new Function('isTucked', 'isPinned', 'activationMode', 'windowPosition', 'config', 'isTauri', 'invoke',
    'isProcessing', 'captureAndExtract', 'setScanRegion', 'getLevelPrices', 'isLong', 'levels',
    `${transpile(handlers)}; return { onMouseEnter, onMouseLeave, onTriggerClick, handleCapture }`)(
    isTucked, isPinned, ref(mode), ref(side), ref({ monitorIndex: 2 }), true,
    async (command, args) => { calls.push({ command, args }) },
    ref(false), extract, region => savedRegions.push(region), () => ({ entry: 0, tp: 0, sl: 0 }), ref(true), {},
  )
  return { ...api, isTucked, isPinned, calls, savedRegions }
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

  test(`${side}: capture tucks the open HUD out of the shot and reopens it before the OCR`, async () => {
    const h = controls(side, 'hover', async afterCapture => {
      h.calls.push({ command: 'capture' })
      // Leaving the shrunken panel or clicking its tab mid-capture changes nothing
      await h.onMouseLeave()
      await h.onTriggerClick()
      await afterCapture()
      h.calls.push({ command: 'ocr' })
      return false
    })
    await h.onMouseEnter()
    h.calls.length = 0
    // A second press during the tuck delay is ignored
    await Promise.all([h.handleCapture(), h.handleCapture()])
    assert.deepEqual(h.calls.map(c => c.command), ['plugin:hud|tuck_window', 'capture', 'plugin:hud|show_window', 'ocr'])
    assert.deepEqual(h.calls[0].args, { position: side, monitorIndex: 2 })
    assert.equal(h.isTucked.value, false)
    assert.deepEqual(h.savedRegions, [])
    await h.onMouseLeave()
    assert.equal(h.isTucked.value, true, 'hover works again after the capture')
  })
}

test('F9 on a tucked HUD captures without opening it, and a dropped region is forgotten', async () => {
  let hook
  const h = controls('right', 'click', async afterCapture => { hook = afterCapture; return true })
  await h.handleCapture()
  assert.equal(hook, undefined)
  assert.deepEqual(h.calls, [])
  assert.equal(h.isTucked.value, true)
  assert.deepEqual(h.savedRegions, [null])
})

// The real composable; only IPC and the OCR engine are substituted.
function fibExtractor(ipc, ocrText = '') {
  const compile = source => transpile(source).replace(/export /g, '')
    .replaceAll('import("@tauri-apps/api/core")', 'Promise.resolve({ invoke: ipc })')
    .replaceAll('import("tesseract.js")', 'Promise.resolve(tesseract)')
  const tesseract = { createWorker: async () => ({ recognize: async () => ({ data: { text: ocrText } }) }) }
  return new Function('ref', 'window', 'ipc', 'tesseract', 'console',
    `${compile(read('composables/useFibExtractor.ts'))}; return useFibExtractor`)(
    ref, { __TAURI_INTERNALS__: {} }, ipc, tesseract, { log() {}, error() {} })()
}

test('a failed capture shows the command error text and still reopens the HUD', async () => {
  let reopened = 0
  const f = fibExtractor(async () => { throw 'Failed to capture screen: no display' })
  assert.equal(await f.captureAndExtract(async () => { reopened++ }), false)
  assert.equal(f.status.value, '❌ Failed to capture screen: no display')
  assert.equal(reopened, 1)
  assert.equal(f.isProcessing.value, false)
})

test('Dual: each pane tucks and reopens on its own edge for a capture', async () => {
  const names = ['windowPosition', 'tuckedForCapture', 'tuckForCapture']
  const code = script.statements.filter(node => ts.isVariableStatement(node)
    && node.declarationList.declarations.some(d => names.includes(d.name.getText(script))))
    .map(node => node.getText(script)).join('\n')
  for (const [label, edge] of [['dual-right', 'right'], ['hud', 'left']]) {
    const calls = []
    const tuckForCapture = new Function('computed', 'config', 'windowLabel', 'isTauri', 'invoke', 'isTucked',
      `${transpile(code)}; return tuckForCapture`)(
      computed, ref({ windowPosition: 'dual', monitorIndex: 1 }), ref(label), true,
      async (command, args) => { calls.push({ command, args }) }, ref(false),
    )
    const reopen = await tuckForCapture()
    assert.equal(await tuckForCapture(), undefined, 'a second capture does not tuck again')
    await reopen()
    assert.deepEqual(calls, [
      { command: 'plugin:hud|tuck_window', args: { position: edge, monitorIndex: 1 } },
      { command: 'plugin:hud|show_window', args: { position: edge, monitorIndex: 1 } },
    ], label)
  }
})

// The real composable; only IPC and storage are substituted.
function chartAnalyzer(ipc) {
  const source = transpile(read('composables/useChartAnalyzer.ts')).replace(/export /g, '')
    .replaceAll('import("@tauri-apps/api/core")', 'Promise.resolve({ invoke: ipc })')
  return new Function('ref', 'window', 'localStorage', 'ipc', `${source}; return useChartAnalyzer`)(
    ref, { __TAURI_INTERNALS__: {} }, { getItem: () => null, setItem() {} }, ipc)()
}

test('the chart analyzer reopens the HUD right after the grab, before the AI call, also on errors', async () => {
  for (const failCapture of [false, true]) {
    const order = []
    const analyzer = chartAnalyzer(async command => {
      order.push(command)
      if (command !== 'plugin:hud|capture_screen') return 'no json here'
      if (failCapture) throw 'Failed to capture screen: gone'
      return { image_base64: 'AA==', width: 1, height: 1, region_dropped: false }
    })
    const tuck = async () => { order.push('tuck'); return async () => { order.push('reopen') } }
    assert.equal(await analyzer.captureAndAnalyze(null, 'ollama', 'http://localhost:11434', 'm', ['wyckoff'], tuck), false)
    assert.deepEqual(order, failCapture
      ? ['tuck', 'plugin:hud|capture_screen', 'reopen']
      : ['tuck', 'plugin:hud|capture_screen', 'reopen', 'plugin:hud|analyze_chart'])
    assert.equal(analyzer.status.value, failCapture ? 'Failed to capture screen: gone' : 'Could not parse response')
    assert.equal(analyzer.isAnalyzing.value, false)
  }
})

test('a capture while one is running hands a tucked HUD back at once', async () => {
  let release, reopened = 0
  const f = fibExtractor(() => new Promise(resolve => { release = resolve }))
  const first = f.captureAndExtract()
  assert.equal(await f.captureAndExtract(async () => { reopened++ }), false)
  assert.equal(reopened, 1)
  await new Promise(setImmediate)
  release({ image_base64: 'AA==', width: 1, height: 1, region_dropped: false })
  assert.equal(await first, false)
  assert.equal(f.isProcessing.value, false)
})

test('an off-screen region is cleared and the default crop still yields levels', async () => {
  let sent, reopened = 0
  const f = fibExtractor(async (command, args) => {
    assert.equal(command, 'plugin:hud|capture_screen')
    sent = args.region
    return { image_base64: 'AA==', width: 384, height: 1200, region_dropped: true }
  }, '1 (3,151.25)\n0 (3,100.50)')
  f.scanRegion.value = [2154, 103, 276, 1358]
  assert.equal(await f.captureAndExtract(async () => { reopened++ }), true)
  assert.deepEqual(sent, [2154, 103, 276, 1358])
  assert.equal(f.scanRegion.value, null)
  assert.deepEqual({ ...f.fibPrices.value }, { 0: 3100.5, 1: 3151.25 })
  assert.equal(f.status.value, '✓ 2/7 levels found · off-screen region cleared')
  assert.equal(reopened, 1, 'the HUD reopens exactly once')
})

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
test('horizontal gestures, zoom and a focused number field keep their own wheel behavior', () => {
  for (const update of [{deltaX: 150}, {ctrlKey: true}, {metaKey: true}, {shiftKey: true}]) {
    const {rail, event} = wheelHarness()
    Object.assign(event, update)
    assert.equal(routeWheel(event, rail), false)
    assert.equal(rail.scrollLeft, 0)
  }
  const {rail, target, event} = wheelHarness()
  target.editable = true
  target.ownerDocument = { activeElement: target }
  assert.equal(routeWheel(event, rail), false)
  assert.equal(event.defaultPrevented, false)
})
test('a text field that fits its content, or an unfocused control, moves the rail', () => {
  const {rail, target, event} = wheelHarness()
  target.editable = true
  target.ownerDocument = { activeElement: null }
  assert.equal(routeWheel(event, rail), true)
  assert.equal(rail.scrollLeft, 120)
})
test('a long note keeps the wheel until its text reaches the end', () => {
  const {rail, target, event} = wheelHarness()
  Object.assign(target, { style: {overflowY: 'auto'}, scrollHeight: 400 })
  assert.equal(routeWheel(event, rail), false)
  target.scrollTop = 300
  assert.equal(routeWheel(event, rail), true)
  assert.equal(rail.scrollLeft, 120)
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
