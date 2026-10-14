import assert from 'node:assert/strict'
import { test } from 'node:test'
import { registerHooks } from 'node:module'
import { createPinia, setActivePinia } from 'pinia'
import { ref, computed } from 'vue'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'

registerHooks({ resolve(specifier, context, next) {
  return next(specifier.startsWith('#systems/') ? new URL(`../modules/systems/app/${specifier.slice(9)}.ts`, import.meta.url).href : specifier, context)
} })
Object.assign(globalThis, { ref, computed, window: {} })
const { useStrategyGroupsStore } = await import('../modules/systems/app/stores/groups.ts')
const { groupSections } = await import('../modules/systems/app/utils/strategyGroups.ts')

function setup(t, initial = null) {
  const disk = { value: initial, writes: 0, failRead: false, failWrite: false }
  setActivePinia(createPinia())
  mockIPC((command, args) => {
    assert.equal(args.scope, 'systems')
    assert.equal(args.key, 'strategyGroups')
    if (command === 'plugin:qs|get_setting') {
      if (disk.failRead) throw new Error('Read failed')
      return structuredClone(disk.value)
    }
    assert.equal(command, 'plugin:qs|set_setting', 'Grouping must never modify strategies or their config')
    if (disk.failWrite) throw new Error('Disk full')
    disk.writes++
    disk.value = structuredClone(args.value)
  })
  t.after(clearMocks)
  return { disk, store: useStrategyGroupsStore() }
}
const catalog = ['baseline', 'one', 'two', 'three'].map(id => ({ id, name: id, short: '', status: 'ready', description: '' }))

test('groups persist, move members exclusively, and retain catalog/shortcut order', async t => {
  const { store, disk } = setup(t)
  assert.equal(await store.load(), true)
  assert.equal(await store.saveGroup(null, 'First', ['two', 'one']), true)
  const first = store.groups[0].id
  await store.saveGroup(null, 'Second', ['two', 'three'])
  const second = store.groups[1].id
  assert.deepEqual(store.groups[0].systemIds, ['one'])
  await store.toggle(first)
  await store.saveGroup(first, 'Renamed', ['one'])
  assert.equal(store.groups[0].collapsed, true)
  setActivePinia(createPinia())
  const reloaded = useStrategyGroupsStore()
  await reloaded.load()
  assert.equal(reloaded.groups[0].name, 'Renamed')
  assert.equal(reloaded.groups[0].collapsed, true)
  const sections = groupSections(reloaded.groups, catalog)
  assert.deepEqual(sections.ungrouped.map(s => s.id), ['baseline'])
  assert.deepEqual(sections.groups[1].systems.map(s => s.id), ['two', 'three'])
  await reloaded.remove(second)
  assert.deepEqual(groupSections(reloaded.groups, catalog).ungrouped.map(s => s.id), ['baseline', 'two', 'three'])
  assert.deepEqual(catalog.map(s => s.id), ['baseline', 'one', 'two', 'three'])
  assert.equal(disk.value.groups.length, 1)
})

test('failed reads and writes keep the last good state and allow retry', async t => {
  const { store, disk } = setup(t)
  await store.saveGroup(null, 'Saved', ['one'])
  const id = store.groups[0].id
  disk.failWrite = true
  assert.equal(await store.toggle(id), false)
  assert.match(store.error, /Disk full/)
  assert.equal(store.groups[0].collapsed, false)
  assert.equal(disk.value.groups[0].collapsed, false)
  disk.failWrite = false
  disk.failRead = true
  assert.equal(await store.remove(id), false)
  assert.equal(store.groups.length, 1)
  assert.equal(disk.writes, 1)
  disk.failRead = false
  assert.equal(await store.toggle(id), true)
  assert.equal(store.groups[0].collapsed, true)
  assert.equal(store.busy, false)
  assert.equal(store.error, null)
})

test('malformed or future saved data cannot be replaced by an empty list', async t => {
  const { store, disk } = setup(t, { version: 2, groups: [] })
  assert.equal(await store.load(), false)
  assert.equal(await store.saveGroup(null, 'New', ['one']), false)
  assert.equal(store.loaded, false)
  assert.equal(disk.writes, 0)
  disk.value = { version: 1, groups: [{ id: 'a', name: 'A', collapsed: false, systemIds: ['one', 'one'] }] }
  assert.equal(await store.load(), false)
  assert.equal(disk.writes, 0)
})

test('queued edits preserve collapse changes and reject stale group editors', async t => {
  const { store, disk } = setup(t)
  await store.saveGroup(null, 'First', ['one'])
  const original = JSON.parse(JSON.stringify(store.groups[0]))
  const results = await Promise.all([store.toggle(original.id), store.saveGroup(original.id, 'Renamed', ['one'], original)])
  assert.deepEqual(results, [true, true])
  assert.equal(store.groups[0].collapsed, true)
  disk.value.groups.push({ id: 'external', name: 'Other window', systemIds: ['three'], collapsed: true })
  await store.toggle(original.id)
  assert.equal(store.groups.length, 2)
  assert.equal(await store.saveGroup(original.id, 'Stale edit', ['two'], original), false)
  assert.match(store.error, /changed in another window/)
  assert.equal(disk.value.groups[0].name, 'Renamed')
  assert.deepEqual(disk.value.groups[1].systemIds, ['three'])
})

test('unchecked members become ungrouped and unavailable members survive catalog refresh', async t => {
  const { store, disk } = setup(t)
  await store.saveGroup(null, 'A', ['one', 'two', 'unavailable'])
  const id = store.groups[0].id
  assert.deepEqual(groupSections(store.groups, catalog).groups[0].systems.map(s => s.id), ['one', 'two'])
  await store.toggle(id)
  assert.ok(disk.value.groups[0].systemIds.includes('unavailable'))
  await store.saveGroup(id, 'A', ['two', 'unavailable'])
  assert.ok(groupSections(store.groups, catalog).ungrouped.some(s => s.id === 'one'))
  assert.equal(await store.saveGroup(null, ' a ', []), false)
  assert.match(store.error, /already exists/)
  assert.equal(disk.value.groups.length, 1)
})

test('drag moves into, between and out of collapsed groups without changing collapse or config', async t => {
  const { store, disk } = setup(t)
  await store.saveGroup(null, 'First', [])
  await store.saveGroup(null, 'Second', ['two'])
  const [first, second] = store.groups.map(g => g.id)
  await store.toggle(first)
  await store.moveSystem('one', first)
  assert.deepEqual(store.groups[0].systemIds, ['one'])
  assert.equal(store.groups[0].collapsed, true)
  await store.moveSystem('one', second)
  assert.deepEqual(store.groups.map(g => g.systemIds), [[], ['two', 'one']])
  disk.failWrite = true
  assert.equal(await store.moveSystem('one', null), false)
  assert.deepEqual(store.groups[1].systemIds, ['two', 'one'])
  disk.failWrite = false
  await store.moveSystem('one', null)
  assert.deepEqual(store.groups[1].systemIds, ['two'])
  assert.equal(await store.moveSystem('two', 'removed-group'), false)
  assert.deepEqual(disk.value.groups[1].systemIds, ['two'])
  setActivePinia(createPinia())
  const reloaded = useStrategyGroupsStore()
  await reloaded.load()
  assert.equal(reloaded.groups[0].collapsed, true)
  assert.ok(groupSections(reloaded.groups, catalog).ungrouped.some(s => s.id === 'one'))
})
