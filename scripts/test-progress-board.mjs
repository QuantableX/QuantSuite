// Agent status: the one board template both hosts render (docs/PLAN-AGENT-STATUS.md).
// Run: node --test scripts/test-progress-board.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { renderBoard, formatDuration, percentText, timeLine } = await import('../packages/ui/src/progress/board.js')

const NOW = 1_800_000_000_000
const MIN = 60_000

const task = (over = {}) => ({
  id: 'g',
  title: 'Gauntlet LCES Top 6',
  state: 'running',
  startedAt: NOW - 70 * MIN,
  updatedAt: NOW - MIN,
  stale: false,
  overall: 0.64,
  totalSecs: 110 * 60,
  step: 4,
  steps: 7,
  stepText: 'Monte-Carlo',
  ...over,
})

const lane = (over = {}) => ({
  key: 'agent:claude-code-1',
  agent: 'claude-code-1',
  place: 'QuantSuite · worktree 84ac3068',
  folder: 'C:/Projects/QuantSuite/.qs-worktrees/84ac3068',
  tasks: [task()],
  shells: [],
  updatedAt: NOW,
  ...over,
})

test('durations read like the mockup: 45s, 12m, 1h 10m, 14h', () => {
  assert.equal(formatDuration(45_000), '45s')
  assert.equal(formatDuration(12 * MIN), '12m')
  assert.equal(formatDuration(70 * MIN), '1h 10m')
  assert.equal(formatDuration(180 * MIN), '3h')
  assert.equal(formatDuration(14 * 60 * MIN + 5 * MIN), '14h')
  assert.equal(formatDuration(-5), '0s')
})

test('a running bar never claims 100 %', () => {
  assert.equal(percentText(0.999), '99%')
  assert.equal(percentText(1.4), '100%')
  assert.equal(percentText(0.64), '64%')
})

test('time is elapsed / ~total while running, what it took when finished', () => {
  assert.equal(timeLine(task(), NOW), '1h 10m / ~1h 50m')
  assert.equal(timeLine(task({ totalSecs: null }), NOW), '1h 10m')
  assert.equal(timeLine(task({ state: 'done', finishedAt: NOW - 47 * MIN }), NOW), 'done in 23m')
  assert.equal(timeLine(task({ state: 'failed', finishedAt: NOW - 65 * MIN }), NOW), 'failed after 5m')
})

test('a lane shows agent, place, the bar with step and time, and its shells', () => {
  const html = renderBoard({
    now: NOW,
    lanes: [
      lane({
        shells: [
          { id: 'a', command: 'python run_gauntlet.py --top 6', background: true, startedAt: NOW - 38 * MIN, failed: false },
          { id: 'b', command: 'cargo test', background: false, startedAt: NOW - 3 * MIN, endedAt: NOW - 2 * MIN, failed: true },
        ],
      }),
    ],
  })
  for (const part of ['claude-code-1', 'QuantSuite · worktree 84ac3068', 'Gauntlet LCES Top 6', '64%', 'width:64.0%', 'Step 4/7 · Monte-Carlo', '1h 10m / ~1h 50m', 'python run_gauntlet.py --top 6', 'background 38m', 'failed · 1m']) {
    assert.ok(html.includes(part), `missing ${part}`)
  }
  assert.ok(html.includes('qpb-shell is-background'))
  assert.ok(html.includes('qpb-shell is-failed'))
})

test('unknown progress is an indeterminate bar; stale, done and failed are marked', () => {
  assert.ok(renderBoard({ now: NOW, lanes: [lane({ tasks: [task({ overall: null })] })] }).includes('qpb-bar is-indeterminate'))
  const stale = renderBoard({ now: NOW, lanes: [lane({ tasks: [task({ stale: true, updatedAt: NOW - 20 * MIN })] })] })
  assert.ok(stale.includes('qpb-task is-stale'))
  assert.ok(stale.includes('no update for 20m'))
  const done = renderBoard({ now: NOW, lanes: [lane({ tasks: [task({ state: 'done', overall: 1, finishedAt: NOW, note: '12 of 12 passed' })] })] })
  assert.ok(done.includes('qpb-task is-done') && done.includes('>Done<') && done.includes('12 of 12 passed'))
  const failed = renderBoard({ now: NOW, lanes: [lane({ tasks: [task({ state: 'failed', overall: null, finishedAt: NOW })] })] })
  assert.ok(failed.includes('qpb-task is-failed') && failed.includes('width:100.0%'))
})

test('every value is escaped', () => {
  const html = renderBoard({
    now: NOW,
    lanes: [lane({ agent: '<img src=x onerror=alert(1)>', tasks: [task({ title: '"><script>x</script>', detail: "a'b&c" })] })],
  })
  assert.ok(!html.includes('<img') && !html.includes('<script>'))
  assert.ok(html.includes('&lt;img src=x onerror=alert(1)&gt;'))
  assert.ok(html.includes('a&#39;b&amp;c'))
})

test('an empty board says so, in the host\'s words when given', () => {
  assert.ok(renderBoard({ now: NOW, lanes: [] }).includes('No agent is reporting work right now.'))
  assert.ok(renderBoard(null, { empty: 'Nothing yet' }).includes('Nothing yet'))
})
