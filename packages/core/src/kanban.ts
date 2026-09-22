/**
 * The suite kanban (PLAN-KANBAN-UNIFY): one board per workspace plus the
 * suite-wide General board, all stored once in QuantMCP's kanban.db and
 * served by the `plugin:mcp` kanban commands. This module is the typed
 * client every surface renders from — QuantCanvas, the shell drawer and
 * the QuantMCP workspaces page draw the same cards.
 *
 * core.db still carries one entity per card (`core / kanban.card`, id
 * `core:kanban.card:mcp-<card id>`) — a READ mirror for Ctrl+K search and
 * the links table, written by the Rust side on every mutation. Its bus
 * events double as the change signal here. Never write the mirror.
 *
 * In the browser (plain `nuxt dev`) there is no board: `kanbanAvailable()`
 * is false and surfaces say so instead of faking one.
 */

import { on as busOn } from './bus'

/** Sentinel board id: the General board. Not a workspace — no folder, so
 *  its cards cannot be claimed by agents until moved to a workspace. */
export const GENERAL_BOARD_ID = 'general'

export type KanbanColumn = 'plan' | 'work' | 'review' | 'done'

export const KANBAN_COLUMNS: { id: KanbanColumn; label: string }[] = [
  { id: 'plan', label: 'Plan' },
  { id: 'work', label: 'Work' },
  { id: 'review', label: 'Review' },
  { id: 'done', label: 'Done' },
]

export type CardPriority = 'low' | 'medium' | 'high' | 'critical'

export const CARD_PRIORITIES: { id: CardPriority; label: string }[] = [
  { id: 'low', label: 'Low' },
  { id: 'medium', label: 'Medium' },
  { id: 'high', label: 'High' },
  { id: 'critical', label: 'Critical' },
]

/**
 * What the card editor (`QKanbanCardModal`) hands back. The host maps it
 * onto the commands: add for a new card, update + move for an existing one
 * (`update_kanban_card` carries no column).
 */
export interface KanbanCardDraft {
  title: string
  description: string
  column: KanbanColumn
  priority: CardPriority
}

export type CardStatus =
  | 'backlog'
  | 'in_progress'
  | 'awaiting_review'
  | 'approved'
  | 'merged'
  | 'rejected'
  | 'cancelled'

/** A card as the Rust side serialises it (snake_case on purpose). */
export interface KanbanCard {
  id: string
  /** Workspace entity id (`core:workspace:<b36>`) or [`GENERAL_BOARD_ID`]. */
  workspace_id: string
  title: string
  description: string
  column: KanbanColumn
  order: number
  priority: CardPriority
  status: CardStatus
  blocked_by: string[]
  created_at: number
  updated_at: number
  start_approved_at?: number | null
  claimed_at?: number | null
  completed_at?: number | null
  merged_at?: number | null
  agent_id?: string | null
  branch?: string | null
  worktree_path?: string | null
  /** Set once the operator archived the card off the Done column. Such a
   *  card is not in `listKanbanCards` — see `listArchivedKanbanCards`. */
  archived_at?: number | null
  /** Derived by the Rust side from `worktree_path`: the one-liners that
   *  run the app from the card's worktree, one per shell. */
  test_commands?: KanbanTestCommands | null
}

/** `worktree::TestCommands` — the same command for three shells. */
export interface KanbanTestCommands {
  powershell: string
  cmd: string
  bash: string
}

/** The card's core.db mirror entity id — what the links table keys on. */
export function kanbanMirrorId(cardId: string): string {
  return `core:kanban.card:mcp-${cardId}`
}

/**
 * An agent holds this card (claimed or under review): surfaces render it
 * read-only — its worktree and branch live outside the board.
 */
export function cardClaimed(card: KanbanCard): boolean {
  return card.agent_id != null || card.status === 'in_progress' || card.status === 'awaiting_review'
}

export function kanbanAvailable(): boolean {
  return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__
}

async function tauriInvoke() {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke
}

export async function listKanbanCards(boardId: string): Promise<KanbanCard[]> {
  if (!kanbanAvailable()) return []
  const invoke = await tauriInvoke()
  return invoke<KanbanCard[]>('plugin:mcp|list_kanban_cards', { workspaceId: boardId })
}

