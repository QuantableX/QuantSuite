// Real composable + compiled Vue tests, with an isolated IPC store. No user settings change.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { build } from 'esbuild'
import { parse, compileScript, compileStyle } from '@vue/compiler-sfc'
import { readFile, writeFile, mkdir, mkdtemp } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { spawn } from 'node:child_process'

const root = fileURLToPath(new URL('../', import.meta.url))
const mockSource = `
import { ref } from 'vue';
const states = new Map();
export function useState(key, init) { if (!states.has(key)) states.set(key, ref(init())); return states.get(key); }
const builtin = name => ({ name, description: 'Manage ' + name.replaceAll('_', ' ') + ' in the active workspace.', parameters: [] });
const suite = (module, name) => ({ module, name, tool: 'quantsuite.' + module + '.' + name, description: 'Access ' + module + ' through QuantSuite.', sideEffects: 'read' });
export const mock = {
  prefs: { disabledGroups: [], disabledTools: [] }, writes: [], listeners: [], failRead: false, failSave: false, hold: null,
  settings: { 'apps.enabled.quantzen': true, 'apps.enabled.quantview': true },
  catalogue: {
    list_tools: [{ id: 'native', name: 'native_example', description: 'Built-in script', native: true, enabled: true, input_schema: {} }, { id: 'custom', name: 'my_script', description: 'Your custom tool', native: false, enabled: true, input_schema: {} }],
    list_codebase_index_tools: ['get_instructions', 'search_code'].map(builtin),
    list_agentos_tools: ['get_agent_instructions', 'update_agent_instructions'].map(builtin),
    list_kanban_tools: ['list_kanban_cards', 'create_kanban_card'].map(builtin),
    list_worktree_tools: ['create_worktree', 'list_worktrees'].map(builtin),
    list_bridge_tools: [suite('memory', 'search'), suite('memory', 'read'), suite('notes', 'search'), suite('notes', 'create_note'), suite('finance', 'summary')],
  },
  event(value) { this.prefs = structuredClone(value); this.listeners.forEach(fn => fn({ payload: {scope:'mcp', key:'tool_preferences', value: structuredClone(value)} })); },
};
export const bus = { on(topic, fn) { mock.listeners.push(fn); } };
export const qs = { core: {
  async getSettings() { return structuredClone(mock.settings); },
  async setSetting(scope, key, value) {
    mock.settings[key] = value;
    mock.listeners.forEach(fn => fn({ payload: { scope, key, value } }));
  },
} };
export async function invoke(command, args) {
  const name = command.split('|')[1];
  if (name === 'get_tool_preferences') { if (mock.failRead) throw Error('database unavailable'); return structuredClone(mock.prefs); }
  if (name === 'set_tool_preference' || name === 'toggle_tool') {
    mock.writes.push({ command: name, ...args });
    if (mock.hold) await mock.hold;
    if (mock.failSave) throw Error('disk full');
    if (name === 'toggle_tool') { mock.catalogue.list_tools.find(t => t.id === args.id).enabled = args.enabled; return; }
    const key = args.kind === 'group' ? 'disabledGroups' : 'disabledTools';
    const values = new Set(mock.prefs[key]); args.enabled ? values.delete(args.name) : values.add(args.name);
    mock.prefs[key] = [...values];
    return structuredClone(mock.prefs);
  }
  return structuredClone(mock.catalogue[name] ?? []);
}`

