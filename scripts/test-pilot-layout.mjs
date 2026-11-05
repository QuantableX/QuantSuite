// Every QuantPilot tab keeps the same two bars; only the middle changes (user rule, 2026-11-05).
// The geometry rule behind it: both bars at their default width, so they line up with the header's caps.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const read = (path) => readFileSync(join(ROOT, path), 'utf8')
const layout = read('modules/pilot/app/layouts/pilot.vue')

test('both bars depend only on their own toggle, never on the tab', () => {
  const left = layout.match(/<QSidebar\b[^>]*>/)?.[0]
  const right = layout.match(/<QRightPanel\b[^>]*>/)?.[0]
  assert.ok(left, 'the left bar is in the layout')
  assert.ok(right, 'the right bar is in the layout')
  assert.match(left, /:model-value="app\.sidebarLeftOpen"/)
  assert.match(right, /:model-value="app\.sidebarRightOpen"/)
  for (const bar of [left, right]) {
    assert.doesNotMatch(bar, /app\.view/, 'no tab takes a bar away')
    assert.doesNotMatch(bar, /:width/, 'the default width lines up with the header')
  }
})

test('every tab renders inside the layout, not as a page of its own', () => {
  const page = read('modules/pilot/app/pages/pilot/index.vue')
  const views = read('modules/pilot/app/stores/app.ts').match(/export type PilotView = ([^\n]+)/)?.[1].match(/'[a-z]+'/g).map((v) => v.slice(1, -1))
  assert.ok(views?.length >= 6)
  assert.match(page, /definePageMeta\(\{ layout: 'pilot' \}\)/)
  for (const view of views) assert.match(page, new RegExp(`app\\.view === '${view}'`), `${view} is shown by the one page`)
})

test('opening a thread shows it from any tab, except where it stays on purpose', () => {
  const app = read('modules/pilot/app/stores/app.ts')
  assert.match(app, /showThread\(\) \{[\s\S]*?this\.view !== 'conversation' && this\.view !== 'crew'/, 'the crew view follows the open thread')
  assert.match(app, /setView\(view: PilotView\) \{[\s\S]*?\.page = 'thread'/, 'a tab leaves the Usage page')
  assert.match(read('modules/pilot/app/stores/pilot.ts'), /if \(!stay\) useAppStore\(\)\.showThread\(\)/)
  assert.match(read('modules/pilot/app/components/Board/View.vue'), /openSession\(threadId, false, true\)/, 'the thread beside the board stays beside it')
  // What the right panel opens beside the conversation brings the conversation back first.
  const details = read('modules/pilot/app/components/Thread/Details.vue')
  for (const call of ['side.startNew', 'inspector.openFile', 'inspector.openAgent', 'inspector.openBrowser']) {
    assert.match(details, new RegExp(`atThread\\(\\(\\) => ${call.replace('.', '\\.')}`), call)
  }
})
