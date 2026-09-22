import type { GraphEdge, GraphNode } from '../types'

export const constellationPalette = ['#a99bff', '#63d6da', '#ecb885', '#7caeff', '#dc9cc8', '#91cfa6']

export const DEFAULT_GRAPH_ZOOM = 1
export const MIN_GRAPH_ZOOM = 0.15
export const MAX_GRAPH_ZOOM = 4

/** Percentage-point steps, not multiplicative zoom (100 → 105 → 110). */
export function stepGraphZoom(current: number, direction: number): number {
  return Math.min(MAX_GRAPH_ZOOM, Math.max(MIN_GRAPH_ZOOM, Math.round((current + Math.sign(direction) * 0.05) * 100) / 100))
}

export function scopeColor(scope: string): string {
  if (scope === 'general') return constellationPalette[0]!
  let hash = 0
  for (const char of scope) hash = (hash * 31 + char.charCodeAt(0)) >>> 0
  return constellationPalette[1 + hash % (constellationPalette.length - 1)]!
}

/** Only real edges create a neighbourhood. Missing endpoints never become nodes. */
export function graphNeighborhood(nodes: GraphNode[], edges: GraphEdge[], id: string): Set<string> {
  const available = new Set(nodes.map(node => node.id))
  if (!available.has(id)) return new Set()
  const found = new Set([id])
  for (const edge of edges) {
    if (edge.source === id && available.has(edge.target)) found.add(edge.target)
    if (edge.target === id && available.has(edge.source)) found.add(edge.source)
  }
  return found
}

export function findGraphNodes(nodes: GraphNode[], query: string): GraphNode[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)
  if (!terms.length) return []
  return nodes.filter(node => {
    const text = `${node.title} ${node.tags.join(' ')} ${node.kind ?? ''}`.toLocaleLowerCase()
    return terms.every(term => text.includes(term))
  }).sort((a, b) => Number(b.title.toLocaleLowerCase() === query.trim().toLocaleLowerCase()) - Number(a.title.toLocaleLowerCase() === query.trim().toLocaleLowerCase()) || b.incoming + b.outgoing - a.incoming - a.outgoing)
}