let css = ''
function plugin() {
  return { name: 'isolated-ipc-and-vue', setup(b) {
    b.onResolve({ filter: /^@quantsuite\/core$/ }, () => ({ path: 'core', namespace: 'actual-core' }))
    b.onLoad({ filter: /.*/, namespace: 'actual-core' }, () => ({ contents: `export * from './packages/core/src/app-availability'; export { bus } from 'mock';`, loader: 'js', resolveDir: root }))
    b.onResolve({ filter: /^(\.\/commands|\.\/bus)$/ }, ({ path, importer }) => {
      if (importer.endsWith('app-availability.ts')) return { path, namespace: 'core-ipc' }
    })
    b.onLoad({ filter: /.*/, namespace: 'core-ipc' }, ({ path }) => ({
      contents: path === './commands' ? `export { qs } from 'mock';` : `import { bus } from 'mock'; export const on = bus.on;`, loader: 'js', resolveDir: root,
    }))
    b.onResolve({ filter: /^(mock|@tauri-apps\/api\/core)$/ }, () => ({ path: 'mock', namespace: 'fixture' }))
    b.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents: mockSource, loader: 'js', resolveDir: root }))
    b.onResolve({ filter: /^#mcp\// }, ({ path }) => ({ path: resolve(root, 'modules/mcp/app', path.slice(5)) + '.ts' }))
    b.onLoad({ filter: /useTools\.ts$/ }, async ({ path }) => ({
      contents: `import { computed } from 'vue'; import { useState } from 'mock';\n${await readFile(path, 'utf8')}`,
      loader: 'ts', resolveDir: root,
    }))
    b.onLoad({ filter: /\.vue$/ }, async ({ path }) => {
      const { descriptor } = parse(await readFile(path, 'utf8'), { filename: path })
      const id = path.includes('ToolToggle') ? 'data-v-toggle-test' : 'data-v-tools-test'
      const compiled = compileScript(descriptor, { id, inlineTemplate: true })
      css += descriptor.styles.map(style => compileStyle({ source: style.content, id, scoped: style.scoped }).code).join('\n')
      return { contents: `import { ref, onMounted } from 'vue';\n${compiled.content.replace('export default ', 'const component = ')}\ncomponent.__scopeId = '${id}'; export default component;`, loader: 'ts', resolveDir: root }
    })
  } }
}
async function bundle(contents, browser = false) {
  return (await build({ stdin: { contents, resolveDir: root }, bundle: true, write: false, format: browser ? 'iife' : 'esm', platform: 'browser',
    define: { 'import.meta.client': 'true', __VUE_OPTIONS_API__: 'true', __VUE_PROD_DEVTOOLS__: 'false', __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: 'false', 'process.env.NODE_ENV': '"production"' }, plugins: [plugin()],
  })).outputFiles[0].text
}
const source = await bundle(`export { useTools } from './modules/mcp/app/composables/useTools'; export { mock } from 'mock'; export { loadAppAvailability, setAppEnabled } from '@quantsuite/core';`)
let sequence = 0
async function fresh(settings) {
  globalThis.window = { __TAURI_INTERNALS__: {} }
  const m = await import(`data:text/javascript;base64,${Buffer.from(source + `\n// ${sequence++}`).toString('base64')}`)
  if (settings) m.mock.settings = settings
  await m.loadAppAvailability()
  return { ...m, tools: m.useTools() }
}

test('individual choices survive group round trips and suite parent gating', async () => {
  const { tools: t, mock } = await fresh()
  assert.equal(t.controlsDisabled.value, true)
  await t.refresh()
  assert.equal(t.enabledQuantmcpToolCount.value, 15)
  await t.setPreference('tool', 'list_kanban_cards', false)
  await t.setPreference('group', 'kanban', false)
  assert.equal(t.selectedCount('kanban', t.kanbanTools.value), 0)
  await t.setPreference('group', 'kanban', true)
  assert.equal(t.selectedCount('kanban', t.kanbanTools.value), 1)
  await t.setPreference('group', 'suite.memory', false)
  await t.setPreference('group', 'suite', false)
  assert.equal(t.selectedCount('suite', t.bridgeTools.value), 0)
  await t.setPreference('group', 'suite', true)
  assert.equal(t.selectedCount('suite', t.bridgeTools.value), 3)
  await t.refresh()
  assert.equal(t.toolSelected('list_kanban_cards'), false)
  assert.equal(t.groupEnabled('suite.memory'), false)
  assert.equal(mock.writes.length, 6)
})

test('failed loads and saves are visible, and pending writes lock controls', async () => {
  const { tools: t, mock } = await fresh()
  mock.failRead = true
  await t.refresh()
  assert.equal(t.controlsDisabled.value, true)
  assert.match(t.error.value, /database unavailable/)
  mock.failRead = false
  await t.refresh()
  mock.failSave = true
  await t.setPreference('group', 'kanban', false)
  assert.equal(t.groupEnabled('kanban'), true)
  assert.match(t.error.value, /disk full/)
  mock.failSave = false
  let finish
  mock.hold = new Promise(resolve => { finish = resolve })
  const saving = t.setPreference('group', 'worktree', false)
  await new Promise(resolve => setTimeout(resolve, 0))
  assert.equal(t.controlsDisabled.value, true)
  await t.setPreference('group', 'kanban', false)
  assert.equal(mock.writes.length, 2)
  finish(); await saving
  assert.equal(t.groupEnabled('worktree'), false)
  assert.equal(t.controlsDisabled.value, false)
})

test('script switches, app filtering and cross-window updates affect actual counts', async () => {
  const { tools: t, mock, setAppEnabled } = await fresh()
  await setAppEnabled('quantzen', false)
  await t.refresh()
  assert.equal(t.enabledQuantmcpToolCount.value, 12)
  await t.toggleTool('custom', false)
  assert.equal(t.enabledQuantmcpToolCount.value, 11)
  await t.setPreference('group', 'custom', false)
  await t.setPreference('group', 'custom', true)
  assert.equal(t.customTools.value[0].enabled, false)
  mock.failSave = true
  await t.toggleTool('custom', true)
  assert.equal(t.customTools.value[0].enabled, false)
  mock.event({ disabledGroups: ['worktree'], disabledTools: ['search_code'] })
  assert.equal(t.enabledQuantmcpToolCount.value, 8)
  mock.failSave = false
  await t.refresh()
  assert.equal(t.groupEnabled('worktree'), false)
})

test('all app tools start available; live app switches preserve individual tool choices', async () => {
  const { tools: t, mock, setAppEnabled } = await fresh({})
  mock.catalogue.list_bridge_tools.push({ module: 'algo', name: 'list_strategies', tool: 'quantsuite.algo.list_strategies', sideEffects: 'read' })
  await t.refresh()
  assert.deepEqual(t.bridgeModules.value.map(g => g.module), ['memory', 'notes', 'finance', 'algo'])
  assert.equal(t.enabledQuantmcpToolCount.value, 16)
  await t.setPreference('tool', 'quantsuite.notes.search', false)
  await t.setPreference('group', 'suite.algo', false)
  assert.equal(t.enabledQuantmcpToolCount.value, 14)
  await setAppEnabled('quantzen', false)
  await setAppEnabled('quantview', false)
  assert.equal(t.enabledQuantmcpToolCount.value, 12)
  assert.deepEqual(t.bridgeModules.value.map(g => g.module), ['memory'])
  await setAppEnabled('quantzen', true)
  await setAppEnabled('quantview', true)
  await t.refresh()
  assert.equal(t.enabledQuantmcpToolCount.value, 14)
  assert.equal(t.toolSelected('quantsuite.notes.search'), false)
  assert.equal(t.groupEnabled('suite.algo'), false)
  assert.equal(mock.writes.length, 2, 'app changes do not overwrite tool preferences')
})

test('compiled Tools page: switches, failures, keyboard access and responsive layout', { timeout: 60000 }, async () => {
  css = ''
  const js = await bundle(`
    import { createApp, nextTick } from 'vue';
    import Tools from './modules/mcp/app/pages/mcp/tools.vue';
    import Toggle from './modules/mcp/app/components/ToolToggle.vue';
    import { mock } from 'mock';
    import { loadAppAvailability, setAppEnabled } from '@quantsuite/core';
    window.__TAURI_INTERNALS__ = {}; window.definePageMeta = () => {};
    window.mock = mock; window.nextTick = nextTick; window.setAppEnabled = setAppEnabled;
    const app = createApp(Tools).component('McpToolToggle', Toggle);
    for (const name of ['McpToolFormModal', 'McpToolTestModal', 'McpScriptEditorModal']) app.component(name, { render: () => null });
    loadAppAvailability().then(() => app.mount('#app'));
  `, true)
  const artifactRoot = process.env.QS_TEST_ARTIFACTS ?? tmpdir()
  await mkdir(artifactRoot, { recursive: true })
  const dir = await mkdtemp(join(artifactRoot, 'qs-mcp-tools-review-'))
  const theme = await readFile(join(root, 'modules/mcp/app/assets/css/main.css'), 'utf8')
  await writeFile(join(dir, 'app.js'), js)
  await writeFile(join(dir, 'index.html'), `<!doctype html><meta charset="utf-8"><style>${theme}\n${css}\n*{box-sizing:border-box}body{margin:0;background:var(--bg-primary);color:var(--text-primary);font:14px Arial}#app{max-width:1120px;margin:auto;padding:24px}button,input{font:inherit}button{cursor:pointer}h1,h2,p{margin-top:0}</style><div id="app"></div><script src="app.js"></script>`)
  const browser = spawn(process.env.CHROME_BIN ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe', [
    '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0', `--user-data-dir=${join(dir, 'profile')}`, pathToFileURL(join(dir, 'index.html')).href,
  ], { windowsHide: true, stdio: 'ignore' })
  const pause = () => new Promise(r => setTimeout(r, 100))
  let socket, call
  try {
    let port
    for (let i = 0; i < 100 && !port; i++) {
      try { port = (await readFile(join(dir, 'profile', 'DevToolsActivePort'), 'utf8')).split('\n')[0] } catch { await pause() }
    }
    assert.ok(port, 'Chrome starts')
    const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
    socket = new WebSocket(pages.find(p => p.type === 'page').webSocketDebuggerUrl)
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }) })
    let seq = 0
    call = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++seq
      const timer = setTimeout(() => { socket.removeEventListener('message', receive); reject(Error(`${method} timed out`)) }, 10000)
      function receive(e) { const response = JSON.parse(e.data); if (response.id !== id) return; clearTimeout(timer); socket.removeEventListener('message', receive); response.error ? reject(Error(response.error.message)) : resolve(response.result) }
      socket.addEventListener('message', receive); socket.send(JSON.stringify({ id, method, params }))
    })
    const evaluate = async expression => {
      const response = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
      assert.ok(!response.exceptionDetails, JSON.stringify(response.exceptionDetails))
      return response.result.value
    }
    await call('Emulation.setDeviceMetricsOverride', { width: 1200, height: 960, deviceScaleFactor: 1, mobile: false })
    for (let i = 0; i < 50 && !await evaluate(`document.querySelector('.page-subtitle')?.textContent.includes('15 / 15')`); i++) await pause()
    assert.equal(await evaluate(`document.querySelector('.page-subtitle').textContent`), '15 / 15 tools enabled for AI clients')
    await writeFile(join(dir, 'groups.png'), Buffer.from((await call('Page.captureScreenshot')).data, 'base64'))
    const result = await evaluate(`(async () => {
      const check = (ok, msg) => { if (!ok) throw Error(msg); };
      const settle = () => new Promise(r => setTimeout(r, 40));
      const sw = label => document.querySelector('input[aria-label="' + label + '"]');
      const toggle = async label => { check(sw(label) && !sw(label).disabled, label + ' available'); sw(label).click(); await settle(); };
      const open = async text => { [...document.querySelectorAll('.section-toggle')].find(b => b.textContent.includes(text)).click(); await settle(); };
      await open('Kanban Tools');
      await toggle('Enable list_kanban_cards'); check(!sw('Enable list_kanban_cards').checked, 'single tool disabled');
      await toggle('Enable kanban tools'); check(sw('Enable list_kanban_cards').disabled, 'group gates children');
      await toggle('Enable kanban tools'); check(!sw('Enable list_kanban_cards').checked, 'single choice preserved');
      mock.failSave = true; await toggle('Enable worktree tools');
      check(sw('Enable worktree tools').checked && document.querySelector('[role=alert]').textContent.includes('disk full'), 'failed save restores DOM and shows error');
      mock.failSave = false; await toggle('Enable worktree tools'); check(!sw('Enable worktree tools').checked, 'retry succeeds');
      await open('Suite Tools'); await toggle('Enable memory tools');
      await toggle('Enable suite tools'); check(sw('Enable notes tools').disabled, 'suite parent gates module');
      await toggle('Enable suite tools'); check(!sw('Enable memory tools').checked, 'module preference preserved');
      await toggle('Enable quantsuite.notes.search'); check(!sw('Enable quantsuite.notes.search').checked, 'suite individual');
      await setAppEnabled('quantzen', false); await settle();
      check(!sw('Enable notes tools') && !sw('Enable finance tools'), 'disabled app removes its tools');
      await setAppEnabled('quantzen', true); await settle();
      check(!sw('Enable quantsuite.notes.search').checked, 'reactivation keeps individual choice');
      await open('Custom Tools'); await toggle('Enable my_script'); check(!sw('Enable my_script').checked, 'custom toggle');
      await open('Basic Tools'); await toggle('Enable native_example'); check(!sw('Enable native_example').checked, 'native toggle');
      document.querySelector('.header-actions button').click(); await settle();
      check(!sw('Enable list_kanban_cards').checked && !sw('Enable memory tools').checked, 'refresh preserves choices');
      sw('Enable agentos tools').focus();
      return 'PASS';
    })()`)
    assert.equal(result, 'PASS')
    await call('Input.dispatchKeyEvent', { type: 'keyDown', key: ' ', code: 'Space', windowsVirtualKeyCode: 32 })
    await call('Input.dispatchKeyEvent', { type: 'keyUp', key: ' ', code: 'Space', windowsVirtualKeyCode: 32 })
    await pause()
    assert.equal(await evaluate(`document.querySelector('input[aria-label="Enable agentos tools"]').checked`), false)
    await writeFile(join(dir, 'selected.png'), Buffer.from((await call('Page.captureScreenshot')).data, 'base64'))
    await call('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: false })
    await pause()
    assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'), true, 'no horizontal overflow on mobile')
    await writeFile(join(dir, 'narrow.png'), Buffer.from((await call('Page.captureScreenshot')).data, 'base64'))
    console.log(`Browser review artifacts: ${dir}`)
  } finally {
    if (call) await call('Browser.close').catch(() => {})
    socket?.close(); browser.kill()
  }
})
