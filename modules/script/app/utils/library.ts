import type { ScriptEntry } from '#script/types'
export const LIBRARY_FILTERS = [
  { id: 'all', label: 'All scripts' },
  { id: 'custom', label: 'Indicators' },
  { id: 'favorites', label: 'Favorites' },
  { id: 'attention', label: 'Needs attention' },
] as const

export function scriptTitle(entry: ScriptEntry): string {
  const registered = entry.classes.filter((c) => c.key)
  return registered.length === 1 ? registered[0]!.name || registered[0]!.class_name : entry.stem.replace(/_/g, ' ')
}

export function scriptStatus(entry: ScriptEntry): { label: string; tone: string } {
  if (entry.syntax_error || entry.discovery_error) return { label: 'Needs fixing', tone: 'is-error' }
  if (entry.versions.external) return { label: 'Changed externally', tone: 'is-warn' }
  const indicators = entry.classes.filter((c) => c.key)
  if (indicators.some((c) => c.certification?.source === 'historical'))
    return { label: 'Historical evidence', tone: '' }
  if (indicators.length && indicators.every((c) => c.certification?.certified))
    return { label: 'Certified', tone: 'is-ok' }
  return { label: indicators.some((c) => c.certification) ? 'Needs validation' : 'Not certified', tone: '' }
}
