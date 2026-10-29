// QuantPilot V3: what the git actions control offers (card 12).
// Run: node --test scripts/test-pilot-git.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { quickAction, menuEntries, openPr } = await import('../modules/pilot/app/utils/gitActions.ts')

const file = (path) => ({ path, status: 'M', added: 1, removed: 0, binary: false })
function st(patch = {}) {
  return {
    branch: 'quantpilot/x',
    defaultBranch: 'main',
    base: 'main',
    upstream: null,
    ahead: 0,
    behind: 0,
    aheadOfBase: 0,
    files: [],
    remote: null,
    github: null,
    prBlocked: null,
    worktree: false,
    aiCredits: [],
    ...patch,
  }
}
const pr = (state) => ({ threadId: 't', kind: 'pr', url: 'https://github.com/o/r/pull/4', number: 4, title: '', state, checkedAt: 0, createdAt: 0 })

test('changes come first: commit, and push along when there is a remote', () => {
  assert.deepEqual([quickAction(st({ files: [file('a')] })).id, quickAction(st({ files: [file('a')] })).label], ['commit', 'Commit'])
  assert.equal(quickAction(st({ files: [file('a')], remote: 'origin' })).label, 'Commit & push')
})

test('a remote-less repository gets local actions only', () => {
  const ids = menuEntries(st()).map((e) => e.id)
  assert.deepEqual(ids, ['commit', 'discard', 'refresh'])
  assert.equal(quickAction(st()).id, null)
  assert.equal(quickAction(st()).label, 'Up to date')
})

test('a clean worktree thread with commits merges into its base', () => {
  const s = st({ worktree: true, aheadOfBase: 2 })
  assert.deepEqual([quickAction(s).id, quickAction(s).label], ['merge', 'Merge into main'])
  const merge = menuEntries(s).find((e) => e.id === 'merge')
  assert.equal(merge.disabled, false)
  const dirty = menuEntries(st({ worktree: true, aheadOfBase: 2, files: [file('a')] })).find((e) => e.id === 'merge')
  assert.equal(dirty.reason, 'Commit or discard the changed files first')
})

test('GitHub: push and create a pull request, unless one is open or gh is missing', () => {
  const s = st({ remote: 'origin', github: 'o/r', aheadOfBase: 1 })
  assert.deepEqual([quickAction(s).id, quickAction(s).label], ['pr', 'Push & create PR'])
  assert.equal(quickAction(s, [pr('open')]).id, 'push', 'an open PR: just push')
  const blocked = st({ remote: 'origin', github: 'o/r', aheadOfBase: 1, prBlocked: 'Sign in to GitHub with `gh auth login` to open pull requests.' })
  assert.equal(menuEntries(blocked).find((e) => e.id === 'pr').reason, blocked.prBlocked)
  assert.equal(quickAction(blocked).id, 'push')
  assert.equal(openPr([pr('merged'), pr('draft')]).state, 'draft')
  assert.equal(openPr([pr('closed')]), null)
})

test('behind pulls, diverged explains', () => {
  assert.equal(quickAction(st({ remote: 'origin', upstream: 'origin/x', behind: 2 })).id, 'pull')
  const d = quickAction(st({ remote: 'origin', upstream: 'origin/x', behind: 1, ahead: 1 }))
  assert.equal(d.id, null)
  assert.match(d.hint, /diverged/)
  assert.equal(menuEntries(st({ remote: 'origin', upstream: 'origin/x', behind: 1, ahead: 1 })).find((e) => e.id === 'push').disabled, true)
})

test('a running action locks the button with its label', () => {
  assert.deepEqual(quickAction(st({ files: [file('a')] }), [], 'Committing…'), { id: null, label: 'Committing…', hint: 'A git action is running.' })
})