export async function addKanbanCard(input: {
  boardId: string
  title: string
  description?: string
  column?: KanbanColumn
  priority?: CardPriority
  /** Comma-separated card ids. */
  blockedBy?: string
}): Promise<KanbanCard> {
  const invoke = await tauriInvoke()
  return invoke<KanbanCard>('plugin:mcp|add_kanban_card', {
    workspaceId: input.boardId,
    title: input.title,
    description: input.description ?? '',
    column: input.column ?? 'plan',
    priority: input.priority,
    blockedBy: input.blockedBy,
  })
}

export async function updateKanbanCard(
  id: string,
  title: string,
  description: string,
  priority?: CardPriority,
  blockedBy?: string,
): Promise<KanbanCard> {
  const invoke = await tauriInvoke()
  return invoke<KanbanCard>('plugin:mcp|update_kanban_card', {
    id,
    title,
    description,
    priority,
    blockedBy,
  })
}

export async function moveKanbanCard(
  id: string,
  column: KanbanColumn,
  beforeId: string | null = null,
): Promise<void> {
  const invoke = await tauriInvoke()
  await invoke('plugin:mcp|move_kanban_card', { id, column, beforeId })
}

/** Re-home an unclaimed card onto another board (workspace or General). */
export async function moveKanbanCardToBoard(id: string, boardId: string): Promise<KanbanCard> {
  const invoke = await tauriInvoke()
  return invoke<KanbanCard>('plugin:mcp|move_kanban_card_to_workspace', {
    id,
    workspaceId: boardId,
  })
}

export async function deleteKanbanCard(id: string): Promise<void> {
  const invoke = await tauriInvoke()
  await invoke('plugin:mcp|delete_kanban_card', { id })
}

/** Human action from the card dialog; deliberately absent from MCP discovery. */
export async function reviewKanbanCard(id: string, action: 'start' | 'approve' | 'reject', reason?: string): Promise<string> {
  const invoke = await tauriInvoke()
  return invoke<string>('plugin:mcp|review_kanban_card', { id, action, reason })
}

// ── Archive ──
//
// "Archive all" on a board's Done column clears the finished cards off the
// board without deleting them: agents still list them (`list_kanban_cards`
// with archived=true) so earlier work is known and not redone.

/** A board's archived cards, newest archive first. */
export async function listArchivedKanbanCards(boardId: string): Promise<KanbanCard[]> {
  if (!kanbanAvailable()) return []
  const invoke = await tauriInvoke()
  return invoke<KanbanCard[]>('plugin:mcp|list_archived_kanban_cards', { workspaceId: boardId })
}

/** Archive every card in the board's Done column; resolves to the count. */
export async function archiveDoneKanbanCards(boardId: string): Promise<number> {
  const invoke = await tauriInvoke()
  return invoke<number>('plugin:mcp|archive_done_kanban_cards', { workspaceId: boardId })
}

/** Archive one card. Only cards in Done can be archived. */
export async function archiveKanbanCard(id: string): Promise<KanbanCard> {
  const invoke = await tauriInvoke()
  return invoke<KanbanCard>('plugin:mcp|archive_kanban_card', { id })
}

/** Put an archived card back on its board, at the end of its column. */
export async function unarchiveKanbanCard(id: string): Promise<KanbanCard> {
  const invoke = await tauriInvoke()
  return invoke<KanbanCard>('plugin:mcp|unarchive_kanban_card', { id })
}

/** How completed cards leave a board — see the mcp module's settings. On
 *  General nothing merges; the mode only governs the review step. */
export type ApprovalMode = 'auto_apply' | 'approval'

export async function getKanbanApprovalMode(boardId: string): Promise<ApprovalMode> {
  if (!kanbanAvailable()) return 'auto_apply'
  const invoke = await tauriInvoke()
  return invoke<ApprovalMode>('plugin:mcp|get_approval_mode', { workspaceId: boardId })
}

export async function setKanbanApprovalMode(boardId: string, mode: ApprovalMode): Promise<void> {
  const invoke = await tauriInvoke()
  await invoke('plugin:mcp|set_approval_mode', { workspaceId: boardId, mode })
}

/**
 * Fires on any card change in any window — the Rust mirror emits an entity
 * event per mutation; re-list to pick changes up.
 */
export function onKanbanChanged(cb: () => void): () => void {
  if (!kanbanAvailable()) return () => {}
  const offUpsert = busOn<{ kind: string }>('core.entity.upserted', (e) => {
    if (e.payload.kind === 'kanban.card') cb()
  })
  const offDelete = busOn<{ id: string }>('core.entity.deleted', (e) => {
    if (e.payload.id.startsWith('core:kanban.card:')) cb()
  })
  return () => {
    offUpsert()
    offDelete()
  }
}
