// QuantPilot round 3, card 03: scheduled prompts — the words for a rule and a time, switching kinds.
// Run: node --test scripts/test-pilot-schedules.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { describeRecurrence, whenLabel, inLabel, localStamp, switchKind, blankSchedule, runLabel } = await import('../modules/pilot/app/utils/schedules.ts')

test('a rule reads as a sentence', () => {
  assert.equal(describeRecurrence({ kind: 'daily', time: '08:00' }), 'Every day at 08:00')
  assert.equal(describeRecurrence({ kind: 'weekdays', time: '07:45' }), 'Weekdays at 07:45')
  assert.equal(describeRecurrence({ kind: 'days', days: [5, 1, 3], time: '18:30' }), 'Mon, Wed, Fri at 18:30')
  assert.equal(describeRecurrence({ kind: 'days', days: [1, 2, 3, 4, 5, 6, 7], time: '09:00' }), 'Every day at 09:00')
  assert.equal(describeRecurrence({ kind: 'hours', every: 6, from: '2026-11-04T08:00' }), 'Every 6 hours from Nov 4, 08:00')
  assert.equal(describeRecurrence({ kind: 'hours', every: 1, from: '2026-11-04T08:00' }), 'Every hour from Nov 4, 08:00')
  assert.equal(describeRecurrence({ kind: 'once', at: '2026-12-24T18:00' }), 'Once, Dec 24, 18:00')
})

test('times read relative to now', () => {
  const now = new Date(2026, 10, 4, 10, 0).getTime() // Wed Nov 4, 10:00 local
  assert.equal(whenLabel(new Date(2026, 10, 4, 18, 30).getTime(), now), 'Today 18:30')
  assert.equal(whenLabel(new Date(2026, 10, 5, 8, 0).getTime(), now), 'Tomorrow 08:00')
  assert.equal(whenLabel(new Date(2026, 10, 3, 8, 0).getTime(), now), 'Yesterday 08:00')
  assert.equal(whenLabel(new Date(2026, 10, 7, 8, 0).getTime(), now), 'Sat 08:00')
  assert.equal(whenLabel(new Date(2026, 10, 20, 8, 5).getTime(), now), 'Nov 20, 08:05')
  assert.equal(inLabel(now + 20_000, now), 'now')
  assert.equal(inLabel(now + 4 * 60_000, now), 'in 4m')
  assert.equal(inLabel(now + 200 * 60_000, now), 'in 3h 20m')
  assert.equal(inLabel(now + 3 * 3_600_000, now), 'in 3h')
  assert.equal(inLabel(now + 50 * 3_600_000, now), 'in 2d')
  assert.equal(localStamp(new Date(2026, 0, 2, 3, 4).getTime()), '2026-01-02T03:04')
})

test('switching the kind keeps the time the user set', () => {
  const now = new Date(2026, 10, 4, 10, 0).getTime()
  assert.deepEqual(switchKind({ kind: 'daily', time: '08:15' }, 'weekdays', now), { kind: 'weekdays', time: '08:15' })
  assert.deepEqual(switchKind({ kind: 'daily', time: '08:15' }, 'days', now), { kind: 'days', days: [1, 2, 3, 4, 5], time: '08:15' })
  // 08:15 has passed today, so a one-off goes to tomorrow; 18:00 is still today.
  assert.deepEqual(switchKind({ kind: 'daily', time: '08:15' }, 'once', now), { kind: 'once', at: '2026-11-05T08:15' })
  assert.deepEqual(switchKind({ kind: 'weekdays', time: '18:00' }, 'hours', now), { kind: 'hours', every: 6, from: '2026-11-04T18:00' })
  assert.deepEqual(switchKind({ kind: 'once', at: '2026-11-09T07:30' }, 'daily', now), { kind: 'daily', time: '07:30' })
})

test('a new schedule starts on weekdays at nine, skipping missed runs, a new thread each time', () => {
  const s = blankSchedule({ contextId: 'core:workspace:abc', prompt: 'Review' })
  assert.equal(s.id, '')
  assert.equal(s.contextId, 'core:workspace:abc')
  assert.deepEqual(s.recurrence, { kind: 'weekdays', time: '09:00' })
  assert.equal(s.missed, 'skip')
  assert.equal(s.target, 'new')
  assert.equal(s.enabled, true)
  assert.equal(runLabel('started', true), 'Ran (run now)')
  assert.equal(runLabel('missed', false), 'Missed')
})

test('a typed time becomes 24-hour HH:MM', async () => {
  const { normalizeTime } = await import('../modules/pilot/app/utils/schedules.ts')
  assert.equal(normalizeTime('8'), '08:00')
  assert.equal(normalizeTime('830'), '08:30')
  assert.equal(normalizeTime('8:5'), '08:05')
  assert.equal(normalizeTime('18.45'), '18:45')
  assert.equal(normalizeTime(' 23:59 '), '23:59')
  assert.equal(normalizeTime('24:00'), null)
  assert.equal(normalizeTime('7:60'), null)
  assert.equal(normalizeTime('soon'), null)
})
