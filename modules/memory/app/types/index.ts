/** TS mirrors of the crate's serde types (camelCase on the wire). */

export interface MemoryMeta {
  id: string
  /** 'general' or a workspace entity id (core:workspace:<b36>). */
  scope: string
  slug: string
  relPath: string
  title: string
  /** The frontmatter `type` — user | project | reference | decision | … */
  kind: string | null
  author: string | null
  tags: string[]
  createdAt: string
  updatedAt: string
  wordCount: number
  outgoingLinks: number
  incomingLinks: number
  managed: boolean
  preview?: string
  quality?: MemoryQuality
}

export interface MemoryQuality {
  basis: 'unverified' | 'observed' | 'inferred'
  confidence: number | null
  sources: string[]
  lastVerified: string | null
  supersededBy: string | null
  conflictsWith: string[]
  reviewed: boolean
}

export interface OutgoingLink {
  target: string
  targetId: string | null
  resolvedTitle: string | null
}

export interface Backlink {
  id: string
  title: string
  slug: string
  count: number
}

export interface MemoryDoc {
  revision?: string | null
  meta: MemoryMeta
  body: string
  frontmatter: Record<string, unknown> | null
  outgoing: OutgoingLink[]
  backlinks: Backlink[]
}

export interface SearchHit {
  id: string
  scope: string
  title: string
  slug: string
  snippet: string
  score: number
}

export interface UnresolvedLink {
  scope: string
  target: string
  count: number
  sources: Backlink[]
}

export interface GraphNode {
  id: string
  scope: string
  title: string
  kind: string | null
  tags: string[]
  incoming: number
  outgoing: number
  missing: boolean
}

export interface GraphEdge {
  source: string
  target: string
  count: number
}

export interface MemoryGraph {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

export interface Suggestion {
  id: string
  title: string
  slug: string
  sharedTerms: string[]
}

export interface VaultStats {
  memories: number
  resolvedLinks: number
  unresolvedLinks: number
  orphans: number
  tags: number
  words: number
  lastUpdatedAt: string | null
}

export interface TagCount {
  tag: string
  count: number
}

/** One memory scope: the general vault or a registered workspace. */
export interface ScopeInfo {
  scope: string
  name: string
  path: string | null
  vaultDir: string
  memoryCount: number
  isActive: boolean
}

export interface VaultInfo {
  path: string
  isDefault: boolean
  watching: boolean
  trashCount: number
}

export interface TrashEntry {
  fileName: string
  size: number
  deletedAtMs: number
}

/** One directory level of the vault, built from the indexed relPaths. */
export interface VaultFolder {
  name: string
  /** Vault-relative path with forward slashes; '' is the root. */
  path: string
  folders: VaultFolder[]
  files: MemoryMeta[]
}

export interface ScanReport {
  added: number
  updated: number
  removed: number
  total: number
  changedIds: string[]
  removedIds: string[]
}
