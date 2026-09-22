<script setup lang="ts">
/**
 * Shared kanban surface for the drawer and QuantCanvas (PLAN-KANBAN-UNIFY).
 *
 * No store of its own any more: every board lives once in QuantMCP's
 * kanban.db — one per workspace plus the suite-wide General board — and
 * this tab is a view on any of them: a dropdown selects the board,
 * General first, then every registered workspace (user-decided shape;
 * a two-pill General⇄active switcher came before it and was rejected).
 *
 * Cards an agent holds (claimed / awaiting review) render read-only with
 * the agent badge — their worktree lives outside the board. The "→" action
 * re-homes an unclaimed card onto another board (that is the old
 * hand-to-agent flow: move it to a workspace, agents see it in
 * `list_kanban_cards` there and can claim it).
 *
 * Drag & drop is pointer-event based on purpose — HTML5 DnD is broken in
 * WebView2 (see the pointer-dnd rule; /mcp/projects does the same).
 */
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  KANBAN_COLUMNS,
  GENERAL_BOARD_ID,
  addKanbanCard,
  archiveKanbanCard,
  cardClaimed,
  deleteKanbanCard,
  getKanbanApprovalMode,
  getActiveWorkspace,
  getSelectedKanbanBoard,
  kanbanAvailable,
  kanbanMirrorId,
  listKanbanCards,
  listWorkspaces,
  moveKanbanCard,
  moveKanbanCardToBoard,
  onKanbanChanged,
  onKanbanBoardSelected,
  onWorkspacesChanged,
  qs,
  selectKanbanBoard,
  setKanbanApprovalMode,
  sortWorkspaces,
  updateKanbanCard,
  workspaceIdFor,
  type ApprovalMode,
  type Entity,
  type KanbanCard,
  type KanbanCardDraft,
  type KanbanColumn,
  type Workspace,
} from '@quantsuite/core'

// Canvas binds its workspace board; the drawer keeps its suite-wide picker.
// A bound board never follows or changes another surface's board selection.
const props = defineProps<{ boardId?: string }>()
const available = kanbanAvailable()
const boardElement = ref<HTMLElement | null>(null)

const cards = ref<KanbanCard[]>([])
const actionError = ref<string | null>(null)

/* ── Board selection: General on top, then every workspace ───────────── */

const boardId = ref<string>(props.boardId ?? getSelectedKanbanBoard() ?? GENERAL_BOARD_ID)
const workspaces = ref<Workspace[]>([])

/** Dropdown rows: General first, then the registry one by one. */
const boards = computed(() => [
  { id: GENERAL_BOARD_ID, label: 'General' },
  ...workspaces.value.map((w) => ({ id: w.id, label: w.name })),
])

async function refreshWorkspaces() {
  if (!available) return
  workspaces.value = sortWorkspaces(await listWorkspaces().catch(() => []))
  // The selected board's workspace was forgotten — fall back to General.
  if (!props.boardId && boardId.value !== GENERAL_BOARD_ID && !workspaces.value.some((w) => w.id === boardId.value)) {
    await showBoard(GENERAL_BOARD_ID)
  }
}

async function onBoardSelect(e: Event) {
  const id = (e.target as HTMLSelectElement).value
  await showBoard(id)
}

async function showBoard(id: string) {
  if (boardId.value === id) return
  boardId.value = id
  cards.value = []
  actionError.value = null
  showCardModal.value = false
  editingCard.value = null
  linksFor.value = null
  handing.value = null
  resetDragState()
  if (!props.boardId) selectKanbanBoard(id)
  await Promise.all([reload(), loadApprovalMode()])
}

watch(() => props.boardId, id => {
  void showBoard(id ?? getSelectedKanbanBoard() ?? GENERAL_BOARD_ID)
})

function openMcpBoard() {
  selectKanbanBoard(boardId.value)
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/mcp/projects' } }))
}

/* ── Approval mode (same dropdown as QuantMCP → Workspaces) ──────────── */

const approvalMode = ref<ApprovalMode>('auto_apply')

