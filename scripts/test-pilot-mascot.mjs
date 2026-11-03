// QuantPilot round 2, card 05: the mascot — moods, lines, preferences, gaze.
// Run: node --test scripts/test-pilot-mascot.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const m = await import('../modules/pilot/app/utils/mascot.ts')

test('states map to moods; events win; long runs tire it and busy crews make it juggle', () => {
  assert.equal(m.moodOf({ state: 'offline' }), 'sleep')
  assert.equal(m.moodOf({ state: 'idle' }), 'calm')
  assert.equal(m.moodOf({ state: 'thinking' }), 'think')
  assert.equal(m.moodOf({ state: 'working' }), 'focus')
  assert.equal(m.moodOf({ state: 'working', busy: m.JUGGLE_AT }), 'juggle')
  assert.equal(m.moodOf({ state: 'working', busy: 9, elapsedMs: m.LONG_RUN_MS }), 'tired', 'a long run outranks juggling')
  assert.equal(m.moodOf({ state: 'waiting' }), 'call')
  assert.equal(m.moodOf({ state: 'error' }), 'worried')
  assert.equal(m.moodOf({ state: 'idle', cheering: true }), 'cheer')
  assert.equal(m.moodOf({ state: 'working', nodding: true }), 'nod')
  assert.equal(m.moodOf({ state: 'error', cheering: true }), 'worried', 'no cheering over an error')
  assert.deepEqual(new Set(m.MOODS), new Set(Object.keys(m.MOOD_LABEL)))
})

test('every mood has lines, said in the name of the mascot', () => {
  for (const mood of m.MOODS) assert.ok(m.LINES[mood].length >= 2, mood)
  assert.equal(m.lineFor('calm', 'Pip', 0), 'Pip: Ready when you are.')
  assert.equal(m.lineFor('calm', '  ', 1), 'What are we building?')
  assert.equal(m.lineFor('cheer', 'Pip', 5), m.lineFor('cheer', 'Pip', 1), 'the seed picks among the lines')
})

test('preferences are checked: unknown values fall back, the name is trimmed, the companion stays on screen', () => {
  assert.deepEqual(m.normalizePrefs(undefined), m.DEFAULT_PREFS)
  const p = m.normalizePrefs({ on: false, name: '  Biscuit the Very Long Named Blob  ', skin: 'mint', eyes: 'laser', accessory: 'cap', size: 'huge', agentTint: true, companion: 'yes', companionAt: { x: 1.4, y: -2 } })
  assert.equal(p.on, false)
  assert.equal(p.name, 'Biscuit the Very Long Na')
  assert.equal(p.skin, 'mint')
  assert.equal(p.eyes, 'dots')
  assert.equal(p.accessory, 'cap')
  assert.equal(p.size, 'medium')
  assert.equal(p.agentTint, true)
  assert.equal(p.companion, false)
  assert.deepEqual(p.companionAt, { x: 1, y: 0 })
  assert.equal(m.agentTint('claude'), '#d97757')
  assert.equal(m.agentTint('nobody'), null)
  assert.equal(m.agentTint(null), null)
})

test('the eyes turn toward the pointer, a little when close, fully when far', () => {
  assert.deepEqual(m.gazeToward(0, 0), { x: 0, y: 0 })
  const far = m.gazeToward(1000, 0)
  assert.ok(Math.abs(far.x - 0.06) < 1e-9 && far.y === 0)
  const near = m.gazeToward(52, 0)
  assert.ok(near.x > 0 && near.x < far.x)
  const up = m.gazeToward(0, -1000)
  assert.ok(up.y < 0 && Math.abs(up.y) < Math.abs(far.x), 'vertical travel is a little shorter')
})
