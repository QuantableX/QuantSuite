import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { GROUPS_SCOPE, GROUPS_KEY, GROUPS_DEV_KEY, readGroups, editGroup, type StrategyGroup, type StrategyGroupsDocument } from '#systems/utils/strategyGroups'

function native() { return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window }
async function read() {
  const raw = native()
    ? await invoke('plugin:qs|get_setting', { scope: GROUPS_SCOPE, key: GROUPS_KEY })
    : JSON.parse(localStorage.getItem(GROUPS_DEV_KEY) ?? 'null')
  return readGroups(raw)
}
async function write(value: StrategyGroupsDocument) {
  if (native()) await invoke('plugin:qs|set_setting', { scope: GROUPS_SCOPE, key: GROUPS_KEY, value })
  else localStorage.setItem(GROUPS_DEV_KEY, JSON.stringify(value))
}

export const useStrategyGroupsStore = defineStore('systems/groups', () => {
  const groups = ref<StrategyGroup[]>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)
  const pending = ref(0)
  const busy = computed(() => pending.value > 0)
  let queue = Promise.resolve()

  // Read the latest document for every edit; serialize this window's writes.
  function run(change?: (doc: StrategyGroupsDocument) => void): Promise<boolean> {
    pending.value++
    const job = queue.then(async () => {
      try {
        const doc = await read()
        if (change) { change(doc); await write(doc) }
        groups.value = doc.groups
        loaded.value = true
        error.value = null
        return true
      } catch (e) {
        error.value = String(e instanceof Error ? e.message : e)
        return false
      } finally { pending.value-- }
    })
    queue = job.then(() => {})
    return job
  }
  function load(force = false) { return loaded.value && !force ? Promise.resolve(true) : run() }
  function saveGroup(id: string | null, name: string, members: string[], expected?: StrategyGroup) {
    const target = id ?? crypto.randomUUID()
    return run(doc => {
      const current = doc.groups.find(g => g.id === target)
      if (expected && (!current || current.name !== expected.name || JSON.stringify(current.systemIds) !== JSON.stringify(expected.systemIds))) {
        throw new Error('This group changed in another window. Close and reopen the editor to use its latest members.')
      }
      editGroup(doc, target, name, members, id !== null)
    })
  }
  function toggle(id: string) {
    return run(doc => {
      const group = doc.groups.find(g => g.id === id)
      if (!group) throw new Error('This group no longer exists. Reload the groups.')
      group.collapsed = !group.collapsed
    })
  }
  function remove(id: string) { return run(doc => { doc.groups = doc.groups.filter(g => g.id !== id) }) }
  function moveSystem(systemId: string, targetId: string | null) {
    return run(doc => {
      const target = doc.groups.find(g => g.id === targetId)
      if (targetId !== null && !target) throw new Error('This group no longer exists. Reload the groups.')
      for (const group of doc.groups) group.systemIds = group.systemIds.filter(id => id !== systemId)
      if (target) target.systemIds.push(systemId)
    })
  }
  return { groups, loaded, error, busy, load, saveGroup, toggle, remove, moveSystem }
})
