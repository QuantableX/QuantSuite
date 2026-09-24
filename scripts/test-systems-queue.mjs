/** Exercise the real Pinia stores and shared queue with deferred engine RPC. */
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { registerHooks } from 'node:module'
import { createPinia, setActivePinia } from 'pinia'
import { ref, reactive, computed } from 'vue'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('#systems/')) {
      return nextResolve(new URL(`../modules/systems/app/${specifier.slice(9)}.ts`, import.meta.url).href, context)
    }
    return nextResolve(specifier, context)
  },
})
Object.assign(globalThis, { ref, reactive, computed, window: {} })
const { useBacktestStore } = await import('../modules/systems/app/stores/backtest.ts')
const { useLiveStore } = await import('../modules/systems/app/stores/live.ts')
const { useSystemsStore } = await import('../modules/systems/app/stores/systems.ts')
const { useConfigStore } = await import('../modules/systems/app/stores/config.ts')
const { findIndicatorOption, indicatorOptionRows } = await import('../modules/systems/app/utils/indicatorOptions.ts')
const { strategyIcon } = await import('../modules/systems/app/utils/strategyIcons.ts')

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
const tick = () => new Promise(resolve => setImmediate(resolve))
const result = { strategies: [], notes: [] }

test('strategy symbols retain legacy defaults and safely handle unknown saved keys', () => {
  assert.equal(strategyIcon({ id: 'lces' }).id, 'layers')
  assert.equal(strategyIcon({ id: 'sces' }).id, 'hexagon')
  assert.equal(strategyIcon({ id: 'custom' }).id, 'grid')
  assert.equal(strategyIcon({ id: 'lces', icon: 'diamond' }).id, 'diamond')
  assert.equal(strategyIcon({ id: 'custom', icon: '<svg>unknown</svg>' }).id, 'grid')
})

test('strategy creation and save send the chosen symbol to persistence', async t => {
  const calls = []
  setup(t, (command, args) => {
    calls.push([command, args])
    return command.endsWith('|create_system') ? { id: 'copy', ...args } : args.config
  })
  const systems = useSystemsStore()
  const copy = await systems.create('Copy', '', '', { topN: 17 }, 'target')
  assert.equal(copy.icon, 'target')
  assert.equal(calls[0][1].icon, 'target')
  const config = useConfigStore()
  config.update('copy', { topN: 17 })
  assert.equal(await config.save('copy', { ...copy, icon: 'wave' }), true)
  assert.equal(calls[1][1].metadata.icon, 'wave')
})

test('an empty catalog stays empty and failed mutations do not change the list', async t => {
  setup(t, command => {
    if (command.endsWith('|list_systems')) return []
    throw new Error('Disk is read-only')
  })
  const systems = useSystemsStore()
  await systems.load()
  assert.equal(systems.loaded, true)
  assert.deepEqual(systems.systems, [])
  await assert.rejects(systems.create('My strategy', '', ''), /read-only/)
  assert.deepEqual(systems.systems, [])
  systems.systems.push({ id: 'custom', name: 'My strategy', short: '', status: 'ready', description: '' })
  await assert.rejects(systems.remove('custom'), /read-only/)
  assert.equal(systems.byId('custom').name, 'My strategy')
})

test('strategy config loading is shared and navigation preserves edits', async t => {
  const job = deferred()
  let calls = 0
  setup(t, () => { calls++; return job.promise })
  const config = useConfigStore()
  const a = config.load('custom')
  const b = config.load('custom')
  assert.equal(calls, 1)
  job.resolve({ topN: 23, indicator: { trend: 'aggregate', aggregate: ['one', 'two'] } })
  await Promise.all([a, b])
  config.update('custom', { topN: 11, endDate: '2024-01-01' })
  await config.load('custom')
  assert.equal(config.get('custom').topN, 11)
  assert.equal(config.get('custom').endDate, '2024-01-01')
  assert.equal(calls, 1)
})

test('failed saves expose the error and keep the edited config', async t => {
  setup(t, () => { throw new Error('Disk is full') })
  const config = useConfigStore()
  config.update('custom', { topN: 17 })
  assert.equal(await config.save('custom'), false)
  assert.match(config.errors.custom, /Disk is full/)
  assert.equal(config.get('custom').topN, 17)
})

test('a custom strategy can run live and backtest with its own config', async t => {
  const calls = []
  const { live, backtest } = setup(t, (command, args) => {
    calls.push([command, args])
    return result
  })
  useConfigStore().update('custom-id', { topN: 19, excludeTopN: 8 })
  await live.refresh('custom-id')
  await backtest.run('custom-id')
  assert.equal(calls.length, 2)
  for (const [, args] of calls) {
    assert.equal(args.systemId, 'custom-id')
    assert.equal(args.config.topN, 19)
    assert.equal(args.config.excludeTopN, 8)
  }
})

function setup(t, invoke) {
  setActivePinia(createPinia())
  mockIPC(invoke)
  t.after(clearMocks)
  return { live: useLiveStore(), backtest: useBacktestStore() }
}

