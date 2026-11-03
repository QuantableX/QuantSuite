// QuantPilot round 2, card 08: the live orchestration graph — nodes, layout, edges, the bus.
// Run: node --test scripts/test-pilot-crewgraph.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const g = await import('../modules/pilot/app/utils/crewGraph.ts')

const task = (key, status, extra = {}) => ({ id: `t-${key}`, crewId: 'c1', key, spec: `Port ${key} to rust\nmore detail`, deps: [], cardId: null, provider: 'claude', model: '', worktree: true, timeoutMin: 0, status, progress: 40, note: '', workerId: `w-${key}`, attempts: 1, summary: '', result: '', error: '', startedAt: 1000, finishedAt: null, sort: 0, brief: '', ...extra })
const member = (threadId, role, taskId = null) => ({ crewId: 'c1', threadId, taskId, role, attempt: 1, status: 'active', provider: role === 'orchestrator' ? 'claude' : 'codex', joinedAt: 1 })
const msg = (n, kind, fromId, toId, taskId = null, text = 'x') => ({ id: `m-${n}`, crewId: 'c1', taskId, fromId, toId, kind, text, replyTo: null, status: '', at: Date.UTC(2026, 10, 3, 13, 59, 16, 907), n })
const KEYS = ['billing', 'queue', 'ledger', 'notify', 'db', 'search']
function fixture(statuses = ['running', 'running', 'asking', 'done', 'failed', 'waiting']) {
  const tasks = KEYS.map((k, i) => task(k, statuses[i]))
  return {
    crew: { id: 'c1', orchestratorId: 'orch', contextId: 'x', objective: 'porting to rust', status: 'running', maxParallel: 0, allowPeers: false, rev: 3, createdAt: 1, updatedAt: 1 },
    tasks,
    members: [member('orch', 'orchestrator'), ...tasks.map((t) => member(t.workerId, 'worker', t.id))],
    messages: [msg(1, 'dispatch', 'orch', 'w-billing', 't-billing'), msg(2, 'progress', 'w-queue', 'orch', 't-queue', '40% — tests green'), msg(3, 'ask', 'w-ledger', 'orch', 't-ledger', 'need a lock on schema.rs'), msg(4, 'gate', 'orch', 'user', null, 'merge: ok?')],
    messageCount: 42,
    open: [],
    maxParallel: 4,
    titles: { orch: 'Lead' },
  }
}

test('a crew becomes a center and one node per task, in states and colours', () => {
  const gr = g.crewGraph(fixture())
  assert.equal(gr.kind, 'crew')
  assert.equal(gr.center.sub, 'orchestrator · 6 workers · leading')
  assert.equal(gr.center.state, 'working')
  assert.deepEqual(gr.nodes.map((n) => n.state), ['working', 'working', 'waiting', 'done', 'error', 'idle'])
  assert.equal(gr.nodes[0].task, 'Port billing to rust')
  assert.equal(gr.nodes[3].progress, 100, 'done is 100%')
  assert.equal(gr.nodes[5].progress, null, 'a queued task has no progress yet')
  assert.equal(gr.nodes[1].latest, '40% — tests green', 'the hover shows the newest message')
  assert.equal(new Set(gr.nodes.map((n) => n.color)).size, 6)
  const settled = g.crewGraph(fixture(['done', 'done', 'done', 'done', 'failed', 'stopped']))
  assert.equal(settled.center.sub, 'orchestrator · 6 workers · finished')
  assert.equal(settled.center.state, 'done')
})

test('native subagents draw the same way', () => {
  const subs = [
    { callId: 'b', title: 'Second', type: 'Explore', status: 'running', tools: 3, last: 'Read a.ts', item: { createdAt: 5 } },
    { callId: 'a', title: 'First', type: '', status: 'completed', tools: 1, last: 'done', item: { createdAt: 1, updatedAt: 4 } },
  ]
  const gr = g.subagentGraph({ id: 't', title: 'Main', provider: 'claude' }, subs)
  assert.equal(gr.kind, 'subagents')
  assert.equal(gr.center.sub, '2 subagents · 1 working')
  assert.deepEqual(gr.nodes.map((n) => [n.name, n.state, n.task]), [['First', 'done', '1 tool'], ['Second', 'working', 'Explore · 3 tools']])
  assert.equal(gr.nodes[0].finishedAt, 4)
  assert.equal(gr.nodes[1].finishedAt, null)
})