async function loadApprovalMode() {
  const id = boardId.value
  const mode = await getKanbanApprovalMode(id).catch(() => 'auto_apply' as const)
  if (id === boardId.value) approvalMode.value = mode
}

async function onApprovalModeChange(e: Event) {
  const mode = (e.target as HTMLSelectElement).value as ApprovalMode
  try {
    await setKanbanApprovalMode(boardId.value, mode)
    approvalMode.value = mode
  } catch (err) {
    actionError.value = `Mode change failed: ${err instanceof Error ? err.message : err}`
  }
}

// Same fold rule as the workspaces page: in auto_apply an empty review
// column steps out of the flow; it always shows while it holds cards.
const visibleColumns = computed(() =>
  approvalMode.value === 'auto_apply' && byColumn('review').length === 0
    ? KANBAN_COLUMNS.filter((col) => col.id !== 'review')
    : KANBAN_COLUMNS,
)

/* ── Cards ───────────────────────────────────────────────────────────── */

const byColumn = (column: KanbanColumn) =>
  cards.value.filter((c) => c.column === column && c.status !== 'cancelled').sort((a, b) => a.order - b.order)

let cardsRequest = 0
async function reload() {
  if (!available) return
  const id = boardId.value
  const request = ++cardsRequest
  try {
    const result = await listKanbanCards(id)
    if (id !== boardId.value || request !== cardsRequest) return
    cards.value = result
    actionError.value = null
  } catch (error) {
    if (id === boardId.value && request === cardsRequest) actionError.value = `Board could not load: ${String(error)}`
  }
}

/* ── Card editor — QKanbanCardModal, the one QuantMCP → Workspaces opens ── */

const showCardModal = ref(false)
const editingCard = ref<KanbanCard | null>(null)
const newCardColumn = ref<KanbanColumn>('plan')
const cardModalBusy = ref(false)
const cardModalError = ref<string | null>(null)

const boardLabel = computed(() => boards.value.find((b) => b.id === boardId.value)?.label ?? '')

function openAddCard(column: KanbanColumn) {
  editingCard.value = null
  newCardColumn.value = column
  cardModalError.value = null
  showCardModal.value = true
}

/** A held card opens too — read-only, the full spec is still worth reading. */
function openCard(card: KanbanCard) {
  editingCard.value = card
  cardModalError.value = null
  showCardModal.value = true
}

async function saveCard(draft: KanbanCardDraft, card: KanbanCard | null) {
  cardModalBusy.value = true
  cardModalError.value = null
  try {
    if (card) {
      await updateKanbanCard(card.id, draft.title, draft.description, draft.priority)
      // update carries no column — a changed one goes through move
      if (draft.column !== card.column) await moveKanbanCard(card.id, draft.column)
    } else {
      await addKanbanCard({
        boardId: boardId.value,
        title: draft.title,
        description: draft.description,
        column: draft.column,
        priority: draft.priority,
      })
    }
    showCardModal.value = false
    await reload() // the bus echo confirms too; this paints it immediately
  } catch (e) {
    cardModalError.value = e instanceof Error ? e.message : String(e)
  } finally {
    cardModalBusy.value = false
  }
}

async function archiveCard(card: KanbanCard) {
  if (card.column !== 'done' || card.archived_at != null || cardClaimed(card) || cardModalBusy.value) return
  actionError.value = null
  cardModalBusy.value = true
  cardModalError.value = null
  try {
    await archiveKanbanCard(card.id)
    showCardModal.value = false
    await reload()
  } catch (e) {
    cardModalError.value = e instanceof Error ? e.message : String(e)
  } finally {
    cardModalBusy.value = false
  }
}

async function deleteFromEditor(card: KanbanCard) {
  showCardModal.value = false
  await removeCard(card)
}

async function removeCard(card: KanbanCard) {
  if (cardClaimed(card)) return
  cards.value = cards.value.filter((c) => c.id !== card.id)
  await deleteKanbanCard(card.id).catch((e) => {
    actionError.value = `Delete failed: ${e instanceof Error ? e.message : e}`
  })
}

/* ── Pointer drag & drop (no HTML5 DnD — broken in WebView2) ─────────── */

