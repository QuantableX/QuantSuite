import type { MemoryMeta, SearchHit } from '../types'

export type MemorySort = 'relevance' | 'updated' | 'title' | 'links'

export function memoryPreview(body: string, title = ''): string {
  return body.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '')
    .replace(/^#{1,6}\s+.*$/gm, '')
    .replace(/```[\s\S]*?(?:```|$)/g, '')
    .replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target: string, alias?: string) => alias ?? target)
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/<[^>]*>/g, '').replace(/[*_`~]/g, '')
    .replace(/^\s*[-*>]\s+/gm, '').replace(/\s+/g, ' ').trim()
    .replace(new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s+`), '').slice(0, 280)
}

export function parseStarred(raw: string | null): string[] {
  try { const ids: unknown = JSON.parse(raw ?? '[]'); return Array.isArray(ids) ? [...new Set(ids.filter((id): id is string => typeof id === 'string' && !!id))].slice(0, 2000) : [] }
  catch { return [] }
}

/** Search order is the backend's BM25 rank; filtering never crosses scopes. */
export function memoryLibraryRows(memories: MemoryMeta[], options: {
  scope: string; tag: string | null; kind: string | null; unlinked: boolean
  searching: boolean; hits: SearchHit[]; sort: MemorySort
}): Array<{ meta: MemoryMeta; snippet: string }> {
  const byId = new Map(memories.map(meta => [meta.id, meta]))
  const rows = options.searching
    ? options.hits.flatMap(hit => { const meta = byId.get(hit.id); return meta ? [{ meta, snippet: hit.snippet }] : [] })
    : memories.map(meta => ({ meta, snippet: '' }))
  const filtered = rows.filter(({ meta }) =>
    (options.scope === 'general' || meta.scope === options.scope) &&
    (!options.tag || meta.tags.includes(options.tag)) &&
    (!options.kind || meta.kind === options.kind) &&
    (!options.unlinked || meta.incomingLinks + meta.outgoingLinks === 0))
  if (options.sort === 'title') filtered.sort((a, b) => a.meta.title.localeCompare(b.meta.title))
  else if (options.sort === 'links') filtered.sort((a, b) => (b.meta.incomingLinks + b.meta.outgoingLinks) - (a.meta.incomingLinks + a.meta.outgoingLinks))
  else if (options.sort === 'updated' || !options.searching) filtered.sort((a, b) => b.meta.updatedAt.localeCompare(a.meta.updatedAt))
  return filtered
}