test('up to twelve workers ring the center; more get further rings and a larger world', () => {
  const one = g.radialLayout(6, 1000, 600, 176, 78)
  assert.deepEqual([one.width, one.height], [1000, 600])
  assert.deepEqual(one.center, { x: 500, y: 300 })
  assert.ok(Math.abs(one.nodes[0].x - 500) < 0.001 && one.nodes[0].y < 300, 'the first worker sits on top')
  for (const p of one.nodes) assert.ok(p.x >= 88 && p.x <= 912 && p.y >= 39 && p.y <= 561, JSON.stringify(p))
  const big = g.radialLayout(30, 1000, 600, 176, 78)
  assert.equal(big.nodes.length, 30)
  assert.ok(big.width > 1000 && big.height > 600, 'three rings need more room than the stage')
  assert.ok(g.fitScale(big, { width: 1000, height: 600 }) < 1)
  assert.equal(g.fitScale(one, { width: 1000, height: 600 }), 1, 'never zoomed in past 1')
  const dist = (p) => Math.hypot((p.x - big.center.x) / big.width, (p.y - big.center.y) / big.height)
  assert.ok(dist(big.nodes[0]) < dist(big.nodes[29]), 'later workers sit further out')
})

test('no two workers overlap, nor a worker and the center, for 1 to 30 workers', () => {
  const W = 176, H = 78, CW = 232, CH = 70
  const hit = (a, b, aw, ah, bw, bh) => Math.abs(a.x - b.x) < (aw + bw) / 2 && Math.abs(a.y - b.y) < (ah + bh) / 2
  for (const [sw, sh] of [[1000, 480], [700, 400], [1600, 900]]) {
    for (let n = 1; n <= 30; n++) {
      const l = g.radialLayout(n, sw, sh, W, H, 18, CW, CH)
      for (let i = 0; i < n; i++) {
        assert.ok(!hit(l.nodes[i], l.center, W, H, CW, CH), `n=${n} node ${i} hits the center (${sw}x${sh})`)
        for (let j = i + 1; j < n; j++) assert.ok(!hit(l.nodes[i], l.nodes[j], W, H, W, H), `n=${n}: ${i} and ${j} overlap (${sw}x${sh})`)
      }
    }
  }
})

test('a message travels its worker edge, toward the orchestrator or out to the worker', () => {
  const s = fixture()
  assert.deepEqual(g.travelOf(s.messages[0], s), { node: 't-billing', inward: false })
  assert.deepEqual(g.travelOf(s.messages[2], s), { node: 't-ledger', inward: true })
  assert.deepEqual(g.travelOf(msg(9, 'escalation', 'quantpilot', 'orch', 't-db'), s), { node: 't-db', inward: true })
  assert.deepEqual(g.travelOf(s.messages[3], s), { center: true }, 'a gate only lights the orchestrator')
  assert.deepEqual(g.travelOf(msg(9, 'peer', 'w-db', 'w-search'), s), { node: 't-db', inward: true })
  assert.equal(g.travelOf(msg(9, 'note', 'stranger', 'other'), s), null)
})

test('the bus: time to the millisecond, a jump target per kind, the footer', () => {
  const s = fixture()
  assert.match(g.busTime(Date.UTC(2026, 10, 3, 13, 59, 16, 907)), /^\d\d:59:16\.907$/)
  assert.deepEqual(g.jumpTarget(msg(1, 'reply', 'orch', 'w-ledger', 't-ledger', 'banana'), s), { threadId: 'w-ledger', query: 'banana' })
  assert.deepEqual(g.jumpTarget(s.messages[0], s), { threadId: 'w-billing', query: 'Your task: billing' })
  assert.deepEqual(g.jumpTarget(s.messages[2], s), { threadId: 'orch', query: 'm-3' })
  assert.deepEqual(g.jumpTarget(s.messages[1], s), { threadId: 'w-queue', query: 'tests green' })
  assert.deepEqual(g.jumpTarget(msg(5, 'done', 'w-db', 'orch', 't-db'), s), { threadId: 'orch', query: '[Crew] db' })
  assert.deepEqual(g.jumpTarget(s.messages[3], s), { threadId: 'orch', query: 'merge' })
  assert.equal(g.footerLine(s), '7 agents on porting to rust · 42 messages on the bus')
  assert.equal(g.elapsed(65_000), '1m 05s')
  assert.equal(g.elapsed(3_725_000), '1h 02m')
  assert.equal(g.elapsed(-5), '0s')
})
