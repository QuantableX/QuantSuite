// The module's view of the suite workspace registry (PLAN-WORKSPACE-UNIFY).
//
// The old useProjects composable owned its own list (projects.json via
// plugin:mcp commands). That registry is gone: the folder list is the core.db
// workspace registry, read through @quantsuite/core exactly like QuantSpace's
// stores do. What stays module-owned is the per-workspace state — kanban
// cards, approval mode, index settings — keyed by the workspace entity id.
import {
  GENERAL_BOARD_ID,
  KANBAN_COLUMNS,
  archiveDoneKanbanCards,
  archiveKanbanCard,
  getActiveWorkspace,
  getSelectedKanbanBoard,
  listArchivedKanbanCards,
  listWorkspaces,
  onKanbanChanged,
  onKanbanBoardSelected,
  onWorkspacesChanged,
  openFolderAsWorkspace,
  removeWorkspace,
  samePath,
  selectKanbanBoard,
  setWorkspacePinned,
  sortWorkspaces,
  unarchiveKanbanCard,
  type CardPriority,
  type CardStatus,
  type KanbanCard,
  type KanbanCardDraft,
  type KanbanColumn,
  type Workspace,
} from '@quantsuite/core'

// The kanban vocabulary lives in @quantsuite/core since PLAN-KANBAN-UNIFY —
// one definition for every surface; re-exported for the module's pages.
export { KANBAN_COLUMNS }
export type { CardPriority, CardStatus, KanbanCard, KanbanCardDraft, KanbanColumn, Workspace }

export type ApprovalMode = 'auto_apply' | 'approval'

export interface CodebaseIndexStatus {
  workspace_id: string
  status: 'not_indexed' | 'indexing' | 'indexed'
  file_count: number
  indexed_at: number | null
  mode?: string | null
  fts_entry_count?: number | null
  chunk_count?: number | null
  structural_indexed_at?: number | null
  semantic_indexed_at?: number | null
}

export interface IndexCodebaseResult {
  status: string
  action?: string
  codebase?: string
  mode?: string
  files_indexed?: number
  files_skipped?: number
  errors?: number
  total_entries?: number
  structural_entries?: number
  semantic_entries?: number
  db_path?: string
  error?: string
}

/** What core.db remembers about how a workspace's index is built. */
export interface IndexSettings {
  mode?: string | null
  provider?: string | null
  model?: string | null
  baseUrl?: string | null
  filter?: string | null
}

// Selection id for the "General" entry in the workspace list. It is not a
// registry entry — no folder, no index — but since PLAN-KANBAN-UNIFY it
// carries the suite-wide General kanban board (plus the global AGENT.md).
// Folder-bound features (index, approval mode, worktrees) must still skip it.
export const GENERAL_WORKSPACE_ID = GENERAL_BOARD_ID

async function tauriInvoke() {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke
}

function hasTauri(): boolean {
  return import.meta.client && !!window.__TAURI_INTERNALS__
}