const draggingCardId = ref<string | null>(null)
const dropTarget = ref<{ column: KanbanColumn; beforeId: string | null } | null>(null)

let pointerMoveHandler: ((e: PointerEvent) => void) | null = null
let pointerUpHandler: ((e: PointerEvent) => void) | null = null
let pointerStartX = 0
let pointerStartY = 0
let pointerDidMove = false

function removePointerListeners() {
  if (pointerMoveHandler) {
    window.removeEventListener('pointermove', pointerMoveHandler, true)
    pointerMoveHandler = null
  }
  if (pointerUpHandler) {
    window.removeEventListener('pointerup', pointerUpHandler, true)
    window.removeEventListener('pointercancel', pointerUpHandler, true)
    pointerUpHandler = null
  }
}

function resetDragState() {
  removePointerListeners()
  draggingCardId.value = null
  dropTarget.value = null
}

function findDropTargetAtPoint(x: number, y: number, draggingId: string) {
  const element = document.elementFromPoint(x, y) as HTMLElement | null
  const columnElement = element?.closest('[data-dkb-column]') as HTMLElement | null
  const columnId = columnElement?.dataset.dkbColumn as KanbanColumn | undefined
  // Multiple Canvas windows and the drawer can be visible at once. A drop
  // outside this instance must not reorder a card on an unrelated board.
  if (!columnId || !boardElement.value?.contains(columnElement)) {
    dropTarget.value = null
    return null
  }

  const cardElement = element?.closest('[data-dkb-card-id]') as HTMLElement | null
  const hoveredCardId = cardElement?.dataset.dkbCardId ?? null

  if (!hoveredCardId || hoveredCardId === draggingId) {
    dropTarget.value = { column: columnId, beforeId: null }
    return dropTarget.value
  }

  const columnCards = byColumn(columnId).filter((c) => c.id !== draggingId)
  const hoveredIdx = columnCards.findIndex((c) => c.id === hoveredCardId)
  if (hoveredIdx === -1) {
    dropTarget.value = { column: columnId, beforeId: null }
    return dropTarget.value
  }

  const rect = cardElement!.getBoundingClientRect()
  const before = y <= rect.top + rect.height / 2
  const beforeId = before ? hoveredCardId : (columnCards[hoveredIdx + 1]?.id ?? null)
  dropTarget.value = { column: columnId, beforeId }
  return dropTarget.value
}

function onCardPointerDown(e: PointerEvent, card: KanbanCard) {
  if (e.button !== 0 || cardClaimed(card)) return
  const target = e.target as HTMLElement | null
  if (target?.closest('button, a, input, textarea, [data-no-drag]')) return
  e.preventDefault()

  pointerStartX = e.clientX
  pointerStartY = e.clientY
  pointerDidMove = false

  pointerMoveHandler = (ev: PointerEvent) => {
    if (!pointerDidMove) {
      if (Math.abs(ev.clientX - pointerStartX) < 4 && Math.abs(ev.clientY - pointerStartY) < 4) return
      pointerDidMove = true
      draggingCardId.value = card.id
    }
    findDropTargetAtPoint(ev.clientX, ev.clientY, card.id)
  }
  pointerUpHandler = async (ev: PointerEvent) => {
    const target = pointerDidMove ? findDropTargetAtPoint(ev.clientX, ev.clientY, card.id) : null
    resetDragState()
    if (!target) return
    if (target.column === card.column && target.beforeId === null && byColumn(card.column).at(-1)?.id === card.id)
      return
    try {
      await moveKanbanCard(card.id, target.column, target.beforeId)
      await reload()
    } catch (e2) {
      actionError.value = `Move failed: ${e2 instanceof Error ? e2.message : e2}`
    }
  }
  window.addEventListener('pointermove', pointerMoveHandler, true)
  window.addEventListener('pointerup', pointerUpHandler, true)
  window.addEventListener('pointercancel', pointerUpHandler, true)
}

function isDropBefore(column: KanbanColumn, cardId: string): boolean {
  return draggingCardId.value !== null && dropTarget.value?.column === column && dropTarget.value?.beforeId === cardId
}

