// QuantPilot round 2, card 07: crews — what the panel, the gate bar and the log show.
// Run: node --test scripts/test-pilot-crew.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const crew = await import('../modules/pilot/app/utils/crew.ts')

const task = (key, status, extra = {}) => ({ id: `t-${key}`, crewId: 'c1', key, spec: `do ${key}`, deps: [], cardId: null, provider: 'claude', model: '', worktree: true, timeoutMin: 0, status, progress: 0, note: '', workerId: `w-${key}`, attempts: 1, summary: '', result: '', error: '', startedAt: 1, finishedAt: null, sort: 0, brief: '', ...extra })
const member = (threadId, role, taskId = null, attempt = 1) => ({ crewId: 'c1', threadId, taskId, role, attempt, status: 'active', provider: 'claude', joinedAt: 1 })
const msg = (id, kind, status = '', extra = {}) => ({ id, crewId: 'c1', taskId: null, fromId: 'orch', toId: 'user', kind, text: 'merge: May I merge billing?', replyTo: null, status, at: Date.UTC(2026, 10, 3, 12, 0, 5), n: 1, ...extra })
const snap = (tasks, members, open = [], createdAt = 1, orchestratorId = 'orch') => ({
  crew: { id: `c-${createdAt}`, orchestratorId, contextId: 'x', objective: 'Payments', status: 'running', maxParallel: 0, allowPeers: false, rev: 1, createdAt, updatedAt: 1 },
  tasks,
  members,
  messages: [],
  messageCount: 0,
  open,
  maxParallel: 4,
  titles: { orch: 'Lead thread' },
})

test('counts say done of all; tones and icons follow the state', () => {
  const tasks = [task('a', 'done'), task('b', 'running'), task('c', 'failed'), task('d', 'waiting'), task('e', 'stalled')]
  assert.deepEqual(crew.counts(tasks), { total: 5, done: 1, running: 1, failed: 1, open: 3 })
  assert.equal(crew.countLabel(tasks), '1/5')
  assert.equal(crew.statusTone('running'), 'run')
  assert.equal(crew.statusTone('stalled'), 'warn')
  assert.equal(crew.statusTone('failed'), 'bad')
  assert.equal(crew.statusTone('stopped'), 'mute')
  assert.equal(crew.iconStatus('asking'), 'waiting')
  assert.equal(crew.iconStatus('failed'), 'error')
  assert.equal(crew.iconStatus('done'), 'idle')
  assert.equal(crew.STATUS_LABEL.running, 'Working')
})

test('a thread finds the crew it leads, else the one it works in; workers leave the sidebar', () => {
  const old = snap([task('a', 'done')], [member('orch', 'orchestrator'), member('w-a', 'worker', 't-a')], [], 1)
  const fresh = snap([task('b', 'running')], [member('orch', 'orchestrator'), member('w-b', 'worker', 't-b')], [], 2)
  const other = snap([task('c', 'running')], [member('lead2', 'orchestrator'), member('w-c', 'worker', 't-c')], [], 3, 'lead2')
  const all = [old, fresh, other]
  assert.equal(crew.crewOfThread(all, 'orch').crew.id, 'c-2', 'the newest crew it leads')
  assert.equal(crew.crewOfThread(all, 'w-a').crew.id, 'c-1')
  assert.equal(crew.crewOfThread(all, 'w-c').crew.id, 'c-3')
  assert.equal(crew.crewOfThread(all, 'nobody'), null)
  assert.equal(crew.crewOfThread(all, null), null)
  assert.deepEqual([...crew.workerIds(all)].sort(), ['w-a', 'w-b', 'w-c'])
})

test('gates and questions come from the open requests, whole', () => {
  const s = snap([], [], [msg('g1', 'gate', 'pending'), msg('q1', 'ask', 'open'), msg('g0', 'gate', 'approved')])
  assert.deepEqual(crew.pendingGates(s).map((m) => m.id), ['g1'])
  assert.deepEqual(crew.openQuestions(s).map((m) => m.id), ['q1'])
  assert.deepEqual(crew.pendingGates(null), [])
  assert.deepEqual(crew.gateParts('merge: May I merge: billing?'), { name: 'merge', question: 'May I merge: billing?' })
  assert.deepEqual(crew.gateParts('deploy'), { name: 'deploy', question: '' })
})

test('the log names who said what to whom, attempts included', () => {
  const tasks = [task('billing', 'running', { workerId: 'w2' })]
  const s = snap(tasks, [member('orch', 'orchestrator'), member('w1', 'worker', 't-billing', 1), member('w2', 'worker', 't-billing', 2)])
  assert.equal(crew.party('user', s), 'You')
  assert.equal(crew.party('quantpilot', s), 'QuantPilot')
  assert.equal(crew.party('orch', s), 'Orchestrator')
  assert.equal(crew.party('w1', s), 'billing #1')
  assert.equal(crew.party('w2', s), 'billing #2')
  assert.equal(crew.party('stranger', s), 'a thread')
  assert.equal(crew.kindLabel('reply'), 'answered')
  assert.equal(crew.kindLabel('custom'), 'custom')
  const text = crew.logText([msg('m1', 'ask', 'open', { fromId: 'w2', toId: 'orch', text: 'Which table?' })], s)
  assert.match(text, /^\d\d:\d\d:\d\d {2}billing #2 → Orchestrator {2}\[asked\] {2}Which table\?$/)
})

test('a task line shows its note, or its outcome once settled', () => {
  assert.equal(crew.taskDetail(task('a', 'running', { note: 'tests pass' })), 'tests pass')
  assert.equal(crew.taskDetail(task('a', 'waiting', { note: 'waits for db' })), 'waits for db')
  assert.equal(crew.taskDetail(task('a', 'done', { summary: 'Shipped', note: 'x' })), 'Shipped')
  assert.equal(crew.taskDetail(task('a', 'failed', { summary: 'gave up', error: 'timed out after 60 min' })), 'timed out after 60 min')
  assert.ok(crew.settled('stopped') && !crew.settled('stalled'))
})
