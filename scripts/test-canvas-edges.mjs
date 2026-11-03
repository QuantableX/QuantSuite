// QuantCanvas, round 2 card 09: connections between windows — anchors, curves, states, crew clusters.
// Run: node --test scripts/test-canvas-edges.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const e = await import('../modules/canvas/app/utils/edges.ts')

const R = (x, y, width = 200, height = 100) => ({ x, y, width, height })
const inside = (p, r) => p.x > r.x + 0.5 && p.x < r.x + r.width - 0.5 && p.y > r.y + 0.5 && p.y < r.y + r.height - 0.5

test('an edge leaves and enters on the sides that face each other', () => {
  const a = R(0, 0)
  const right = e.anchors(a, R(400, 20))
  assert.deepEqual([right.p0, right.p3, right.n0, right.n3], [{ x: 200, y: 50 }, { x: 400, y: 70 }, { x: 1, y: 0 }, { x: -1, y: 0 }])
  const left = e.anchors(a, R(-500, 0))
  assert.deepEqual([left.p0.x, left.p3.x], [0, -300])
  const below = e.anchors(a, R(20, 300))
  assert.deepEqual([below.p0, below.p3], [{ x: 100, y: 100 }, { x: 120, y: 300 }])
  const above = e.anchors(a, R(0, -400))
  assert.deepEqual([above.p0.y, above.p3.y], [0, -300])
  // Wide windows side by side with a small vertical offset stay horizontal.
  assert.equal(e.anchors(R(0, 0, 600, 400), R(700, 300, 600, 400)).n0.x, 1)
})

test('the curve stays out of both windows and its points run from one to the other', () => {
  for (const [a, b] of [[R(0, 0), R(400, 200)], [R(0, 0), R(-50, 300)], [R(100, 100), R(-400, -300)], [R(0, 0, 520, 620), R(600, 0, 700, 520)]]) {
    const g = e.edgeGeometry(a, b)
    assert.match(g.d, /^M -?[\d.]+ -?[\d.]+ C /)
    assert.equal(g.points.length, 11)
    for (const p of g.points.slice(1, -1)) assert.ok(!inside(p, a) && !inside(p, b), JSON.stringify({ a, b, p }))
    assert.deepEqual(g.end, g.points.at(-1))
  }
})

test('edge states: threads, cards and crew tasks; the more pressing end wins', () => {
  assert.equal(e.threadState('working'), 'working')
  assert.equal(e.threadState('waiting'), 'waiting')
  assert.equal(e.threadState('limited'), 'error')
  assert.equal(e.threadState(undefined), 'idle')
  assert.equal(e.cardState({ column: 'plan', status: 'backlog' }), 'idle')
  assert.equal(e.cardState({ column: 'work', status: 'in_progress', agent_id: 'claude-code-1' }), 'working')
  assert.equal(e.cardState({ column: 'review', status: 'awaiting_review' }), 'waiting')
  assert.equal(e.cardState({ column: 'done', status: 'merged' }), 'done')
  assert.equal(e.cardState({ column: 'plan', status: 'rejected' }), 'error')
  assert.equal(e.cardState(null), 'idle')
  assert.deepEqual(['running', 'asking', 'stalled', 'failed', 'done', 'waiting'].map(e.taskState), ['working', 'waiting', 'waiting', 'error', 'done', 'idle'])
  assert.equal(e.worse('working', 'waiting'), 'waiting')
  assert.equal(e.worse('done', 'error'), 'error')
  assert.equal(e.worse('idle', 'done'), 'done')
})

test('a crew cluster: workers in two rows right of the orchestrator, never overlapping it or each other', () => {
  const orch = R(0, 0, 520, 620)
  const slots = e.clusterSlots(orch, 4)
  assert.equal(slots.length, 4)
  for (const s of slots) assert.ok(s.x >= orch.x + orch.width + 100)
  const hit = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
  for (let i = 0; i < slots.length; i++) for (let j = i + 1; j < slots.length; j++) assert.ok(!hit(slots[i], slots[j]), `${i}/${j}`)
  assert.deepEqual(e.clusterSlots(orch, 1)[0].y, Math.round(310 - 260), 'one worker sits level with the orchestrator')
  assert.deepEqual(e.boundsOf([R(10, 20), R(300, -40, 50, 50)]), { x: 10, y: -40, width: 340, height: 160 })
  assert.equal(e.boundsOf([]), null)
  assert.deepEqual(e.besideOf(R(0, 0), { width: 50, height: 60 }), { x: 280, y: 0, width: 50, height: 60 })
  assert.deepEqual(e.rectOf({ position: R(1, 2, 300, 400), minimized: true }), { x: 1, y: 2, width: 300, height: e.HEADER_H })
})