/* ── Re-home: move a card to another board (the hand-over) ───────────── */

const handing = ref<string | null>(null)

const handTargets = computed(() =>
  boards.value
    .filter((b) => b.id !== boardId.value)
    .map((b) => ({ id: b.id, name: b.label })),
)

function startHand(card: KanbanCard) {
  actionError.value = null
  handing.value = handing.value === card.id ? null : card.id
  if (handing.value) void refreshWorkspaces()
}

async function handToBoard(card: KanbanCard, targetId: string) {
  handing.value = null
  try {
    await moveKanbanCardToBoard(card.id, targetId)
    await reload()
  } catch (e) {
    actionError.value = `Move failed: ${e instanceof Error ? e.message : e}`
  }
}

/* ── Links (E5): the core.db links table, keyed on the card's mirror ──── */

const linksFor = ref<string | null>(null)
const outLinks = ref<Entity[]>([])
const inLinks = ref<Entity[]>([])
const linkQuery = ref('')
const linkResults = ref<Entity[]>([])

async function toggleLinks(card: KanbanCard) {
  actionError.value = null
  if (linksFor.value === card.id) {
    linksFor.value = null
    return
  }
  linkQuery.value = ''
  linkResults.value = []
  const mid = kanbanMirrorId(card.id)
  outLinks.value = await qs.core.linkedEntities(mid, false).catch(() => [])
  inLinks.value = await qs.core.linkedEntities(mid, true).catch(() => [])
  linksFor.value = card.id
}

// Debounced and tokened like the command palette: one FTS round-trip per
// keystroke, and an older query resolving late must not overwrite a newer one.
let linkToken = 0
let linkTimer: ReturnType<typeof setTimeout> | null = null

function onLinkQuery() {
  const token = ++linkToken
  if (linkTimer) clearTimeout(linkTimer)
  const q = linkQuery.value.trim()
  if (!q || !linksFor.value) {
    linkResults.value = []
    return
  }
  const mid = kanbanMirrorId(linksFor.value)
  linkTimer = setTimeout(async () => {
    try {
      const found = await qs.core.searchEntities(q, 8)
      if (token !== linkToken) return
      linkResults.value = found.filter((e) => e.id !== mid && !outLinks.value.some((l) => l.id === e.id))
    } catch {
      if (token !== linkToken) return
      linkResults.value = []
    }
  }, 200)
}

async function addLink(target: Entity) {
  if (!linksFor.value) return
  const mid = kanbanMirrorId(linksFor.value)
  await qs.core.linkEntities({ src: mid, dst: target.id, rel: 'references' }).catch(() => {})
  outLinks.value = await qs.core.linkedEntities(mid, false).catch(() => outLinks.value)
  linkQuery.value = ''
  linkResults.value = []
}

async function removeLink(target: Entity, incoming: boolean) {
  if (!linksFor.value) return
  const mid = kanbanMirrorId(linksFor.value)
  const link = incoming
    ? { src: target.id, dst: mid, rel: 'references' }
    : { src: mid, dst: target.id, rel: 'references' }
  await qs.core.unlinkEntities(link).catch(() => {})
  outLinks.value = await qs.core.linkedEntities(mid, false).catch(() => [])
  inLinks.value = await qs.core.linkedEntities(mid, true).catch(() => [])
}

function followLink(entity: Entity) {
  // Cards live on a board already; everything else navigates its route.
  if (entity.kind === 'kanban.card') return
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: entity.route } }))
}

/* ── Lifecycle ───────────────────────────────────────────────────────── */

let offKanban: (() => void) | undefined
let offWorkspaces: (() => void) | undefined
let offSelection: (() => void) | undefined
let disposed = false

