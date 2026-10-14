import type { SystemMeta } from '../types'

export interface StrategyGroup {
  id: string
  name: string
  collapsed: boolean
  systemIds: string[]
}
export interface StrategyGroupsDocument { version: 1; groups: StrategyGroup[] }
export const GROUPS_SCOPE = 'systems'
export const GROUPS_KEY = 'strategyGroups'
export const GROUPS_DEV_KEY = 'qs-systems-strategy-groups'

export function readGroups(raw: unknown): StrategyGroupsDocument {
  if (raw === null || raw === undefined) return { version: 1, groups: [] }
  const doc = raw as StrategyGroupsDocument
  if (doc.version !== 1 || !Array.isArray(doc.groups)) throw new Error('Saved strategy groups could not be read. Your saved data has been kept.')
  const ids = new Set<string>(), members = new Set<string>()
  for (const g of doc.groups) {
    if (!g || typeof g.id !== 'string' || !g.id || ids.has(g.id) || typeof g.name !== 'string' || !g.name.trim()
      || typeof g.collapsed !== 'boolean' || !Array.isArray(g.systemIds)
      || g.systemIds.some(id => typeof id !== 'string' || !id || members.has(id))) {
      throw new Error('Saved strategy groups are invalid. Your saved data has been kept.')
    }
    ids.add(g.id)
    for (const id of g.systemIds) {
      if (members.has(id)) throw new Error('A strategy belongs to more than one saved group.')
      members.add(id)
    }
  }
  return { version: 1, groups: doc.groups.map(g => ({ ...g, systemIds: [...g.systemIds] })) }
}

/** Catalog order stays stable, including Ctrl+1…9, regardless of grouping. */
export function groupSections(groups: StrategyGroup[], systems: SystemMeta[]) {
  const assigned = new Set(groups.flatMap(g => g.systemIds))
  return {
    ungrouped: systems.filter(s => !assigned.has(s.id)),
    groups: groups.map(group => ({ group, systems: systems.filter(s => group.systemIds.includes(s.id)) })),
  }
}

export function editGroup(doc: StrategyGroupsDocument, id: string, name: string, members: string[], existing: boolean) {
  name = name.trim()
  if (!name || name.length > 80) throw new Error('Use a group name between 1 and 80 characters.')
  if (doc.groups.some(g => g.id !== id && g.name.toLowerCase() === name.toLowerCase())) throw new Error('A group with this name already exists.')
  const current = doc.groups.find(g => g.id === id)
  if (existing && !current) throw new Error('This group was removed. Close the editor and try again.')
  const systemIds = [...new Set(members)]
  for (const g of doc.groups) g.systemIds = g.systemIds.filter(key => !systemIds.includes(key))
  if (current) Object.assign(current, { name, systemIds })
  else doc.groups.push({ id, name, systemIds, collapsed: false })
}
