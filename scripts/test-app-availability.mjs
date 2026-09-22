// Exercises the actual frontend state with an isolated settings store/event bus.
// No personal settings, MCP clients, or live QuantSuite instance are changed.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { build } from 'esbuild'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const bundle = await build({
  stdin: {
    resolveDir: root,
    contents: `export * from './packages/core/src/app-availability';
      export * from './packages/core/src/registry';
      export * from './packages/core/src/palette-actions';
      export * from './packages/core/src/settings-sections';
      export { mock } from 'test-mock';`,
  },
  bundle: true, write: false, format: 'esm', platform: 'browser',
  plugins: [{ name: 'settings-store', setup(b) {
    b.onResolve({ filter: /^(\.\/commands|\.\/bus|test-mock)$/ }, ({ path }) => ({ path, namespace: 'test-mock' }))
    b.onLoad({ filter: /.*/, namespace: 'test-mock' }, ({ path }) => ({
      contents: path === './commands' ? `import { mock } from 'test-mock'; export const qs = { core: {
        getSettings: (...args) => mock.read(...args), setSetting: (...args) => mock.write(...args)
      } };` : path === './bus' ? `import { mock } from 'test-mock'; export function on(topic, fn) { mock.listeners.push(fn); return () => {} }`
        : `export const mock = { settings: {}, listeners: [], writes: [],
            async read() { return { ...this.settings } },
            async write(scope, key, value) { this.writes.push({scope,key,value}); this.settings[key] = value },
            event(scope, key, value) { for (const fn of this.listeners) fn({ payload: { scope, key, value } }) }
          };`, loader: 'js',
    }))
  } }],
})
let sequence = 0
async function fresh() {
  return import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text + `\n// ${sequence++}`).toString('base64')}`)
}

test('startup is gated; fresh settings enable every app including Zen and View', async () => {
  const m = await fresh()
  assert.equal(m.isRouteEnabled('/notes'), false)
  assert.equal(m.isRouteEnabled('/'), true)
  await m.loadAppAvailability()
  assert.equal(m.appAvailability.ready.value, true)
  for (const app of m.apps) {
    assert.equal(m.isAppEnabled(app.id), true, app.id)
  }
  assert.equal(m.isRouteEnabled('/notes'), true)
  assert.equal(m.isRouteEnabled('/algo/manual'), true)
  assert.deepEqual(m.mock.writes, [])
})

test('manual app switches override defaults and survive a fresh frontend session', async () => {
  const m = await fresh()
  await m.loadAppAvailability()
  await m.setAppEnabled('quantzen', false)
  await m.setAppEnabled('quantview', true)
  await m.setAppEnabled('quantspace', false)
  const restarted = await fresh()
  restarted.mock.settings = JSON.parse(JSON.stringify(m.mock.settings))
  await restarted.loadAppAvailability()
  assert.equal(restarted.isRouteEnabled('/notes'), false)
  assert.equal(restarted.isRouteEnabled('/systems/id'), true)
  assert.equal(restarted.isAppEnabled('quantspace'), false)
  assert.equal(restarted.isAppEnabled('quantagent'), true)
  await restarted.setAppEnabled('quantzen', true)
  await restarted.loadAppAvailability()
  assert.equal(restarted.isRouteEnabled('/notes'), true)
  assert.equal(restarted.isRouteEnabled('/algo/manual'), true)
})

test('stored switches cover app members, legacy routes, palettes and settings', async () => {
  const m = await fresh()
  m.mock.settings = { 'apps.enabled.quantzen': false, 'apps.enabled.quantview': false }
  m.registerPaletteActions('notes', [{ id: 'notes.create', group: 'Notes', label: 'New note' }])
  m.registerPaletteActions('console', [{ id: 'console.open', group: 'Console', label: 'Open console' }])
  m.registerSettingsSections('algo', [{ id: 'general', label: 'General' }])
  m.registerSettingsSections('suite', [{ id: 'apps', label: 'Apps' }])
  await m.loadAppAvailability()
  for (const route of ['/notes', '/flow/habits', '/finance', '/plan', '/habit', '/systems/id', '/algo/manual', '/script/forge?tab=reports', '/terminal']) {
    assert.equal(m.isRouteEnabled(route), false, route)
  }
  for (const route of ['/', '/processes', '/mcp', '/memory', '/pilot', '/console', '/canvas', '/code']) {
    assert.equal(m.isRouteEnabled(route), true, route)
  }
  assert.deepEqual(m.paletteActions().map((a) => a.id), ['console.open'])
  assert.deepEqual(m.settingsSectionsFor('algo'), [])
  assert.equal(m.settingsSectionsFor('suite').length, 1)
  await m.setAppEnabled('quantview', true)
  assert.equal(m.settingsSectionsFor('algo').length, 1)
  assert.equal(m.isRouteEnabled('/systems/id'), true)
})

test('writes persist individual switches; failure does not pretend to change state', async () => {
  const m = await fresh()
  await m.loadAppAvailability()
  await m.setAppEnabled('quantzen', false)
  await m.setAppEnabled('quantview', false)
  assert.deepEqual(m.mock.writes, [
    { scope: 'core', key: 'apps.enabled.quantzen', value: false },
    { scope: 'core', key: 'apps.enabled.quantview', value: false },
  ])
  m.mock.write = async () => { throw new Error('disk full') }
  await assert.rejects(m.setAppEnabled('quantzen', true), /disk full/)
  assert.equal(m.isAppEnabled('quantzen'), false)
  await m.loadAppAvailability()
  assert.equal(m.isAppEnabled('quantview'), false)
  assert.equal(m.isAppEnabled('quantagent'), true)
  await assert.rejects(m.setAppEnabled('dashboard', false), /Unknown app/)
  await assert.rejects(m.setAppEnabled('unknown', false), /Unknown app/)
})

test('failed startup remains gated and can be retried', async () => {
  const m = await fresh()
  m.mock.read = async () => { throw new Error('database unavailable') }
  await m.loadAppAvailability()
  assert.equal(m.appAvailability.ready.value, false)
  assert.match(m.appAvailability.error.value, /database unavailable/)
  await assert.rejects(m.setAppEnabled('quantzen', false), /not loaded/)
  m.mock.read = async () => ({ 'apps.enabled.quantzen': false })
  await m.loadAppAvailability()
  assert.equal(m.isAppEnabled('quantzen'), false)
  assert.equal(m.isAppEnabled('quantagent'), true)
  assert.equal(m.appAvailability.error.value, null)
})

test('cross-window changes win over stale reads without losing unrelated saved switches', async () => {
  const m = await fresh()
  let finishRead
  m.mock.read = () => new Promise((resolve) => { finishRead = resolve })
  const loading = m.loadAppAvailability()
  m.mock.event('core', 'apps.enabled.quantzen', false)
  m.mock.event('notes', 'apps.enabled.quantagent', false)
  m.mock.event('core', 'apps.enabled.quantspace', 'false')
  finishRead({ 'apps.enabled.quantzen': true, 'apps.enabled.quantview': false })
  await loading
  assert.equal(m.isAppEnabled('quantzen'), false)
  assert.equal(m.isAppEnabled('quantview'), false)
  assert.equal(m.isAppEnabled('quantagent'), true)
  assert.equal(m.isAppEnabled('quantspace'), true)
  m.mock.event('core', 'apps.enabled.quantzen', true)
  assert.equal(m.isRouteEnabled('/notes'), true)
})