onMounted(async () => {
  offSelection = onKanbanBoardSelected(id => { if (!props.boardId) void showBoard(id) })
  offKanban = onKanbanChanged(reload)
  offWorkspaces = onWorkspacesChanged(() => { void refreshWorkspaces() })
  if (!props.boardId && !getSelectedKanbanBoard()) {
    const active = await getActiveWorkspace().catch(() => null)
    // A selection made during the lookup wins over the default.
    if (disposed) return
    if (!props.boardId && !getSelectedKanbanBoard()) {
      boardId.value = active?.path ? workspaceIdFor(active.path) : GENERAL_BOARD_ID
      selectKanbanBoard(boardId.value)
    }
  }
  await refreshWorkspaces()
  if (disposed) return
  await Promise.all([reload(), loadApprovalMode()])
})
onActivated(() => { void Promise.all([reload(), loadApprovalMode()]) })
onDeactivated(() => {
  resetDragState()
  showCardModal.value = false
})
onUnmounted(() => {
  disposed = true
  resetDragState()
  offKanban?.()
  offWorkspaces?.()
  offSelection?.()
})
</script>

<template>
  <div ref="boardElement" class="dkb">
    <p v-if="!available" class="dkb-note">The kanban boards live in the app — open QuantSuite itself.</p>
    <template v-else>
      <p v-if="actionError" class="dkb-error" role="alert">{{ actionError }}</p>

      <!-- One grid for header AND columns: the dropdowns sit in the same
           tracks as the outer columns, so they align exactly and scroll
           together (two separate flex rows drifted apart). -->
      <div class="dkb-grid" :style="{ '--dkb-cols': visibleColumns.length }">
        <span v-if="props.boardId" class="dkb-board-label dkb-select-left" :title="boardLabel">{{ boardLabel }}</span>
        <select v-else class="dkb-select dkb-select-left" :value="boardId" aria-label="Board" @change="onBoardSelect">
          <option v-for="b in boards" :key="b.id" :value="b.id">{{ b.label }}</option>
        </select>
        <button class="dkb-title dkb-mcp-link" title="Open this board in MCP Workspaces" @click="openMcpBoard">Open in MCP ↗</button>
        <select
          class="dkb-select dkb-select-right"
          :value="approvalMode"
          aria-label="Approval mode"
          title="Auto Apply: start immediately and merge completed work. Approval: approve the plan before work, then approve the result before merging."
          @change="onApprovalModeChange"
        >
          <option value="auto_apply">Auto Apply</option>
          <option value="approval">Approval</option>
        </select>

        <section
          v-for="col in visibleColumns"
          :key="col.id"
          class="dkb-col"
          :class="{ 'is-over': draggingCardId && dropTarget?.column === col.id }"
          :data-dkb-column="col.id"
        >
          <header class="dkb-col-head">
            <span>{{ col.label }}</span>
            <span class="dkb-count">{{ byColumn(col.id).length }}</span>
          </header>

          <div class="dkb-cards">
            <article
              v-for="card in byColumn(col.id)"
              :key="card.id"
              class="dkb-card"
              :class="{
                'is-dragging': draggingCardId === card.id,
                'is-agent': cardClaimed(card),
                'is-drop-before': isDropBefore(col.id, card.id),
              }"
              :data-dkb-card-id="card.id"
              :title="cardClaimed(card) ? 'An agent is working this card — read-only here' : undefined"
              @pointerdown="onCardPointerDown($event, card)"
              @dblclick="openCard(card)"
            >
              <span class="dkb-card-title">{{ card.title }}</span>

              <span v-if="cardClaimed(card)" class="dkb-agent-badge">
                {{ card.agent_id ? `agent · ${card.agent_id}` : 'agent' }}
              </span>
              <button class="dkb-mini" :title="cardClaimed(card) ? 'Open card' : 'Edit card'" @click="openCard(card)">
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                </svg>
              </button>
              <button class="dkb-mini" title="Links" @click="toggleLinks(card)">
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7" />
                  <path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7" />
                </svg>
              </button>
              <template v-if="!cardClaimed(card)">
                <button class="dkb-mini" title="Move to another board" @click="startHand(card)">
                  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
                  </svg>
                </button>
                <button class="dkb-mini" title="Delete card" @click="removeCard(card)">&times;</button>
              </template>

              <!-- Links: outgoing and incoming, unlink, and search-to-link -->
              <div v-if="linksFor === card.id" class="dkb-links" data-no-drag @click.stop>
                <span v-for="l in outLinks" :key="'o' + l.id" class="dkb-link-chip" @click="followLink(l)">
                  → {{ l.title }}
                  <button title="Unlink" @click.stop="removeLink(l, false)">&times;</button>
                </span>
                <span v-for="l in inLinks" :key="'i' + l.id" class="dkb-link-chip is-in" @click="followLink(l)">
                  ← {{ l.title }}
                  <button title="Unlink" @click.stop="removeLink(l, true)">&times;</button>
                </span>
                <input v-model="linkQuery" class="dkb-link-search" placeholder="Link to…" @input="onLinkQuery" />
                <button v-for="r in linkResults" :key="r.id" class="dkb-link-result" @click="addLink(r)">
                  + {{ r.title }} <em>{{ r.kind }}</em>
                </button>
              </div>

              <!-- Board picker for the re-home -->
              <div v-if="handing === card.id" class="dkb-hand" data-no-drag @click.stop>
                <span>To which board?</span>
                <button v-for="t in handTargets" :key="t.id" class="dkb-hand-p" @click="handToBoard(card, t.id)">
                  {{ t.name }}
                </button>
                <span v-if="!handTargets.length">No other boards — open a folder as a workspace first.</span>
              </div>
            </article>
          </div>

          <button class="dkb-new" @click="openAddCard(col.id)">+ Add card</button>
        </section>
      </div>

      <!-- Card editor — the suite-wide QKanbanCardModal; held cards open read-only -->
      <QKanbanCardModal
        v-model="showCardModal"
        :card="editingCard"
        :column="newCardColumn"
        :columns="visibleColumns"
        :board-label="boardLabel"
        :approval-mode="approvalMode"
        :readonly="editingCard ? cardClaimed(editingCard) : false"
        :busy="cardModalBusy"
        :error="cardModalError"
        @save="saveCard"
        @delete="deleteFromEditor"
        @archive="archiveCard"
      />
    </template>
  </div>
