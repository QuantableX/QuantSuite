// QuantPilot V3 card 21: the board view's card ↔ thread links and the Start thread prompt.
// Run: node --test scripts/test-pilot-board.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const B = await import('../modules/pilot/app/utils/board.ts')

const link = (threadId, kind, url, createdAt) => ({ threadId, kind, url, number: 0, title: '', state: kind === 'card' ? 'card' : 'open', checkedAt: 0, createdAt })

test('a card link names its card; a pull request does not', () => {
  assert.equal(B.cardIdOf({ kind: 'card', url: 'card:abc' }), 'abc')
  assert.equal(B.cardIdOf({ kind: 'pr', url: 'https://github.com/o/r/pull/1' }), null)
  assert.equal(B.cardIdOf({ kind: 'card', url: 'https://x' }), null)
})

test('every card lists the threads started from it, oldest first', () => {
  const links = {
    t2: [link('t2', 'card', 'card:c1', 20), link('t2', 'pr', 'https://github.com/o/r/pull/3', 25)],
    t1: [link('t1', 'card', 'card:c1', 10)],
    t3: [link('t3', 'card', 'card:c2', 5)],
  }
  assert.deepEqual(B.threadsByCard(links), { c1: ['t1', 't2'], c2: ['t3'] })
  assert.deepEqual(B.threadsByCard({}), {})
})

test('a workspace card is claimed, a General card moves through the columns', () => {
  const card = { id: 'c1', title: ' Board view ', description: 'Columns and a panel.\n', workspace_id: 'core:workspace:7' }
  const ws = B.cardPrompt(card, 'QuantSuite')
  assert.ok(ws.startsWith('Work on the kanban card "Board view" (card c1, QuantSuite board).\n\nColumns and a panel.\n\n'))
  assert.match(ws, /claim_kanban_card/)
  assert.match(ws, /approval mode/)
  const general = B.cardPrompt({ ...card, description: '', workspace_id: 'general' }, 'General')
  assert.doesNotMatch(general, /claim_kanban_card/)
  assert.match(general, /move_kanban_card/)
  assert.equal(general.split('\n\n').length, 2)
})
