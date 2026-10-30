// QuantPilot V3 card 23: the GitHub view's prompts and linked threads.
// Run: node --test scripts/test-pilot-github.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const G = await import('../modules/pilot/app/utils/github.ts')

const pr = { number: 45, title: ' Faster rows ', url: 'https://github.com/o/r/pull/45', state: 'open', author: 'ann', branch: 'rows', review: 'CHANGES_REQUESTED', checks: { total: 3, passed: 1, failed: 2, pending: 0, failing: ['lint', 'e2e'] }, additions: 120, deletions: 30, labels: [], updatedAt: '', ready: false }
const issue = { number: 12, title: 'Tabs vanish', url: 'https://github.com/o/r/issues/12', author: 'bob', labels: ['bug'], comments: 2, updatedAt: '' }

test('every prompt names its #number, so the item goes along as GitHub context', () => {
  for (const p of [G.reviewPrompt(pr), G.fixChecksPrompt(pr), G.addressReviewPrompt(pr)]) {
    assert.match(p, /#45 "Faster rows"/)
    assert.match(p, /gh pr checkout 45/)
  }
  assert.match(G.fixChecksPrompt(pr), /failing checks: lint, e2e\./)
  assert.match(G.reviewPrompt(pr), /Do not push or comment on GitHub unless I ask/)
  assert.match(G.solvePrompt(issue), /^Solve issue #12 "Tabs vanish"\./)
  assert.match(G.solvePrompt(issue), /`Closes #12`/)
  assert.equal(G.draftAbout('issue', 12, 'Tabs vanish'), 'Issue #12 "Tabs vanish": ')
  assert.equal(G.draftAbout('pr', 45, 'x'), 'Pull request #45 "x": ')
})

test('the thread that holds an item: the newest link of that repository', () => {
  const link = (threadId, kind, url, createdAt) => ({ threadId, kind, url, number: 0, title: '', state: 'open', checkedAt: 0, createdAt })
  const links = {
    t1: [link('t1', 'pr', 'https://github.com/o/r/pull/45', 10)],
    t2: [link('t2', 'pr', 'https://github.com/o/r/pull/45', 20), link('t2', 'issue', 'https://github.com/o/r/issues/12', 20)],
    t3: [link('t3', 'pr', 'https://github.com/o/other/pull/45', 30), link('t3', 'card', 'card:x', 30)],
  }
  assert.equal(G.threadFor(links, 'https://github.com/o/r', 'pr', 45), 't2')
  assert.equal(G.threadFor(links, 'https://github.com/o/r', 'issue', 12), 't2')
  assert.equal(G.threadFor(links, 'https://github.com/o/r', 'issue', 45), null)
  assert.equal(G.sizeOf(pr), '+120 −30')
})