</template>

<style scoped>
.dkb {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.dkb-note {
  margin: 12px 14px;
  color: var(--qss-text-muted);
  font-size: 12px;
}

/* Header and columns share these tracks — row 1 holds the two dropdowns
 * and the title, row 2 the columns. Same track = same width, always. */
.dkb-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(var(--dkb-cols, 4), minmax(220px, 1fr));
  grid-template-rows: auto minmax(0, 1fr);
  gap: 8px 10px;
  padding: 8px 14px 12px;
  overflow: auto;
}

.dkb-select {
  min-width: 0;
  grid-row: 1;
  padding: 4px 9px;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  background: var(--qss-bg-raised);
  color: var(--qss-text);
  font: 500 11.5px/1.5 var(--qss-font-sans);
  cursor: pointer;
}
.dkb-board-label {
  min-width: 0;
  grid-row: 1;
  align-self: center;
  overflow-wrap: anywhere;
  color: var(--qss-text);
  font: 600 12px/1.5 var(--qss-font-sans);
}
.dkb-select:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}
.dkb-select-left {
  grid-column: 1;
}
.dkb-select-right {
  grid-column: -2;
}

.dkb-title {
  grid-row: 1;
  grid-column: 2 / -2;
  align-self: center;
  text-align: center;
  color: var(--qss-text-secondary);
  font: 600 11px/1 var(--qss-font-sans);
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.dkb-error {
  margin: 8px 14px 0;
  padding: 4px 10px;
  border: 1px solid color-mix(in srgb, var(--qss-error) 40%, transparent);
  border-radius: 7px;
  background: color-mix(in srgb, var(--qss-error) 9%, transparent);
  color: var(--qss-text);
  font-size: 11.5px;
}

.dkb-mcp-link { border: 0; background: transparent; cursor: pointer; }
.dkb-mcp-link:hover { color: var(--qss-text); }

.dkb-col {
  grid-row: 2;
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 10px;
  background: color-mix(in srgb, var(--qss-bg-card) 35%, transparent);
  transition: border-color 120ms ease, background 120ms ease;
}
.dkb-col.is-over {
  border-color: color-mix(in srgb, var(--qss-accent) 55%, var(--qss-border));
  background: color-mix(in srgb, var(--qss-accent) 6%, transparent);
}

.dkb-col-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 11px 6px;
  color: var(--qss-text-secondary);
  font: 600 11px/1 var(--qss-font-sans);
  letter-spacing: 0.03em;
  text-transform: uppercase;
}
.dkb-count {
  font: 500 10px/1 var(--qss-font-mono);
  color: var(--qss-text-muted);
}

