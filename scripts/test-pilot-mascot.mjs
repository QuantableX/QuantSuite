// QuantPilot: Bob The Blob, the mascot — moods, lines, preferences, gaze, quirks and gestures.
// Run: node --test scripts/test-pilot-mascot.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const m = await import('../modules/pilot/app/utils/mascot.ts')

test('states map to moods; events win; long runs tire him, busy crews make him juggle, quiet idles make him nap', () => {
  assert.equal(m.moodOf({ state: 'offline' }), 'sleep')
  assert.equal(m.moodOf({ state: 'idle' }), 'calm')
  assert.equal(m.moodOf({ state: 'idle', napping: true }), 'sleep')
  assert.equal(m.moodOf({ state: 'working', napping: true }), 'focus', 'a nap is for idle time only')
  assert.equal(m.moodOf({ state: 'thinking' }), 'think')
  assert.equal(m.moodOf({ state: 'working' }), 'focus')
  assert.equal(m.moodOf({ state: 'working', busy: m.JUGGLE_AT }), 'juggle')
  assert.equal(m.moodOf({ state: 'working', busy: 9, elapsedMs: m.LONG_RUN_MS }), 'tired', 'a long run outranks juggling')
  assert.equal(m.moodOf({ state: 'waiting' }), 'call')
  assert.equal(m.moodOf({ state: 'error' }), 'worried')
  assert.equal(m.moodOf({ state: 'idle', cheering: true }), 'cheer')
  assert.equal(m.moodOf({ state: 'idle', napping: true, cheering: true }), 'cheer')
  assert.equal(m.moodOf({ state: 'working', nodding: true }), 'nod')
  assert.equal(m.moodOf({ state: 'error', cheering: true }), 'worried', 'no cheering over an error')
  assert.deepEqual(new Set(m.MOODS), new Set(Object.keys(m.MOOD_LABEL)))
})

test('Bob says his lines in his name, and greets by the time of day now and then', () => {
  for (const mood of m.MOODS) assert.ok(m.LINES[mood].length >= 3, mood)
  assert.equal(m.MASCOT_NAME, 'Bob')
  assert.equal(m.MASCOT_FULL_NAME, 'Bob The Blob')
  assert.equal(m.lineFor('calm', 'Bob', 0), 'Bob: Ready when you are.')
  assert.equal(m.lineFor('calm', '  ', 1), 'What are we building?')
  assert.equal(m.lineFor('cheer', 'Bob', 4), m.lineFor('cheer', 'Bob', 1), 'the seed picks among the lines')
  assert.equal(m.lineFor('calm', 'Bob', 3, 8), 'Bob: Morning! What are we building?')
  assert.equal(m.lineFor('calm', 'Bob', 3, 14), 'Bob: Afternoon — ready when you are.')
  assert.equal(m.lineFor('calm', 'Bob', 3, 19), 'Bob: Evening shift. What is next?')
  assert.equal(m.lineFor('calm', 'Bob', 3, 2), 'Bob: Late one, huh? I am here.')
  assert.equal(m.lineFor('calm', 'Bob', 2, 8), "Bob: I'm all eyes.", 'only every fourth seed greets')
  assert.equal(m.lineFor('focus', 'Bob', 3, 8), 'Bob: Heads down.', 'only a calm Bob greets')
})

