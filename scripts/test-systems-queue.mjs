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

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
const tick = () => new Promise(resolve => setImmediate(resolve))
const result = { strategies: [], notes: [] }

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
