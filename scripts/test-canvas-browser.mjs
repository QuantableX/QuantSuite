// QuantCanvas browser windows (rework 2026-11-03): where the live page may show.
// Run: node --test scripts/test-canvas-browser.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const b = await import('../modules/canvas/app/utils/browserSurface.ts')

const B = (left, top, right, bottom) => ({ left, top, right, bottom })

test('a higher window over the page is cut out of it, in page-local device px', () => {
  const page = B(100, 100, 500, 400)
  const clips = b.clipRects(page, [B(400, 50, 700, 200)], B(0, 0, 1000, 800), 2)
  assert.deepEqual(clips, [{ x: 600, y: 0, w: 200, h: 200 }])
  // A window elsewhere cuts nothing.
  assert.deepEqual(b.clipRects(page, [B(600, 0, 700, 50)], B(0, 0, 1000, 800), 1), [])
})

test('what lies outside the visible canvas is cut away on every side', () => {
  const clips = b.clipRects(B(-50, -20, 300, 900), [], B(0, 0, 1000, 800), 1)
  assert.deepEqual(clips, [
    { x: 0, y: 0, w: 50, h: 920 },
    { x: 0, y: 0, w: 350, h: 20 },
    { x: 0, y: 820, w: 350, h: 100 },
  ])
})

test('probes cover the visible part of the page and skip what is cut out anyway', () => {
  const page = B(0, 0, 600, 500)
  const all = b.probePoints(page, B(0, 0, 1000, 800), [])
  assert.equal(all.length, 6 * 5 + 4)
  assert.ok(all.every((p) => p.x >= 0 && p.x <= 600 && p.y >= 0 && p.y <= 500))
  // Points under a cut (a higher window) are not probed.
  const cut = B(300, 0, 600, 500)
  assert.ok(b.probePoints(page, B(0, 0, 1000, 800), [cut]).every((p) => p.x < 300))
  // Only the part inside the canvas is probed; a page off the canvas has none.
  assert.ok(b.probePoints(page, B(0, 0, 200, 800), []).every((p) => p.x <= 200))
  assert.deepEqual(b.probePoints(page, B(700, 0, 900, 800), []), [])
})

test('anything else on top of the page covers it; the page and its picture do not', () => {
  const page = { id: 'page' }
  const picture = { id: 'picture', parent: page }
  const menu = { id: 'menu' }
  const isPage = (el) => el === page || el.parent === page
  const pts = [{ x: 1, y: 1 }, { x: 2, y: 2 }]
  assert.equal(b.covered(pts, () => picture, isPage), false)
  assert.equal(b.covered(pts, (x) => (x === 2 ? menu : page), isPage), true)
  // Nothing at a point (outside the suite page) is not a cover.
  assert.equal(b.covered(pts, () => null, isPage), false)
})

test('windows overlap only when their areas share space', () => {
  const a = { x: 0, y: 0, width: 100, height: 100 }
  assert.equal(b.overlaps(a, { x: 50, y: 50, width: 100, height: 100 }), true)
  assert.equal(b.overlaps(a, { x: 100, y: 0, width: 50, height: 50 }), false)
})

test('the address bar: URLs, hosts, local servers and searches', () => {
  const search = (q) => `S:${q}`
  assert.equal(b.normalizeAddress('https://example.com/a', search), 'https://example.com/a')
  assert.equal(b.normalizeAddress('example.com', search), 'https://example.com')
  assert.equal(b.normalizeAddress('docs.rs/tauri/latest', search), 'https://docs.rs/tauri/latest')
  assert.equal(b.normalizeAddress('localhost:3000', search), 'http://localhost:3000')
  assert.equal(b.normalizeAddress('3000', search), 'http://localhost:3000')
  assert.equal(b.normalizeAddress('192.168.1.5:8080', search), 'http://192.168.1.5:8080')
  assert.equal(b.normalizeAddress('how to use tauri', search), 'S:how to use tauri')
  assert.equal(b.normalizeAddress('tauri', search), 'S:tauri')
  assert.equal(b.normalizeAddress('  ', search), 'about:blank')
})

test('a tab is named by its page title, else its host', () => {
  assert.equal(b.tabTitle('Tauri — docs', 'https://tauri.app/x'), 'Tauri — docs')
  assert.equal(b.tabTitle('  ', 'https://tauri.app/x'), 'tauri.app')
  assert.equal(b.tabTitle('', 'about:blank'), 'about:blank')
})

test('page zoom steps like a browser and stays in range', () => {
  assert.equal(b.zoomStep(1, 1), 1.1)
  assert.equal(b.zoomStep(1, -1), 0.9)
  assert.equal(b.zoomStep(3, 1), 3)
  assert.equal(b.zoomStep(0.25, -1), 0.25)
  assert.equal(b.zoomStep(1.05, -1), 1)
})
