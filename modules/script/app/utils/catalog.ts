/**
 * The Collection page's reading of a catalog — pure, so test:script covers it:
 * an item's state badge, its score on a track, the list filters and a
 * plan's summary for the confirmation.
 */
import type { CollectionCatalogItem, CollectionLocal, CollectionPlan } from '#script/types'

export const COLLECTION_TRACKS = ['1h', '4h', '1d'] as const
export type CollectionTrack = (typeof COLLECTION_TRACKS)[number]

/** role → the timeframe it was optimized for (null: Standard and general). */
const ROLE_TRACK: Record<string, string | null> = {
  standard: null,
  optimized: null,
  optimized_1h: '1h',
  optimized_4h: '4h',
  optimized_1d: '1d',
}

export type CollectionStateFilter = 'all' | 'installed' | 'update' | 'available'

export interface CollectionFilters {
  search: string
  tag: string
  state: CollectionStateFilter
  track: CollectionTrack
  minScore: number
  certifiedOnly: boolean
}

export const DEFAULT_COLLECTION_FILTERS: CollectionFilters = {
  search: '',
  tag: '',
  state: 'all',
  track: '1d',
  minScore: 0,
  certifiedOnly: false,
}

export interface ItemState {
  label: string
  tone: '' | 'is-ok' | 'is-warn' | 'is-error'
  title: string
}

/** The one badge a row shows; the most pressing state wins. */
export function itemState(local: CollectionLocal | null | undefined, version: string): ItemState {
  if (!local) return { label: 'Available', tone: '', title: '' }
  if (local.too_new) return { label: 'Needs a newer QuantSuite', tone: 'is-error', title: 'The package was published for a newer engine contract.' }
  if (local.installed_version && local.modified) {
    return { label: 'Modified', tone: 'is-warn', title: 'Files the Collection installed were changed in your library; an update would be a conflict.' }
  }
  if (local.installed_version && local.update) {
    return { label: `Update ${local.installed_version} → ${version}`, tone: 'is-warn', title: 'A newer version is in the catalog.' }
  }
  if (local.installed_version) return { label: `Installed ${local.installed_version}`, tone: 'is-ok', title: '' }
  if (local.conflict) {
    return { label: 'In your library', tone: 'is-warn', title: 'A script of this name exists that the Collection did not install — installing it replaces it only if you confirm.' }
  }
  return { label: 'Available', tone: '', title: '' }
}

/** The best verdict of any version on `track` — what a bot of that timeframe could run. */
export function trackVerdict(item: Pick<CollectionCatalogItem, 'scores'>, track: string) {
  let best: { score: number; grade: string; certified: boolean; role: string } | null = null
  for (const [role, tracks] of Object.entries(item.scores ?? {})) {
    const verdict = tracks?.[track]
    if (verdict && (!best || verdict.score > best.score)) best = { ...verdict, role }
  }
  return best
}

/** A role's own track, for the "which version is for which bot" hint. */
export function roleTrack(role: string): string | null {
  return ROLE_TRACK[role] ?? null
}

export function allTags(items: CollectionCatalogItem[]): string[] {
  return [...new Set(items.flatMap((i) => i.tags ?? []))].sort()
}

export function filterItems(items: CollectionCatalogItem[], f: CollectionFilters): CollectionCatalogItem[] {
  const q = f.search.trim().toLowerCase()
  return items
    .filter((i) => !q || [i.key, i.name, i.summary, ...(i.tags ?? [])].some((s) => (s ?? '').toLowerCase().includes(q)))
    .filter((i) => !f.tag || (i.tags ?? []).includes(f.tag))
    .filter((i) => {
      switch (f.state) {
        case 'installed':
          return !!i.local?.installed_version
        case 'update':
          return !!i.local?.update
        case 'available':
          return !i.local?.installed_version
        default:
          return true
      }
    })
    .filter((i) => {
      if (!f.certifiedOnly && !(f.minScore > 0)) return true
      if (i.type === 'library') return false
      const v = trackVerdict(i, f.track)
      if (!v) return false
      if (f.certifiedOnly && !Object.values(i.scores ?? {}).some((t) => t?.[f.track]?.certified)) return false
      return v.score >= f.minScore
    })
}

export interface PlanSummary {
  create: number
  replace: number
  skip: number
  conflict: number
  dependencies: number
  /** Nothing would be written. */
  empty: boolean
}

export function planSummary(plan: CollectionPlan): PlanSummary {
  const count = (action: string) => plan.items.filter((i) => i.action === action).length
  const s = {
    create: count('create'),
    replace: count('replace'),
    skip: count('skip'),
    conflict: count('conflict'),
    dependencies: plan.items.filter((i) => !i.requested && i.action !== 'skip').length,
    empty: false,
  }
  s.empty = s.create + s.replace + s.conflict === 0
  return s
}

export const PLAN_ACTION_LABELS: Record<string, string> = {
  create: 'New',
  replace: 'Replaced',
  skip: 'Unchanged',
  conflict: 'Conflict',
}

export function shortCommit(commit: string | null | undefined): string {
  if (!commit) return '—'
  return commit.startsWith('local-') ? commit : commit.slice(0, 7)
}
