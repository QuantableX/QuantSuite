// QuantPilot round 3, card 06: design artboards — the sandboxed frame document and the agent's message.
// Run: node --test scripts/test-canvas-artboard.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { frameDoc, askText, starterHtml, DEVICES, CSP } = await import('../modules/canvas/app/utils/artboard.ts')

test('the frame document blocks the network before the mockup runs', () => {
  const doc = frameDoc('<!DOCTYPE html><html><body><h1 id="hero">Hi</h1><script>fetch("https://x")</script></body></html>')
  assert.ok(doc.startsWith('<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy"'))
  assert.ok(doc.indexOf(CSP) < doc.indexOf('<h1 id="hero">'), 'the policy comes first')
  assert.match(CSP, /default-src 'none'/)
  assert.doesNotMatch(CSP, /connect-src|https?:/, 'nothing may load or connect')
  assert.equal((doc.match(/<!doctype/gi) ?? []).length, 1, "the mockup's own doctype is dropped")
  assert.ok(doc.includes("parent.postMessage({qcArt:1,type:'picked'"), 'the bridge talks only by postMessage')
  assert.ok(doc.includes('})();</scr' + 'ipt>'), 'the bridge script is closed')
})

test('a comment reaches the agent with the element, the text and the tools to use', () => {
  const msg = askText({ id: 'landing', name: 'Landing page' }, { id: 'c1a2', selector: 'main > h1', quote: 'Welcome', text: 'Make it bigger' })
  assert.match(msg, /artboard “Landing page” \(id `landing`/)
  assert.match(msg, /Element: `main > h1` — “Welcome”/)
  assert.match(msg, /Comment: Make it bigger/)
  assert.match(msg, /quantsuite\.canvas\.design_read/)
  assert.match(msg, /design_write \(artboard `landing`\)/)
  assert.match(msg, /design_resolve \(comment `c1a2`\)/)
  assert.doesNotMatch(askText({ id: 'a', name: 'A' }, { id: 'c', selector: 'p', quote: '', text: 't' }), /“”/)
})

test('devices and the starter page', () => {
  assert.deepEqual(DEVICES.map((d) => [d.id, d.width]), [['desktop', 1280], ['tablet', 820], ['phone', 390]])
  assert.match(starterHtml('Pricing <v2>'), /<h1>Pricing v2<\/h1>/)
})
