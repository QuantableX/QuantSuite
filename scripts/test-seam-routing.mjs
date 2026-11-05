// The client-mode routing table (card Server 5/7): every command of the remote
// manifest goes where its placement says, the core's settings and entity
// commands go by their argument, and nothing unlisted leaves this PC.
//
//   node --test scripts/test-seam-routing.mjs

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { mergeSearch, route } from '../apps/shell/app/seam/routing.mjs'

const manifest = JSON.parse(readFileSync(fileURLToPath(new URL('../crates/qs-gateway/remote-manifest.json', import.meta.url)), 'utf8'))
const DATA = /^plugin:qs\|(get_settings|get_setting|set_setting|upsert_entity|delete_entity|list_entities|count_entities|search_entities|link_entities|unlink_entities|linked_entities)$/

test('every manifest command is routed by its placement', () => {
  let n = 0
  for (const [cmd, entry] of Object.entries(manifest.commands)) {
    if (DATA.test(cmd)) continue
    const run = entry.split(':')[0]
    const want = run === 'server' ? 'server' : run === 'deny' ? 'deny' : 'local'
    assert.equal(route(manifest, cmd, {}), want, cmd)
    n++
  }
  assert.ok(n > 600, `${n} commands`)
})

test('the phase-1 placement of §3', () => {
  assert.equal(route(manifest, 'plugin:notes|list_notes', {}), 'server')
  assert.equal(route(manifest, 'plugin:algo|start_bot', {}), 'server')
  assert.equal(route(manifest, 'plugin:systems|list_systems', {}), 'server')
  assert.equal(route(manifest, 'plugin:pilot|list_threads', {}), 'local')
  assert.equal(route(manifest, 'plugin:hud|hud_status', {}), 'local')
  assert.equal(route(manifest, 'plugin:algo|wallet_export_key', {}), 'deny', 'secrets never leave the server')
  assert.equal(route(manifest, 'plugin:qs|server_status', {}), 'server', 'the health page shows the server')
})

test('the developer modules never leave the PC you sit at (§6.3.10)', () => {
  for (const [cmd, entry] of Object.entries(manifest.commands)) {
    if (/^plugin:(pilot|canvas|console|mcp|memory|hud)\|/.test(cmd)) {
      assert.notEqual(entry.split(':')[0], 'server', cmd)
      assert.notEqual(route(manifest, cmd, {}), 'server', cmd)
    }
  }
})

test('Tauri’s own plugins and unlisted commands stay on this PC', () => {
  for (const cmd of ['plugin:event|listen', 'plugin:window|set_title', 'plugin:path|resolve_directory', 'plugin:dialog|open', 'plugin:fs|read_file', 'plugin:shell|open', 'suite_check_for_update', 'plugin:resources|close'])
    assert.equal(route(manifest, cmd, {}), 'local', cmd)
})

test('core settings go by scope, entities by module', () => {
  assert.equal(route(manifest, 'plugin:qs|get_setting', { scope: 'systems', key: 'groups' }), 'server')
  assert.equal(route(manifest, 'plugin:qs|set_setting', { scope: 'notes', key: 'sections', value: [] }), 'server', 'the drawer notes')
  assert.equal(route(manifest, 'plugin:qs|set_setting', { scope: 'todos', key: 'sections', value: [] }), 'server', 'the to-dos')
  assert.equal(route(manifest, 'plugin:qs|get_settings', { scope: 'core' }), 'local', 'appearance, window, connections')
  assert.equal(route(manifest, 'plugin:qs|set_setting', { scope: 'mcp', key: 'x', value: 1 }), 'local')
  assert.equal(route(manifest, 'plugin:qs|upsert_entity', { entity: { id: 'notes:note:1', module: 'notes' } }), 'server')
  assert.equal(route(manifest, 'plugin:qs|upsert_entity', { entity: { id: 'core:workspace:1', module: 'core' } }), 'local', 'workspaces are per PC')
  assert.equal(route(manifest, 'plugin:qs|list_entities', { module: 'plan' }), 'server')
  assert.equal(route(manifest, 'plugin:qs|list_entities', {}), 'local')
  assert.equal(route(manifest, 'plugin:qs|delete_entity', { id: 'habit:habit:9' }), 'server')
  assert.equal(route(manifest, 'plugin:qs|linked_entities', { id: 'pilot:thread:1' }), 'local')
  assert.equal(route(manifest, 'plugin:qs|link_entities', { link: { src: 'notes:note:1', dst: 'core:workspace:1', rel: 'r' } }), 'server')
  assert.equal(route(manifest, 'plugin:qs|search_entities', { query: 'x' }), 'both')
  assert.equal(route(manifest, 'plugin:qs|emit_event', { topic: 'notes.note.created' }), 'local', 'no echo to the server')
})

test('the search merges the server’s results with this PC’s own', () => {
  const local = [{ id: 'core:workspace:1', module: 'core' }, { id: 'notes:note:old', module: 'notes' }]
  const remote = [{ id: 'notes:note:1', module: 'notes' }]
  assert.deepEqual(mergeSearch(manifest, local, remote, 10).map((e) => e.id), ['notes:note:1', 'core:workspace:1'], 'no stale local copies')
  assert.equal(mergeSearch(manifest, local, remote, 1).length, 1)
})
