/**
 * The vault as the UI sees it — list, search, the open memory, and every
 * mutation. All data flows through `plugin:memory|*`; the store never touches
 * files itself. Writes reload what they changed; external changes arrive via
 * the bus (wired in the layout, warm-cache aware).
 *
 * **Scopes (V2):** the module always loads the FULL list (every scope — the
 * gigabrain), and the selected view decides what is shown: the 'general' view
 * shows everything (general at the root, workspaces as groups), a workspace
 * view shows only that project's memories. The selection is module-local —
 * it never touches `core / workspace.active`.
 */
import { defineStore } from 'pinia'
import { getInvoke } from '#memory/utils/invoke'
import type {
  MemoryDoc,
  MemoryGraph,
  MemoryMeta,
  ScanReport,
  ScopeInfo,
  SearchHit,
  Suggestion,
  TrashEntry,
  UnresolvedLink,
  VaultInfo,
  VaultStats,
} from '#memory/types'

/** Lazy invoke; reports unavailable access outside Tauri. Call sites spell the full
 *  `plugin:memory|<cmd>` name — check:isolation verifies exactly that. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  const invoke = await getInvoke()
  return invoke(cmd, args)
}

export const GENERAL_SCOPE = 'general'

function readStoredScope(): string {
  try {
    return localStorage.getItem('qm-scope') || GENERAL_SCOPE
  } catch {
    return GENERAL_SCOPE
  }
}

export const useVaultStore = defineStore('memory/vault', {
  state: () => ({
    /** The selected view: 'general' (gigabrain — everything) or a workspace scope id. */
    scope: readStoredScope(),
    scopes: [] as ScopeInfo[],

    memories: [] as MemoryMeta[],
    stats: null as VaultStats | null,
    orphans: [] as MemoryMeta[],
    unresolved: [] as UnresolvedLink[],
    vaultInfo: null as VaultInfo | null,
    loading: false,
    error: null as string | null,
    loadRequest: 0,
    viewRequest: 0,
    hygieneRequest: 0,
    searchRequest: 0,
    openRequest: 0,
    searching: false,
    searchError: null as string | null,
    searchMode: 'all' as 'all' | 'any',

    query: '',
    hits: [] as SearchHit[],
    filterTag: null as string | null,
    filterKind: null as string | null,

    activeDoc: null as MemoryDoc | null,
    suggestions: [] as Suggestion[],

    graph: null as MemoryGraph | null,
    graphLoading: false,
    graphError: null as string | null,
    graphRequest: 0,
    trash: [] as TrashEntry[],
  }),

  getters: {
    scopeMemories(state): MemoryMeta[] {
      return state.memories.filter(m => state.scope === GENERAL_SCOPE || m.scope === state.scope)
    },
    /** What the backend should filter to for the current view. */
    viewScope(state): string {
      return state.scope === GENERAL_SCOPE ? 'all' : state.scope
    },
    /** Where a create from this view lands. */
    createScope(state): string {
      return state.scope
    },
    /** The view's memories, with the tag/type chips applied. In the general
     *  view this is every scope — the tree groups them. */
    filtered(state): MemoryMeta[] {
      return state.memories.filter(
        (m) =>
          (state.scope === GENERAL_SCOPE || m.scope === state.scope) &&
          (!state.filterTag || m.tags.includes(state.filterTag)) &&
          (!state.filterKind || m.kind === state.filterKind)
      )
    },
    kinds(): string[] {
      return [...new Set(this.scopeMemories.map((m) => m.kind).filter((k): k is string => !!k))].sort()
    },
    titles(state): string[] {
      return state.memories.map((m) => m.title)
    },
    scopeName() {
      return (scope: string): string =>
        scope === GENERAL_SCOPE
          ? 'General'
          : (this.scopes as ScopeInfo[]).find((s) => s.scope === scope)?.name ?? 'Workspace'
    },
  },

  actions: {
    byIdentifier(identifier: string, scope?: string): MemoryMeta | undefined {
      const lc = identifier.toLowerCase()
      const candidates = this.memories.filter(
        (m) => m.id === identifier || m.title.toLowerCase() === lc || m.slug === lc
      )
      if (candidates.length <= 1) return candidates[0]
      // Prefer the current view's scope, then general — the crate's rule.
      return (
        candidates.find((m) => m.scope === (scope ?? this.scope)) ??
        candidates.find((m) => m.scope === GENERAL_SCOPE) ??
        candidates[0]
      )
    },

    setScope(scope: string) {
      if (this.scope === scope) return
      this.scope = scope
      this.graph = null
      this.graphError = null
      this.stats = null
      this.orphans = []
      this.unresolved = []
      this.hits = []
      this.filterTag = null
      this.filterKind = null
      try {
        localStorage.setItem('qm-scope', scope)
      } catch {
        /* private mode */
      }
      void this.loadView()
      void this.loadHygiene()
      void this.search(this.query)
    },

    /** Everything the sidebar needs, across all scopes (the gigabrain load). */
    async loadAll() {
      const request = ++this.loadRequest
      this.loading = true
      this.error = null
      try {
        const [memories, scopes, info] = await Promise.all([
          call<MemoryMeta[]>('plugin:memory|list_memories', { scope: 'all', limit: 5000 }),
          call<ScopeInfo[]>('plugin:memory|list_scopes'),
          call<VaultInfo>('plugin:memory|get_vault_info'),
        ])
        if (request !== this.loadRequest) return
        this.memories = memories ?? []
        this.scopes = scopes ?? []
        this.vaultInfo = info
      } catch (error) {
        if (request === this.loadRequest) this.error = String(error)
      } finally {
        if (request === this.loadRequest) this.loading = false
      }
      if (request === this.loadRequest && !this.error) {
        await Promise.all([this.loadView(), this.loadHygiene(), this.search(this.query)])
      }
    },

    /** The numbers that depend on the selected view. */
    async loadView() {
      const scope = this.viewScope
      const request = ++this.viewRequest
      try {
        const stats = await call<VaultStats>('plugin:memory|get_stats', { scope })
        if (scope === this.viewScope && request === this.viewRequest) this.stats = stats
      } catch (error) {
        if (scope === this.viewScope && request === this.viewRequest) this.error = String(error)
      }
    },

    async loadHygiene() {
      const scope = this.viewScope
      const request = ++this.hygieneRequest
      try {
        const [orphans, unresolved] = await Promise.all([
          call<MemoryMeta[]>('plugin:memory|get_orphans', { scope }),
          call<UnresolvedLink[]>('plugin:memory|get_unresolved_links', { scope }),
        ])
        if (scope !== this.viewScope || request !== this.hygieneRequest) return
        this.orphans = orphans ?? []
        this.unresolved = unresolved ?? []
      } catch (error) {
        if (scope === this.viewScope && request === this.hygieneRequest) this.error = String(error)
      }
    },

    async search(query: string) {
      this.query = query
      const request = ++this.searchRequest
      this.searchError = null
      if (!query.trim()) {
        this.hits = []
        this.searching = false
        return
      }
      const scope = this.viewScope
      const mode = this.searchMode
      this.searching = true
      try {
        const hits = await call<SearchHit[]>('plugin:memory|search_memories', { query, mode, limit: 200, scope })
        if (request === this.searchRequest && this.query === query && this.viewScope === scope && this.searchMode === mode) this.hits = hits ?? []
      } catch (error) {
        if (request === this.searchRequest) { this.hits = []; this.searchError = String(error) }
      } finally {
        if (request === this.searchRequest) this.searching = false
      }
    },

    async open(identifier: string) {
      const request = ++this.openRequest
      const doc = await call<MemoryDoc>('plugin:memory|get_memory', {
        identifier,
        scope: this.viewScope === 'all' ? null : this.viewScope,
      })
      if (request !== this.openRequest) return null
      this.activeDoc = doc
      this.suggestions = []
      if (doc) {
        const suggestions = await call<Suggestion[]>('plugin:memory|suggest_connections', {
          identifier: doc.meta.id,
          limit: 6,
        }).catch(() => [])
        if (this.activeDoc?.meta.id === doc.meta.id) this.suggestions = suggestions ?? []
      }
      return doc
    },

    /** Refresh the open memory's links/meta without touching the editor body. */
    async refreshActive() {
      const id = this.activeDoc?.meta.id
      if (!id) return
      const doc = await call<MemoryDoc>('plugin:memory|get_memory', { identifier: id })
      if (doc && this.activeDoc?.meta.id === id) {
        this.activeDoc.meta = doc.meta
        this.activeDoc.revision = doc.revision
        this.activeDoc.outgoing = doc.outgoing
        this.activeDoc.backlinks = doc.backlinks
        this.activeDoc.frontmatter = doc.frontmatter
      }
    },

    async create(input: {
      title: string
      body?: string
      tags?: string[]
      kind?: string
      author?: string
      scope?: string
    }): Promise<MemoryMeta | null> {
      const meta = await call<MemoryMeta>('plugin:memory|create_memory', {
        title: input.title,
        body: input.body ?? null,
        tags: input.tags ?? null,
        kind: input.kind ?? null,
        author: input.author ?? 'operator',
        scope: input.scope ?? this.createScope,
      })
      await this.loadAll()
      return meta
    },

    async saveBody(identifier: string, body: string): Promise<MemoryMeta | null> {
      const meta = await call<MemoryMeta>('plugin:memory|update_memory', { identifier, body })
      if (meta && this.activeDoc?.meta.id === meta.id) {
        this.activeDoc.meta = meta
        void this.refreshActive()
      }
      void this.loadAll()
      return meta
    },

    async rename(identifier: string, newTitle: string): Promise<MemoryMeta | null> {
      const meta = await call<MemoryMeta>('plugin:memory|rename_memory', { identifier, newTitle })
      if (meta && this.activeDoc) await this.open(meta.id)
      await this.loadAll()
      return meta
    },

    async setMeta(identifier: string, patch: { kind?: string; tags?: string[]; author?: string }) {
      const meta = await call<MemoryMeta>('plugin:memory|set_memory_meta', { identifier, ...patch })
      if (meta && this.activeDoc?.meta.id === meta.id) this.activeDoc.meta = meta
      void this.loadAll()
      return meta
    },

    async remove(identifier: string) {
      await call('plugin:memory|delete_memory', { identifier })
      if (this.activeDoc && this.byIdentifier(identifier)?.id === this.activeDoc.meta.id) {
        this.activeDoc = null
      }
      await this.loadAll()
    },

    async loadGraph() {
      const scope = this.viewScope
      const request = ++this.graphRequest
      this.graphLoading = true
      this.graphError = null
      try {
        const graph = await call<MemoryGraph>('plugin:memory|get_graph', { scope })
        if (scope === this.viewScope && request === this.graphRequest) this.graph = graph
      } catch (error) {
        if (scope === this.viewScope && request === this.graphRequest) this.graphError = String(error)
      } finally { if (request === this.graphRequest) this.graphLoading = false }
    },

    /** Trash is per vault: the view's scope (general in the general view). */
    async loadTrash() {
      const scope = this.scope
      try {
        const trash = (await call<TrashEntry[]>('plugin:memory|list_trash', { scope })) ?? []
        if (scope === this.scope) this.trash = trash
      } catch (error) { this.error = String(error) }
    },

    async restore(fileName: string) {
      await call('plugin:memory|restore_memory', { fileName, scope: this.scope })
      await Promise.all([this.loadTrash(), this.loadAll()])
    },

    async purgeTrash() {
      await call('plugin:memory|purge_trash', { scope: this.scope })
      await this.loadTrash()
      this.vaultInfo = await call<VaultInfo>('plugin:memory|get_vault_info')
    },

    /** Rebuild the index — the whole brain, every scope. */
    async reindex(): Promise<ScanReport | null> {
      const report = await call<ScanReport>('plugin:memory|reindex_vault', { scope: 'all' })
      await this.loadAll()
      return report
    },

    async setVaultPath(path: string | null) {
      this.vaultInfo = await call<VaultInfo>('plugin:memory|set_vault_path', { path })
      await this.loadAll()
    },
  },
})