.dkb-cards {
  flex: 1;
  min-height: 24px;
  overflow-y: auto;
  padding: 2px 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.dkb-card {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  flex-wrap: wrap;
  padding: 8px 10px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.dkb-card.is-dragging {
  opacity: 0.45;
}
.dkb-card.is-agent {
  cursor: default;
  border-style: dashed;
}
.dkb-card.is-drop-before {
  box-shadow: 0 -2px 0 0 var(--qss-accent);
}
.dkb-card-title {
  flex: 1;
  min-width: 0;
  color: var(--qss-text);
  font-size: 12.5px;
  line-height: 1.35;
  overflow-wrap: anywhere;
}

.dkb-agent-badge {
  flex-shrink: 0;
  padding: 2px 7px;
  border-radius: 999px;
  border: 1px solid color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
  color: var(--qss-accent);
  font: 500 9.5px/1.4 var(--qss-font-mono);
}

.dkb-mini {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--qss-text-muted);
  font-size: 12px;
  cursor: pointer;
  opacity: 0;
}
.dkb-card:hover .dkb-mini {
  opacity: 1;
}
.dkb-mini:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.dkb-links {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
  padding-top: 6px;
  border-top: 1px solid var(--qss-border-subtle);
}
.dkb-link-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  color: var(--qss-text-secondary);
  font: 500 10px/1.5 var(--qss-font-sans);
  cursor: pointer;
}
.dkb-link-chip:hover {
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
  color: var(--qss-text);
}
.dkb-link-chip button {
  border: none;
  background: none;
  color: var(--qss-text-muted);
  font-size: 11px;
  line-height: 1;
  cursor: pointer;
  padding: 0;
}
.dkb-link-chip button:hover {
  color: var(--qss-error);
}
.dkb-link-search {
  width: 110px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  background: none;
  color: var(--qss-text);
  font-size: 10.5px;
}
.dkb-link-search:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}
.dkb-link-result {
  padding: 2px 8px;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 999px;
  background: none;
  color: var(--qss-text-secondary);
  font: 400 10px/1.5 var(--qss-font-sans);
  cursor: pointer;
}
.dkb-link-result:hover {
  color: var(--qss-text);
  background: var(--qss-bg-hover);
}
.dkb-link-result em {
  font-style: normal;
  color: var(--qss-text-muted);
  font-size: 9px;
}

.dkb-hand {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  padding-top: 6px;
  border-top: 1px solid var(--qss-border-subtle);
  font-size: 10.5px;
  color: var(--qss-text-muted);
}
.dkb-hand-p {
  padding: 3px 9px;
  border: 1px solid color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
  border-radius: 999px;
  background: none;
  color: var(--qss-text);
  font: 500 10.5px/1.4 var(--qss-font-sans);
  cursor: pointer;
}
.dkb-hand-p:hover {
  background: color-mix(in srgb, var(--qss-accent) 12%, transparent);
}

.dkb-new {
  margin: 6px 8px 8px;
  padding: 6px 9px;
  border: 1px dashed var(--qss-border);
  border-radius: 7px;
  background: none;
  color: var(--qss-text-muted);
  font: 500 12px/1.4 var(--qss-font-sans);
  text-align: left;
  cursor: pointer;
}
.dkb-new:hover {
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
  color: var(--qss-text);
}

/* The card editor is QKanbanCardModal — styled there, once for every board. */

@media (prefers-reduced-motion: reduce) {
  .dkb-col {
    transition: none;
  }
}
</style>
