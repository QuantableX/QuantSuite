import type { SystemMeta } from '../types'

export interface StrategyGroup {
  id: string
  name: string
  collapsed: boolean
  systemIds: string[]
}
/** `order` is the user's strategy order (optional on disk; documents saved before it load as []). */
export interface StrategyGroupsDocument { version: 1; groups: StrategyGroup[]; order: string[] }
export const GROUPS_SCOPE = 'systems'
export const GROUPS_KEY = 'strategyGroups'
export const GROUPS_DEV_KEY = 'qs-systems-strategy-groups'

export function readGroups(raw: unknown): StrategyGroupsDocument {
  if (raw === null || raw === undefined) return { version: 1, groups: [], order: [] }
  const doc = raw as Omit<StrategyGroupsDocument, 'order'> & { order?: unknown }
  if (doc.version !== 1 || !Array.isArray(doc.groups)) throw new Error('Saved strategy groups could not be read. Your saved data has been kept.')
  const order = doc.order ?? []
  if (!Array.isArray(order) || order.some(id => typeof id !== 'string' || !id) || new Set(order).size !== order.length) {
    throw new Error('Saved strategy order is invalid. Your saved data has been kept.')
  }
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
  return { version: 1, groups: doc.groups.map(g => ({ ...g, systemIds: [...g.systemIds] })), order: [...order] }
}

/** The saved order first; strategies it does not name yet follow in catalog order. Ctrl+1…9 use this order too. */
export function orderSystems<T extends Pick<SystemMeta, 'id'>>(systems: T[], order: string[]) {
  const rank = new Map(order.map((id, i) => [id, i]))
  return [...systems].sort((a, b) => (rank.get(a.id) ?? order.length) - (rank.get(b.id) ?? order.length))
}

export function groupSections<T extends Pick<SystemMeta, 'id'>>(groups: StrategyGroup[], systems: T[], order: string[] = []) {
  const sorted = orderSystems(systems, order)
  const assigned = new Set(groups.flatMap(g => g.systemIds))
  return {
    ungrouped: sorted.filter(s => !assigned.has(s.id)),
    groups: groups.map(group => ({ group, systems: sorted.filter(s => group.systemIds.includes(s.id)) })),
  }
}

/**
 * Drops a strategy into a section (null = ungrouped) before another strategy, or at the section's end.
 * The saved order becomes the rail's top-to-bottom order, so Ctrl+1…9 follow what the user sees after a drag.
 */
export function placeSystem(doc: StrategyGroupsDocument, systemId: string, groupId: string | null, before: string | null, catalog: string[]) {
  const target = doc.groups.find(g => g.id === groupId)
  if (groupId !== null && !target) throw new Error('This group no longer exists. Reload the groups.')
  for (const group of doc.groups) group.systemIds = group.systemIds.filter(id => id !== systemId)
  if (target) target.systemIds.push(systemId)
  const lists = sectionIds(doc, [...catalog, systemId])
  const list = lists[target ? doc.groups.indexOf(target) + 1 : 0]!
  const from = list.indexOf(systemId)
  // Dropped right before itself: it stays where it is.
  if (before === systemId) before = list[from + 1] ?? null
  list.splice(from, 1)
  const at = before === null ? -1 : list.indexOf(before)
  list.splice(at < 0 ? list.length : at, 0, systemId)
  doc.order = lists.flat()
}

/** Moves a folder before another folder, or to the end; Ctrl+1…9 keep following the rail from top to bottom. */
export function placeGroup(doc: StrategyGroupsDocument, groupId: string, before: string | null, catalog: string[]) {
  const group = doc.groups.find(g => g.id === groupId)
  if (!group) throw new Error('This group no longer exists. Reload the groups.')
  if (before === groupId) return
  doc.groups.splice(doc.groups.indexOf(group), 1)
  const at = before === null ? -1 : doc.groups.findIndex(g => g.id === before)
  doc.groups.splice(at < 0 ? doc.groups.length : at, 0, group)
  doc.order = sectionIds(doc, catalog).flat()
}

/** Strategy ids per rail section, top to bottom: ungrouped first, then each folder. */
function sectionIds(doc: StrategyGroupsDocument, ids: string[]) {
  const sections = groupSections(doc.groups, [...new Set([...ids, ...doc.order])].map(id => ({ id })), doc.order)
  return [sections.ungrouped, ...sections.groups.map(s => s.systems)].map(list => list.map(s => s.id))
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