export function useMcpWorkspaces() {
  const workspaces = useState<Workspace[]>('mcp-workspaces', () => [])
  const activePath = useState<string | null>('mcp-active-workspace-path', () => null)
  const selectedWorkspaceId = useState<string | null>('mcp-selected-workspace-id', () => null)
  const cards = useState<KanbanCard[]>('kanban-cards', () => [])
  // The selected board's archive — what "Archive all" took off the Done
  // column. Loaded with the board so the header can show its count.
  const archivedCards = useState<KanbanCard[]>('kanban-archived-cards', () => [])
  const loading = useState<boolean>('mcp-workspaces-loading', () => false)
  const cardsRequest = useState<number>('mcp-cards-request', () => 0)
  const archiveRequest = useState<number>('mcp-archive-request', () => 0)
  const cardsError = useState<string | null>('mcp-cards-error', () => null)

  const selectedWorkspace = computed(
    () => workspaces.value.find((w) => w.id === selectedWorkspaceId.value) ?? null,
  )

  const isGeneralSelected = computed(
    () => selectedWorkspaceId.value === GENERAL_WORKSPACE_ID,
  )

  function isActive(w: Workspace): boolean {
    return samePath(w.path, activePath.value)
  }

  /** Registry read. Defaults the selection to the suite's active workspace. */
  async function refresh() {
    loading.value = true
    try {
      const [list, active] = await Promise.all([listWorkspaces(), getActiveWorkspace()])
      workspaces.value = sortWorkspaces(list)
      activePath.value = active?.path ?? null

      const preferred = getSelectedKanbanBoard() ?? selectedWorkspaceId.value
      const id = preferred === GENERAL_WORKSPACE_ID || workspaces.value.some(w => w.id === preferred)
        ? preferred!
        : preferred ? GENERAL_WORKSPACE_ID : (workspaces.value.find(isActive)?.id ?? GENERAL_WORKSPACE_ID)
      await selectWorkspace(id)
    } finally {
      loading.value = false
    }
  }

  /**
   * Re-read on registry changes from anywhere in the suite — and re-list the
   * selected board's cards on any card change. The MCP server's tools (claim,
   * complete, approve, move …) write kanban.db behind this page's back, and
   * the mirror's bus event is the only sign that they did; the drawer always
   * listened for it, this page did not, so a card an agent had merged kept
   * showing "in progress" here until a remount (2026-09-03).
   */
  function subscribe(): () => void {
    const offSelection = onKanbanBoardSelected(id => {
      if (id !== selectedWorkspaceId.value) void selectWorkspace(id)
    })
    const offWorkspaces = onWorkspacesChanged(() => {
      refresh()
    })
    const offKanban = onKanbanChanged(() => {
      const id = selectedWorkspaceId.value
      if (id) {
        void refreshCards(id)
        void refreshArchived(id)
      }
    })
    return () => {
      offWorkspaces()
      offKanban()
      offSelection()
    }
  }

  async function refreshCards(workspaceId: string) {
    if (!hasTauri()) return
    const request = ++cardsRequest.value
    try {
      const invoke = await tauriInvoke()
      const result = await invoke<KanbanCard[]>('plugin:mcp|list_kanban_cards', { workspaceId })
      if (request !== cardsRequest.value || workspaceId !== selectedWorkspaceId.value) return
      cards.value = result
      cardsError.value = null
    } catch (e) {
      if (request !== cardsRequest.value || workspaceId !== selectedWorkspaceId.value) return
      cardsError.value = String(e)
      console.error('Failed to load kanban cards:', e)
    }
  }

  async function refreshArchived(workspaceId: string) {
    if (!hasTauri()) return
    const request = ++archiveRequest.value
    try {
      const result = await listArchivedKanbanCards(workspaceId)
      if (request === archiveRequest.value && workspaceId === selectedWorkspaceId.value) archivedCards.value = result
    } catch (e) {
      console.error('Failed to load the kanban archive:', e)
    }
  }

  // The General entry is a board like any other — refreshCards('general')
  // lists the suite-wide General board.
  async function selectWorkspace(id: string) {
    if (selectedWorkspaceId.value !== id) {
      cards.value = []
      archivedCards.value = []
      cardsError.value = null
    }
    selectedWorkspaceId.value = id
    selectKanbanBoard(id)
    await Promise.all([refreshCards(id), refreshArchived(id)])
  }

  /** Folder dialog → registers + opens the folder suite-wide, selects it here. */
  async function addFolder(): Promise<Workspace | null> {
    const ws = await openFolderAsWorkspace()
    if (ws) {
      await refresh()
      await selectWorkspace(ws.id)
    }
    return ws
  }

  /** Forget the workspace suite-wide. Folder and kanban cards stay on disk. */
  async function forgetWorkspace(id: string): Promise<void> {
    await removeWorkspace(id)
    if (selectedWorkspaceId.value === id) {
      selectedWorkspaceId.value = null
      cards.value = []
    }
    await refresh()
  }

  async function pinWorkspace(id: string, pinned: boolean): Promise<void> {
    await setWorkspacePinned(id, pinned)
    await refresh()
  }

  // ── Kanban ──

  async function addCard(
    workspaceId: string,
    title: string,
    description: string,
    column: KanbanColumn,
    priority?: CardPriority,
    blockedBy?: string,
  ): Promise<KanbanCard> {
    const invoke = await tauriInvoke()
    const card = await invoke<KanbanCard>('plugin:mcp|add_kanban_card', {
      workspaceId,
      title,
      description,
      column,
      priority: priority || undefined,
      blockedBy: blockedBy || undefined,
    })
    await refreshCards(workspaceId)
    return card
  }

  async function updateCard(
    id: string,
    title: string,
    description: string,
    priority?: CardPriority,
    blockedBy?: string,
  ): Promise<void> {
    const invoke = await tauriInvoke()
    await invoke('plugin:mcp|update_kanban_card', {
      id,
      title,
      description,
      priority: priority || undefined,
      blockedBy: blockedBy || undefined,
    })
    if (selectedWorkspaceId.value) await refreshCards(selectedWorkspaceId.value)
  }

  async function moveCard(
    id: string,
    column: KanbanColumn,
    beforeId: string | null = null,
  ): Promise<void> {
    const invoke = await tauriInvoke()
    await invoke('plugin:mcp|move_kanban_card', { id, column, beforeId })
    if (selectedWorkspaceId.value) await refreshCards(selectedWorkspaceId.value)
  }

  async function deleteCard(id: string): Promise<void> {
    const invoke = await tauriInvoke()
    await invoke('plugin:mcp|delete_kanban_card', { id })
    if (selectedWorkspaceId.value) await refreshCards(selectedWorkspaceId.value)
  }

  // ── Archive ──

  /** "Archive all" on the Done column of the selected board. */
  async function archiveDoneCards(workspaceId: string): Promise<number> {
    const n = await archiveDoneKanbanCards(workspaceId)
    await Promise.all([refreshCards(workspaceId), refreshArchived(workspaceId)])
    return n
  }

  /** Archive one Done card and refresh the board and archive count. */
  async function archiveCard(id: string): Promise<void> {
    await archiveKanbanCard(id)
    const wid = selectedWorkspaceId.value
    if (wid) await Promise.all([refreshCards(wid), refreshArchived(wid)])
  }

  /** Restore one card from the archive to the end of its column. */
  async function unarchiveCard(id: string): Promise<void> {
    await unarchiveKanbanCard(id)
    const wid = selectedWorkspaceId.value
    if (wid) await Promise.all([refreshCards(wid), refreshArchived(wid)])
  }

  // ── Per-workspace module settings (core.db, scope "mcp") ──

  async function getApprovalMode(workspaceId: string): Promise<ApprovalMode> {
    if (!hasTauri()) return 'auto_apply'
    const invoke = await tauriInvoke()
    const mode = await invoke<string>('plugin:mcp|get_approval_mode', { workspaceId })
    return mode as ApprovalMode
  }

  async function setApprovalMode(workspaceId: string, mode: ApprovalMode): Promise<void> {
    const invoke = await tauriInvoke()
    await invoke('plugin:mcp|set_approval_mode', { workspaceId, mode })
  }

  async function getIndexSettings(workspaceId: string): Promise<IndexSettings> {
    if (!hasTauri()) return {}
    const invoke = await tauriInvoke()
    return invoke<IndexSettings>('plugin:mcp|get_workspace_index_settings', { workspaceId })
  }

  async function setIndexSettings(workspaceId: string, settings: IndexSettings): Promise<void> {
    if (!hasTauri()) return
    const invoke = await tauriInvoke()
    await invoke('plugin:mcp|set_workspace_index_settings', {
      workspaceId,
      indexSettings: settings,
    })
  }

  // ── Codebase index ──

  async function getIndexStats(workspaceId: string): Promise<CodebaseIndexStatus> {
    if (!hasTauri()) {
      return { workspace_id: workspaceId, status: 'not_indexed', file_count: 0, indexed_at: null }
    }
    const invoke = await tauriInvoke()
    return invoke<CodebaseIndexStatus>('plugin:mcp|get_codebase_index_stats', { workspaceId })
  }

  async function indexWorkspaceCodebase(
    workspaceId: string,
    mode: string = 'structural',
    embedProvider?: string,
    embedModel?: string,
    embedBaseUrl?: string,
    forceReindex?: boolean,
    filterMode?: string,
  ): Promise<IndexCodebaseResult> {
    const invoke = await tauriInvoke()
    return invoke<IndexCodebaseResult>('plugin:mcp|index_project_codebase', {
      workspaceId,
      mode,
      embedProvider,
      embedModel,
      embedBaseUrl,
      forceReindex,
      filterMode,
    })
  }

  function cardsForColumn(column: KanbanColumn): KanbanCard[] {
    return cards.value.filter((c) => c.column === column).sort((a, b) => a.order - b.order)
  }

  return {
    workspaces,
    activePath,
    selectedWorkspaceId,
    selectedWorkspace,
    isGeneralSelected,
    cards,
    cardsError,
    archivedCards,
    loading,
    isActive,
    cardsForColumn,
    refresh,
    subscribe,
    refreshCards,
    refreshArchived,
    selectWorkspace,
    addFolder,
    forgetWorkspace,
    pinWorkspace,
    addCard,
    updateCard,
    moveCard,
    deleteCard,
    archiveDoneCards,
    archiveCard,
    unarchiveCard,
    getApprovalMode,
    setApprovalMode,
    getIndexSettings,
    setIndexSettings,
    getIndexStats,
    indexWorkspaceCodebase,
  }
}
