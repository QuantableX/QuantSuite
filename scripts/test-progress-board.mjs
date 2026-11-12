// Shared drawer and /progress behaviour. Run: npm run test:progress
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { renderBoard, formatDuration, percentText, timeLine } from '../packages/ui/src/progress/board.js'

const NOW = 1_800_000_000_000
const MIN = 60_000
const task = (over = {}) => ({
  id: 'g', title: 'Gauntlet LCES Top 6', state: 'running',
  startedAt: NOW - 70 * MIN, updatedAt: NOW - MIN, stale: false,
  overall: 0.64, totalSecs: 110 * 60, step: 4, steps: 7, stepText: 'Monte-Carlo', ...over,
})
const lane = (over = {}) => ({
  key: 'agent:claude-code-1', agent: 'claude-code-1',
  place: 'QuantSuite · worktree 84ac3068', folder: 'C:/Projects/QuantSuite/.qs-worktrees/84ac3068',
  tasks: [task()], shells: [], updatedAt: NOW, ...over,
})
const render = (over = {}) => renderBoard({ now: NOW, lanes: [lane({ tasks: [task(over)] })] })

test('duration formatting is bounded and readable', () => {
  for (const [ms, expected] of [[45_000, '45s'], [12 * MIN, '12m'], [70 * MIN, '1h 10m'], [180 * MIN, '3h'], [14 * 60 * MIN, '14h'], [-5, '0s']]) {
    assert.equal(formatDuration(ms), expected)
  }
  assert.equal(percentText(0.999), '99%')
  assert.equal(percentText(1.4), '100%')
})

test('elapsed time never uses the old extrapolated total', () => {
  assert.equal(timeLine(task(), NOW), '1h 10m elapsed')
  assert.equal(timeLine(task({ state: 'done', finishedAt: NOW - 47 * MIN }), NOW), 'Done in 23m')
  assert.equal(timeLine(task({ state: 'failed', finishedAt: NOW - 65 * MIN }), NOW), 'Failed after 5m')
  assert.doesNotMatch(render(), /1h 50m|~|Agent estimate/)
  assert.match(render({ estimateSecs: 60 * 60 }), /Agent estimate: 1h total · exceeded/)
  assert.match(render({ estimateSecs: 120 * 60 }), /Agent estimate: 2h total</)
})

test('step counts do not masquerade as overall work percentages', () => {
  const html = render({ step: 3, steps: 3, overall: 2 / 3 })
  assert.match(html, /Step 3 of 3/)
  assert.match(html, /Steps completed · current step unmeasured/)
  assert.match(html, />2\/3</)
  assert.match(html, /is-steps/)
  assert.doesNotMatch(html, />66%|>67%/)
})

test('current-step log progress takes precedence over agent estimates', () => {
  const html = render({ stepFraction: 0.7, followFraction: 17 / 30, overall: 0.7833 })
  assert.match(html, /Current step · from log/)
  assert.match(html, />56%</)
  assert.match(html, /width:56.7%/)
  assert.doesNotMatch(html, />78%|>70%/)
  assert.match(render({ stepFraction: 0.7 }), /Current step · agent estimate/)
  assert.match(render({ steps: null, step: null, percent: 42 }), /Progress · agent estimate/)
  assert.match(render({ steps: null, step: null, percent: 42 }), />42%</)
})

test('unknown and invalid values stay indeterminate, completion requires done', () => {
  for (const extra of [{}, { percent: NaN }, { followFraction: Infinity }]) {
    const html = render({ step: null, steps: null, ...extra })
    assert.match(html, /is-indeterminate/)
    assert.doesNotMatch(html, /aria-valuenow|NaN|Infinity/)
  }
  const running = render({ followFraction: 1 })
  assert.match(running, />100%</)
  assert.match(running, /awaiting completion/)
  assert.match(running, />Running</)
  assert.doesNotMatch(running, /qpb-task is-done|>Completed</)
  const done = render({ state: 'done', finishedAt: NOW, note: '12 of 12 passed' })
  assert.match(done, />100%</)
  assert.match(done, /Completed/)
  assert.match(done, /12 of 12 passed/)
  assert.doesNotMatch(done, /awaiting completion|No recent update/)
})

test('old task updates remain visibly stale even against an older server', () => {
  const html = render({ stale: false, updatedAt: NOW - 20 * MIN })
  assert.match(html, /qpb-task is-stale/)
  assert.match(html, /No update for 20m/)
  assert.match(html, /1 awaiting update/)
  const following = render({ updatedAt: NOW - 20 * MIN, followChangedAt: NOW - MIN })
  assert.doesNotMatch(following, /is-stale/)
  assert.match(following, /Updated 1m ago/)
  assert.match(render({ state: 'failed', finishedAt: NOW }), /qpb-task is-failed/)
})

test('commands and raw logs are inside a closed technical disclosure', () => {
  const html = renderBoard({ now: NOW, lanes: [lane({
    tasks: [task({ detail: 'PROGRESS 12/48 window 12' })],
    shells: [{ id: 'a', command: 'python run_gauntlet.py --top 6', background: true, startedAt: NOW - 38 * MIN, failed: false }],
  })] })
  assert.match(html, /<details class="qpb-technical" data-qpb-key="lane:agent:claude-code-1">/)
  const disclosure = html.slice(html.indexOf('<details'), html.indexOf('</details>'))
  assert.match(disclosure, /python run_gauntlet.py --top 6/)
  assert.match(disclosure, /PROGRESS 12\/48 window 12/)
  assert.match(disclosure, /Background · 38m/)
  assert.doesNotMatch(disclosure, /<details[^>]*\bopen/)
})

test('shell-only sessions do not occupy task cards in the primary view', () => {
  const html = renderBoard({ now: NOW, lanes: [lane({ tasks: [], shells: [{ id: 'b', command: 'cargo test', startedAt: NOW }] })] })
  assert.match(html, /No agent is reporting work right now/)
  assert.match(html, /<details class="qpb-background" data-qpb-key="background">/)
  assert.match(html, /1 session without a progress report/)
  assert.ok(html.indexOf('cargo test') > html.indexOf('Other activity'))
})

test('all user and agent strings, including disclosure keys, are escaped', () => {
  const html = renderBoard({ now: NOW, lanes: [lane({
    key: '"><img src=x>', agent: '<img src=x onerror=alert(1)>',
    tasks: [task({ title: '"><script>x</script>', detail: "a'b&c" })],
    shells: [{ command: '<svg onload=x>', cwd: '" onmouseover="x', startedAt: NOW }],
  })] })
  assert.doesNotMatch(html, /<img|<script>|<svg|" onmouseover=/)
  assert.match(html, /&lt;img src=x onerror=alert\(1\)&gt;/)
  assert.match(html, /a&#39;b&amp;c/)
  assert.match(renderBoard(null, { empty: '<Nothing yet>' }), /&lt;Nothing yet&gt;/)
})