test('preferences are checked: unknown values fall back, the old default name becomes Bob, the companion stays on screen', () => {
  assert.deepEqual(m.normalizePrefs(undefined), m.DEFAULT_PREFS)
  assert.equal(m.DEFAULT_PREFS.name, 'Bob')
  assert.equal(m.normalizePrefs({ name: 'Pip' }).name, 'Bob', 'the first version called him Pip')
  assert.equal(m.normalizePrefs({ name: 'Biscuit' }).name, 'Biscuit')
  assert.equal(m.normalizePrefs({ name: '' }).name, '', 'no name stays no name')
  const p = m.normalizePrefs({ on: false, name: '  Biscuit the Very Long Named Blob  ', skin: 'ghost', eyes: 'laser', accessory: 'crown', size: 'huge', agentTint: true, companion: 'yes', companionAt: { x: 1.4, y: -2 } })
  assert.equal(p.on, false)
  assert.equal(p.name, 'Biscuit the Very Long Na')
  assert.equal(p.skin, 'ghost')
  assert.equal(p.eyes, 'dots')
  assert.equal(p.accessory, 'crown')
  assert.equal(p.size, 'medium')
  assert.equal(p.agentTint, true)
  assert.equal(p.companion, false)
  assert.deepEqual(p.companionAt, { x: 1, y: 0 })
  assert.equal(m.agentTint('claude'), '#d97757')
  assert.equal(m.agentTint('nobody'), null)
  assert.equal(m.agentTint(null), null)
  for (const s of ['ghost', 'rose', 'ocean']) assert.ok(m.SKINS.some((k) => k.id === s), s)
  for (const a of ['crown', 'sprout', 'party', 'beanie']) assert.ok(m.ACCESSORIES.some((k) => k.id === a), a)
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

test('idle quirks: weighted, never the same twice in a row, nodding off only after a long quiet spell', () => {
  let last = null
  for (let i = 0; i < 400; i++) {
    const q = m.pickQuirk(last, 0)
    assert.notEqual(q, last, 'never the same quirk twice in a row')
    assert.notEqual(q, 'drowsy', 'no nodding off while the user is around')
    last = q
  }
  const seen = new Set()
  for (let i = 0; i < 4000; i++) seen.add(m.pickQuirk(null, m.DROWSY_AFTER_MS))
  assert.deepEqual([...seen].sort(), Object.keys(m.QUIRKS).sort(), 'after a quiet spell every quirk comes up, drowsy included')
  assert.equal(m.pickQuirk('look', 0, () => 0), 'roll', 'the lowest draw takes the first quirk that is not the last one')
  assert.equal(m.quirkPause(() => 0), 2800)
  assert.equal(m.quirkPause(() => 1), 7000)
  for (const q of Object.values(m.QUIRKS)) assert.ok(q.ms >= 600 && q.ms <= 4500)
  assert.deepEqual(Object.keys(m.QUIRK_LABEL).sort(), Object.keys(m.QUIRKS).sort(), 'every quirk is a trick in Settings')
})

test('movement levels: Lively by default; Calm is rarer and gentler; Still is a choice', () => {
  assert.equal(m.DEFAULT_PREFS.motion, 'lively')
  assert.equal(m.normalizePrefs({ motion: 'still' }).motion, 'still')
  assert.equal(m.normalizePrefs({ motion: 'calm' }).motion, 'calm')
  assert.equal(m.normalizePrefs({ motion: 'wild' }).motion, 'lively')
  for (let i = 0; i < 400; i++) assert.ok(!m.QUIRKS[m.pickQuirk(null, m.DROWSY_AFTER_MS, Math.random, 'calm')].big, 'no big quirks when Calm')
  assert.ok(m.quirkPause(() => 0, 'calm') >= 9000)
  assert.ok(m.quirkPause(() => 1, 'lively') <= 7000)
})

test('four quick clicks make him dizzy; slow ones do not', () => {
  const t = 10_000
  assert.equal(m.dizzyFrom([t - 1200, t - 800, t - 400, t], t), true)
  assert.equal(m.dizzyFrom([t - 900, t - 400, t], t), false, 'three are not enough')
  assert.equal(m.dizzyFrom([t - 3000, t - 2000, t - 1000, t], t), false, 'too slow')
})

test('stroking back and forth is petting; moving past him is not', () => {
  assert.equal(m.isPetting([0, 10, 20, 10, 0, 10, 20, 10]), true)
  assert.equal(m.isPetting([0, 10, 20, 30, 40, 50, 60]), false, 'a pass in one direction')
  assert.equal(m.isPetting([0, 2, 0, 2, 0, 2, 0]), false, 'jitter is not a stroke')
  assert.equal(m.isPetting([0, 12, 0, 12]), false, 'two turns are not enough')
})