test('a backtest waiting for live is queued and cannot claim live progress', async t => {
  const liveJob = deferred()
  const backtestJob = deferred()
  const calls = []
  const { live, backtest } = setup(t, command => {
    calls.push(command)
    return command.endsWith('|live_eval') ? liveJob.promise : backtestJob.promise
  })
  const first = live.refresh('lces')
  await tick()
  const second = backtest.run('lces')
  await tick()
  assert.equal(calls.length, 1)
  assert.equal(backtest.runningSystemId, null)
  assert.match(backtest.stateFor('lces').progressLabel, /Queued/)
  backtest.setProgress('Wrong job', 0.9)
  assert.equal(backtest.stateFor('lces').progressValue, 0)
  liveJob.resolve({})
  await first
  await tick()
  assert.equal(live.runningSystemId, null)
  assert.equal(backtest.runningSystemId, 'lces')
  assert.equal(calls.length, 2)
  backtestJob.resolve(result)
  await second
  assert.equal(backtest.stateFor('lces').isRunning, false)
})

test('a failed live request releases the queue for the backtest', async t => {
  const job = deferred()
  const { live, backtest } = setup(t, command => command.endsWith('|live_eval') ? job.promise : result)
  const first = live.refresh('lces')
  await tick()
  const second = backtest.run('lces')
  job.reject('CoinGecko denied access (HTTP 401)')
  await Promise.all([first, second])
  assert.match(live.stateFor('lces').error, /401/)
  assert.equal(backtest.stateFor('lces').error, null)
  assert.deepEqual(backtest.stateFor('lces').result.strategies, [])
  assert.equal(backtest.runningSystemId, null)
})

test('duplicate backtest starts cannot enqueue the same system twice', async t => {
  const job = deferred()
  let calls = 0
  const { backtest } = setup(t, () => { calls++; return job.promise })
  const first = backtest.run('lces')
  const duplicate = backtest.run('lces')
  await tick()
  assert.equal(calls, 1)
  job.resolve(result)
  await Promise.all([first, duplicate])
  assert.equal(calls, 1)
})

test('duplicate live starts cannot enqueue the same system twice', async t => {
  const job = deferred()
  let calls = 0
  const { live } = setup(t, () => { calls++; return job.promise })
  const first = live.refresh('lces')
  const duplicate = live.refresh('lces')
  await tick()
  assert.equal(calls, 1)
  job.resolve({})
  await Promise.all([first, duplicate])
  assert.equal(calls, 1)
})

test('the pickers show one row per base indicator with its versions; the values stay version keys', () => {
  const verdict = score => ({ score, grade: score >= 80 ? 'A' : 'B', certified: score >= 70, source: 'release' })
  const catalog = [
    {
      key: 'dcl', name: 'DonchianLevels', role: 'standard', certification: null,
      versions: { standard: 'dcl', optimized: 'dcl_opt', optimized_1h: 'dcl_opt_1h', optimized_4h: null, optimized_1d: 'dcl_opt_1d' },
      timeframes: { '1d': verdict(71), '1h': verdict(60) },
    },
    { key: 'dcl_opt', name: 'DonchianLevels · Optimized', base_key: 'dcl', role: 'optimized', variant: { base_key: 'dcl', label: 'Optimized', status: 'release', role: 'optimized' }, certification: null, timeframes: { '1d': verdict(78) } },
    { key: 'dcl_opt_1h', name: 'DonchianLevels · Optimized 1H', base_key: 'dcl', role: 'optimized_1h', variant: { base_key: 'dcl', label: 'Optimized 1H', status: 'release', role: 'optimized_1h' }, certification: null, timeframes: { '1h': verdict(66) } },
    { key: 'dcl_opt_1d', name: 'DonchianLevels · Optimized 1D', base_key: 'dcl', role: 'optimized_1d', variant: { base_key: 'dcl', label: 'Optimized 1D', status: 'release', role: 'optimized_1d' }, certification: null, timeframes: { '1d': verdict(84) } },
    { key: 'dcl_fast', name: 'DonchianLevels · fast', base_key: 'dcl', role: null, variant: { base_key: 'dcl', label: 'fast', status: 'research' }, certification: null, timeframes: { '1d': verdict(50) } },
    { key: 'dmi', name: 'DMI', certification: null, timeframes: { '1d': verdict(74) } },
  ]

  const daily = indicatorOptionRows(catalog, '1d')
  assert.deepEqual(daily.map(o => o.name), ['DonchianLevels', 'DMI', 'DonchianLevels › fast'], 'versions fold into their base; research keys keep a row')
  const dcl = daily[0]
  assert.equal(dcl.value, 'dcl_opt_1d', 'a click on the row picks the version of the cadence track')
  assert.equal(dcl.score, 84)
  assert.deepEqual(dcl.versions.map(v => [v.value, v.short, v.matches]), [
    ['dcl', 'S', false], ['dcl_opt', 'G', false], ['dcl_opt_1h', '1H', false], ['dcl_opt_1d', '1D', true],
  ])
  assert.equal(daily[1].versions, undefined, 'an indicator without versions stays a plain row')
  assert.equal(dcl.versions[2].tag, 'no 1d verdict', 'a timeframe version is only measured on its own track')
  assert.equal(findIndicatorOption(daily, 'dcl_opt'), dcl, "LCES's stored key finds its base row")
  assert.equal(findIndicatorOption(daily, 'dcl_fast').name, 'DonchianLevels › fast')
  assert.equal(findIndicatorOption(daily, 'nope'), undefined)

  const fourHour = indicatorOptionRows(catalog, '4h')
  assert.equal(fourHour.find(o => o.name === 'DonchianLevels').value, 'dcl_opt', 'no 4h version: the general one')
  const twelveHour = indicatorOptionRows(catalog, '12h')
  assert.ok(twelveHour.find(o => o.name === 'DonchianLevels').versions.every(v => !v.matches), '12h has no track, so nothing is marked')
})
