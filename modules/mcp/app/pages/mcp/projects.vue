<script setup lang="ts">
import { useMcpWorkspaces } from '#mcp/composables/useMcpWorkspaces'
definePageMeta({ layout: 'mcp' })
const router = useRouter()

import {
  GENERAL_WORKSPACE_ID,
  KANBAN_COLUMNS,
  type ApprovalMode,
  type KanbanCard,
  type KanbanCardDraft,
  type KanbanColumn,
} from "#mcp/composables/useMcpWorkspaces";

const {
  selectedWorkspace,
  isGeneralSelected,
  cards,
  cardsError,
  isActive,
  refresh,
  subscribe,
  addFolder,
  forgetWorkspace,
  pinWorkspace,
  addCard,
  updateCard,
  moveCard,
  deleteCard,
  archivedCards,
  archiveDoneCards,
  archiveCard,
  unarchiveCard,
  getApprovalMode,
  setApprovalMode,
  getIndexSettings,
  setIndexSettings,
  getIndexStats,
  indexWorkspaceCodebase,
} = useMcpWorkspaces();

// ── Codebase Indexing state (per-workspace) ──
const indexStatusMap = ref<Record<string, {
  status: string;
  file_count: number;
  indexed_at: number | null;
  mode?: string | null;
  fts_entry_count?: number | null;
  chunk_count?: number | null;
  structural_indexed_at?: number | null;
  semantic_indexed_at?: number | null;
}>>({});
const structuralIndexingIds = ref<Set<string>>(new Set());
const semanticIndexingIds = ref<Set<string>>(new Set());
const embedProvider = ref<"ollama" | "lmstudio">("ollama");
const embedModel = ref("nomic-embed-text");
const embedBaseUrl = ref("http://localhost:11434");
const structuralResultMap = ref<Record<string, string>>({});
const semanticResultMap = ref<Record<string, string>>({});
const indexFilter = ref<"everything" | "smart">("smart");

// Computed proxies for the currently selected workspace
const indexStatus = computed(() => {
  const id = selectedWorkspace.value?.id;
  return id ? indexStatusMap.value[id] ?? null : null;
});
const structuralIndexing = computed(() => {
  const id = selectedWorkspace.value?.id;
  return id ? structuralIndexingIds.value.has(id) : false;
});
const semanticIndexing = computed(() => {
  const id = selectedWorkspace.value?.id;
  return id ? semanticIndexingIds.value.has(id) : false;
});
const structuralResult = computed(() => {
  const id = selectedWorkspace.value?.id;
  return id ? structuralResultMap.value[id] ?? null : null;
});
const semanticResult = computed(() => {
  const id = selectedWorkspace.value?.id;
  return id ? semanticResultMap.value[id] ?? null : null;
});

// ── AgentOS (merged from the old /mcp/agent page) ──
// Per-entry toggle between the board view and the AGENT.md editor — the
// "General" entry carries the suite-wide board plus the global AGENT.md.
const showAgentOS = ref(false);

// The board the page is showing: a workspace's or the General board.
const selectedBoardId = computed(() =>
  isGeneralSelected.value ? GENERAL_WORKSPACE_ID : (selectedWorkspace.value?.id ?? null),
);

// ── Kanban approval mode state ──
const approvalMode = ref<ApprovalMode>("auto_apply");

// Keyed by board — General ('general') stores its mode like any workspace.
// Nothing merges there, so on General the mode only governs whether the
// review column is part of the flow.
async function loadApprovalMode() {
  const id = selectedBoardId.value;
  if (!id) return;
  try {
    const mode = await getApprovalMode(id);
    if (id === selectedBoardId.value) approvalMode.value = mode;
  } catch {
    if (id === selectedBoardId.value) approvalMode.value = "auto_apply";
  }
}

function openDrawerBoard() {
  window.dispatchEvent(new CustomEvent('qss:drawer', { detail: { tab: 'kanban' } }));
}

function openWorkspaceMemory() {
  if (selectedBoardId.value) void router.push({ path: '/memory', query: { scope: selectedBoardId.value } });
}

async function onApprovalModeChange(e: Event) {
  const mode = (e.target as HTMLSelectElement).value as ApprovalMode;
  if (!selectedBoardId.value) return;
  try {
    await setApprovalMode(selectedBoardId.value, mode);
    approvalMode.value = mode;
  } catch (err) {
    console.error("Failed to set approval mode:", err);
  }
}

// One grouping pass per card change instead of a filter+sort per lookup: the
// board asks for every column three times per render, and a drag re-renders on
// each pointermove.
const cardsByColumn = computed(() => {
  const grouped = { plan: [], work: [], review: [], done: [] } as Record<
    KanbanColumn,
    KanbanCard[]
  >;
  for (const card of cards.value) grouped[card.column]?.push(card);
  for (const column of Object.values(grouped))
    column.sort((a, b) => a.order - b.order);
  return grouped;
});

// 'review' is where the backend parks every completed card before it merges,
// in both modes — and where it stays when the merge conflicts. Hiding it in
// auto_apply would strand those cards off the board, so it only folds away
// while it is empty. Same rule on General: its dropdown flips the review
// step in and out of the flow (nothing merges there — no folder).
const visibleColumns = computed(() =>
  approvalMode.value === "auto_apply" && cardsByColumn.value.review.length === 0
    ? KANBAN_COLUMNS.filter((col) => col.id !== "review")
    : KANBAN_COLUMNS,
);

const hasStructural = computed(
  () => !!indexStatus.value?.structural_indexed_at || indexStatus.value?.mode === "structural" || indexStatus.value?.mode === "both",
);
const hasSemantic = computed(
  () => !!indexStatus.value?.semantic_indexed_at || indexStatus.value?.mode === "semantic" || indexStatus.value?.mode === "both",
);

// Track whether we're restoring saved settings (skip base URL auto-sync)
let restoringSettings = false;

// Sync base URL when provider changes (only on user interaction, not restore)
watch(embedProvider, (provider) => {
  if (restoringSettings) return;
  embedBaseUrl.value =
    provider === "ollama" ? "http://localhost:11434" : "http://localhost:1234";
});

// Saved index settings live in core.db (scope "mcp"), not on the registry
// entry — the registry only knows folders.
async function restoreIndexSettings() {
  const ws = selectedWorkspace.value;
  if (!ws) return;
  restoringSettings = true;
  try {
    const saved = await getIndexSettings(ws.id);
    embedProvider.value = (saved.provider as "ollama" | "lmstudio") || "ollama";
    embedModel.value = saved.model || "nomic-embed-text";
    embedBaseUrl.value =
      saved.baseUrl ||
      (embedProvider.value === "ollama"
        ? "http://localhost:11434"
        : "http://localhost:1234");
    indexFilter.value = (saved.filter as "everything" | "smart") || "smart";
  } catch {
    // defaults stand
  }
  nextTick(() => {
    restoringSettings = false;
  });
}

async function loadIndexStatus() {
  if (!selectedWorkspace.value) return;
  const wid = selectedWorkspace.value.id;
  try {
    const stats = await getIndexStats(wid);
    indexStatusMap.value = { ...indexStatusMap.value, [wid]: stats };
  } catch {
    const { [wid]: _, ...rest } = indexStatusMap.value;
    indexStatusMap.value = rest;
  }
}

async function startIndexing(mode: "structural" | "semantic") {
  if (!selectedWorkspace.value) return;
  const wid = selectedWorkspace.value.id;
  const isStructural = mode === "structural";
  if (isStructural && structuralIndexingIds.value.has(wid)) return;
  if (!isStructural && semanticIndexingIds.value.has(wid)) return;

  if (isStructural) {
    structuralIndexingIds.value = new Set([...structuralIndexingIds.value, wid]);
    const { [wid]: _, ...rest } = structuralResultMap.value;
    structuralResultMap.value = rest;
  } else {
    semanticIndexingIds.value = new Set([...semanticIndexingIds.value, wid]);
    const { [wid]: _, ...rest } = semanticResultMap.value;
    semanticResultMap.value = rest;
  }

  try {
    const forceReindex = isStructural ? hasStructural.value : hasSemantic.value;
    const result = await indexWorkspaceCodebase(
      wid,
      mode,
      !isStructural ? embedProvider.value : undefined,
      !isStructural ? embedModel.value : undefined,
      !isStructural ? embedBaseUrl.value : undefined,
      forceReindex,
      indexFilter.value,
    );
    const msg = result.error
      ? `Error: ${result.error}`
      : `Indexed ${result.files_indexed ?? 0} files (${result.files_skipped ?? 0} unchanged, ${result.errors ?? 0} errors). Entries: ${result.total_entries ?? 0}`;
    if (isStructural) structuralResultMap.value = { ...structuralResultMap.value, [wid]: msg };
    else semanticResultMap.value = { ...semanticResultMap.value, [wid]: msg };

    // Reload status for the workspace that was indexed
    const stats = await getIndexStats(wid);
    indexStatusMap.value = { ...indexStatusMap.value, [wid]: stats };
  } catch (e: any) {
    const msg = `Error: ${e?.message || e}`;
    if (isStructural) structuralResultMap.value = { ...structuralResultMap.value, [wid]: msg };
    else semanticResultMap.value = { ...semanticResultMap.value, [wid]: msg };
  } finally {
    if (isStructural) {
      const s = new Set(structuralIndexingIds.value);
      s.delete(wid);
      structuralIndexingIds.value = s;
    } else {
      const s = new Set(semanticIndexingIds.value);
      s.delete(wid);
      semanticIndexingIds.value = s;
    }
  }
}

function formatTimestamp(ts: number | null | undefined): string {
  if (!ts) return "never";
  return new Date(ts * 1000).toLocaleString();
}

// Watch board selection to load index status and restore saved settings.
// General has a stored approval mode too, but no folder — index settings
// stay workspace-only.
watch(
  () => selectedBoardId.value,
  (newId) => {
    showAgentOS.value = false;
    if (!newId) {
      approvalMode.value = "auto_apply";
      return;
    }
    if (selectedWorkspace.value) {
      restoreIndexSettings();
      loadIndexStatus();
    }
    loadApprovalMode();
  },
);

// Save indexFilter per-workspace when the user changes it
watch(indexFilter, (val) => {
  if (restoringSettings) return;
  const ws = selectedWorkspace.value;
  if (!ws) return;
  getIndexSettings(ws.id).then((saved) =>
    setIndexSettings(ws.id, { ...saved, filter: val }),
  );
});

// ── Workspace actions ──

async function togglePin() {
  const ws = selectedWorkspace.value;
  if (!ws) return;
  await pinWorkspace(ws.id, !ws.pinned);
}

async function confirmForgetWorkspace() {
  const ws = selectedWorkspace.value;
  if (!ws) return;
  if (
    !confirm(
      `Forget workspace "${ws.name}"? The folder and its kanban cards are kept; the workspace disappears from the suite until the folder is opened again.`,
    )
  )
    return;
  await forgetWorkspace(ws.id);
}

// ── Card editor (QKanbanCardModal — the suite-wide one, packages/ui) ──
const showCardModal = ref(false);
const editingCard = ref<KanbanCard | null>(null);
const newCardColumn = ref<KanbanColumn>("plan");
const cardModalBusy = ref(false);
const cardModalError = ref<string | null>(null);

const boardLabel = computed(() =>
  isGeneralSelected.value ? "General" : (selectedWorkspace.value?.name ?? ""),
);

function openAddCard(column: KanbanColumn = "plan") {
  editingCard.value = null;
  newCardColumn.value = column;
  cardModalError.value = null;
  showCardModal.value = true;
}

function openEditCard(card: KanbanCard) {
  editingCard.value = card;
  cardModalError.value = null;
  showCardModal.value = true;
}

// The editor hands back a draft; the host owns the commands. A failure stays
// in the editor's footer with the draft intact instead of vanishing.
async function saveCard(draft: KanbanCardDraft, card: KanbanCard | null) {
  if (!selectedBoardId.value) return;
  cardModalBusy.value = true;
  cardModalError.value = null;
  try {
    if (card) {
      await updateCard(card.id, draft.title, draft.description, draft.priority);
      // updateCard carries no column — a changed one has to go through moveCard
      if (draft.column !== card.column) await moveCard(card.id, draft.column);
    } else {
      await addCard(
        selectedBoardId.value,
        draft.title,
        draft.description,
        draft.column,
        draft.priority,
      );
    }
    showCardModal.value = false;
  } catch (err) {
    cardModalError.value = err instanceof Error ? err.message : String(err);
  } finally {
    cardModalBusy.value = false;
  }
}

// Delete from inside the editor: the editor goes, the confirm takes over.
function deleteFromModal(card: KanbanCard) {
  showCardModal.value = false;
  confirmDeleteCard(card);
}

// ── Delete card confirm modal ──
const showDeleteConfirm = ref(false);
const deletingCard = ref<KanbanCard | null>(null);

function confirmDeleteCard(card: KanbanCard) {
  deletingCard.value = card;
  showDeleteConfirm.value = true;
}

async function executeDeleteCard() {
  if (!deletingCard.value) return;
  await deleteCard(deletingCard.value.id);
  deletingCard.value = null;
}

// ── Archive ──
// "Archive all" on the Done column clears it without deleting anything: the
// cards stay in kanban.db, agents list them (list_kanban_cards
// archived=true) so earlier work is known, and the archive modal shows them
// full-size. Reversible per card (Restore), so no confirm step.
const showArchiveModal = ref(false);
const archiveBusy = ref(false);
const archivingCardId = ref<string | null>(null);
const restoringCardId = ref<string | null>(null);
const archiveError = ref<string | null>(null);

async function archiveDone() {
  if (!selectedBoardId.value || archiveBusy.value || archivingCardId.value) return;
  archiveBusy.value = true;
  archiveError.value = null;
  try {
    await archiveDoneCards(selectedBoardId.value);
  } catch (err) {
    archiveError.value = err instanceof Error ? err.message : String(err);
    console.error("Failed to archive the Done column:", err);
  } finally {
    archiveBusy.value = false;
  }
}

async function archiveIndividual(card: KanbanCard) {
  if (card.column !== "done" || card.archived_at != null || archiveBusy.value || archivingCardId.value || cardModalBusy.value) return;
  archivingCardId.value = card.id;
  archiveError.value = null;
  cardModalBusy.value = true;
  cardModalError.value = null;
  try {
    await archiveCard(card.id);
    showCardModal.value = false;
  } catch (err) {
    cardModalError.value = err instanceof Error ? err.message : String(err);
  } finally {
    archivingCardId.value = null;
    cardModalBusy.value = false;
  }
}

async function restoreArchived(card: KanbanCard) {
  if (restoringCardId.value) return;
  restoringCardId.value = card.id;
  archiveError.value = null;
  try {
    await unarchiveCard(card.id);
  } catch (err) {
    archiveError.value = err instanceof Error ? err.message : String(err);
  } finally {
    restoringCardId.value = null;
  }
}

// ── Drag & Drop ──
const draggingCardId = ref<string | null>(null);
const dropTarget = ref<{
  column: KanbanColumn;
  beforeId: string | null;
} | null>(null);

let pointerMoveHandler: ((e: PointerEvent) => void) | null = null;
let pointerUpHandler: ((e: PointerEvent) => void) | null = null;
let pointerStartX = 0;
let pointerStartY = 0;
let pointerDidMove = false;

function isColumnDragOver(column: KanbanColumn): boolean {
  return dropTarget.value?.column === column;
}

function isCardDropBefore(column: KanbanColumn, cardId: string): boolean {
  return (
    dropTarget.value?.column === column && dropTarget.value?.beforeId === cardId
  );
}

function isCardDropAfter(column: KanbanColumn, cardId: string): boolean {
  if (dropTarget.value?.column !== column) return false;
  const dragId = draggingCardId.value;
  const colCards = cardsByColumn.value[column].filter((c) => c.id !== dragId);
  const idx = colCards.findIndex((c) => c.id === cardId);
  if (idx === -1) return false;
  const next = colCards[idx + 1];
  return dropTarget.value.beforeId === (next ? next.id : null);
}

function removePointerListeners() {
  if (pointerMoveHandler) {
    window.removeEventListener("pointermove", pointerMoveHandler, true);
    pointerMoveHandler = null;
  }
  if (pointerUpHandler) {
    window.removeEventListener("pointerup", pointerUpHandler, true);
    window.removeEventListener("pointercancel", pointerUpHandler, true);
    pointerUpHandler = null;
  }
}

function resetDragState() {
  removePointerListeners();
  document.body.classList.remove("kanban-dragging");
  draggingCardId.value = null;
  dropTarget.value = null;
}

function findDropTargetAtPoint(
  x: number,
  y: number,
  draggingId: string,
): { column: KanbanColumn; beforeId: string | null } | null {
  const element = document.elementFromPoint(x, y) as HTMLElement | null;
  if (!element) {
    dropTarget.value = null;
    return null;
  }

  const columnElement = element.closest(
    "[data-kanban-column]",
  ) as HTMLElement | null;
  const columnId = columnElement?.dataset.kanbanColumn as
    | KanbanColumn
    | undefined;
  if (!columnId) {
    dropTarget.value = null;
    return null;
  }

  const cardElement = element.closest(
    "[data-kanban-card-id]",
  ) as HTMLElement | null;
  const hoveredCardId = cardElement?.dataset.kanbanCardId ?? null;

  if (!hoveredCardId || hoveredCardId === draggingId) {
    const target = { column: columnId, beforeId: null };
    dropTarget.value = target;
    return target;
  }

  const columnCardIds = cardsByColumn.value[columnId]
    .map((c) => c.id)
    .filter((id) => id !== draggingId);
  const hoveredIdx = columnCardIds.findIndex((id) => id === hoveredCardId);
  if (hoveredIdx === -1) {
    const target = { column: columnId, beforeId: null };
    dropTarget.value = target;
    return target;
  }

  // non-null: the early return at `!hoveredCardId` above only passes when
  // cardElement exists, but that runs through an optional chain TS cannot follow
  const rect = cardElement!.getBoundingClientRect();
  const before = y <= rect.top + rect.height / 2;
  const beforeId = before
    ? hoveredCardId
    : (columnCardIds[hoveredIdx + 1] ?? null);
  const target = { column: columnId, beforeId };
  dropTarget.value = target;
  return target;
}

function onCardPointerDown(e: PointerEvent, card: KanbanCard) {
  if (e.button !== 0) return;
  const target = e.target as HTMLElement | null;
  if (
    target?.closest(
      'button, a, input, textarea, select, [contenteditable="true"], [data-no-drag]',
    )
  ) {
    return;
  }
  e.preventDefault();
  e.stopPropagation();

  pointerStartX = e.clientX;
  pointerStartY = e.clientY;
  pointerDidMove = false;
  draggingCardId.value = card.id;
  dropTarget.value = null;
  document.body.classList.add("kanban-dragging");

  pointerMoveHandler = (moveEvent: PointerEvent) => {
    if (!draggingCardId.value) return;
    if (!pointerDidMove) {
      const dx = Math.abs(moveEvent.clientX - pointerStartX);
      const dy = Math.abs(moveEvent.clientY - pointerStartY);
      if (dx + dy < 4) return;
      pointerDidMove = true;
    }
    findDropTargetAtPoint(
      moveEvent.clientX,
      moveEvent.clientY,
      draggingCardId.value,
    );
  };

  pointerUpHandler = async (upEvent: PointerEvent) => {
    const draggedId = draggingCardId.value;
    if (!draggedId) return;
    if (!pointerDidMove) {
      resetDragState();
      return;
    }
    const target = findDropTargetAtPoint(
      upEvent.clientX,
      upEvent.clientY,
      draggedId,
    );

    try {
      if (target) {
        await moveCard(draggedId, target.column, target.beforeId);
      }
    } finally {
      resetDragState();
    }
  };

  window.addEventListener("pointermove", pointerMoveHandler, true);
  window.addEventListener("pointerup", pointerUpHandler, true);
  window.addEventListener("pointercancel", pointerUpHandler, true);
}

let unsubscribeWorkspaces: (() => void) | null = null;

onMounted(async () => {
  await refresh();
  unsubscribeWorkspaces = subscribe();
  // Restore saved indexing settings after fresh data is loaded
  if (selectedWorkspace.value) {
    restoreIndexSettings();
    loadIndexStatus();
  }
  if (selectedBoardId.value) loadApprovalMode();
});

onBeforeUnmount(() => {
  resetDragState();
  unsubscribeWorkspaces?.();
  unsubscribeWorkspaces = null;
});
</script>


<template>
  <div class="projects-layout">
    <!-- Project Content -->
    <div class="project-content">
      <p v-if="cardsError" role="alert">Board could not load: {{ cardsError }} <button class="btn-secondary btn-sm" @click="refresh">Retry</button></p>
      <!-- General: the suite-wide board plus the global AGENT.md -->
      <template v-if="isGeneralSelected">
        <div class="project-header">
          <div class="project-header-info">
            <h1 class="project-title">General</h1>
            <span class="project-folder">Suite-wide board — no folder; move a card to a workspace before an agent can claim it</span>
          </div>
          <div class="project-header-actions">
            <button class="btn-secondary btn-sm" @click="openDrawerBoard">Open in drawer</button>
            <button class="btn-secondary btn-sm" @click="openWorkspaceMemory">Memory</button>
            <button
              class="btn-secondary btn-sm"
              :class="{ 'btn-toggled': showAgentOS }"
              title="Edit the global AGENT.md — applies to all workspaces"
              @click="showAgentOS = !showAgentOS"
            >
              AgentOS
            </button>
          </div>
        </div>
        <section v-if="showAgentOS" class="agent-section">
          <McpAgentEditor :workspace-path="null" />
        </section>
      </template>

      <!-- No workspace selected -->
      <div v-else-if="!selectedWorkspace" class="no-project">
        <div class="no-project-inner">
          <p class="no-project-text">
            No workspace selected. Workspaces are the suite's registered
            folders — open one anywhere in QuantSuite, or add one here.
          </p>
          <button class="btn-primary" @click="addFolder">
            + Add Workspace
          </button>
        </div>
      </div>

      <!-- Workspace content -->
      <template v-else>
        <!-- Workspace header -->
        <div class="project-header">
          <div class="project-header-info">
            <h1 class="project-title">
              {{ selectedWorkspace.name }}
              <span v-if="isActive(selectedWorkspace)" class="badge-active">Active</span>
            </h1>
            <span class="project-folder" :title="selectedWorkspace.path">
              <svg
                class="folder-icon"
                width="12"
                height="12"
                viewBox="0 0 16 14"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <path
                  d="M1 2.5A1.5 1.5 0 012.5 1h3.172a1.5 1.5 0 011.06.44L7.94 2.65A1.5 1.5 0 009 3.09H13.5A1.5 1.5 0 0115 4.59V11.5A1.5 1.5 0 0113.5 13h-11A1.5 1.5 0 011 11.5V2.5z"
                  stroke="currentColor"
                  stroke-width="1.3"
                  fill="none"
                />
              </svg>
              {{ selectedWorkspace.path }}
            </span>
          </div>
          <div class="project-header-actions">
            <button class="btn-secondary btn-sm" @click="openDrawerBoard">Open in drawer</button>
            <button class="btn-secondary btn-sm" @click="openWorkspaceMemory">Memory</button>
            <button
              class="btn-secondary btn-sm"
              :class="{ 'btn-toggled': showAgentOS }"
              title="Edit this workspace's AGENT.md"
              @click="showAgentOS = !showAgentOS"
            >
              AgentOS
            </button>
            <button
              class="btn-secondary btn-sm"
              :title="selectedWorkspace.pinned ? 'Unpin workspace' : 'Pin workspace to the top'"
              @click="togglePin"
            >
              {{ selectedWorkspace.pinned ? "Unpin" : "Pin" }}
            </button>
            <button
              class="btn-danger btn-sm"
              title="Remove from the registry — folder and cards stay on disk"
              @click="confirmForgetWorkspace"
            >
              Forget
            </button>
          </div>
        </div>

        <!-- AgentOS: this workspace's AGENT.md -->
        <section v-if="showAgentOS" class="agent-section">
          <McpAgentEditor v-if="selectedWorkspace.path" :workspace-path="selectedWorkspace.path" />
          <p v-else class="indexing-desc">
            This workspace has no folder path — re-open it to fix the registry entry.
          </p>
        </section>

        <!-- Codebase Indexing -->
        <section v-if="!showAgentOS" class="indexing-section">
          <div class="indexing-header">
            <h2 class="section-title">Codebase Indexing</h2>
            <span v-if="hasStructural && hasSemantic" class="badge-indexed">Both</span>
            <span v-else-if="hasStructural || hasSemantic" class="badge-indexed">Indexed</span>
            <span v-else-if="structuralIndexing || semanticIndexing" class="badge-indexing">Indexing...</span>
            <select
              v-model="indexFilter"
              class="form-input form-input-xs index-filter-select"
              title="File filter: Smart skips config, docs & style files"
            >
              <option value="everything">Everything</option>
              <option value="smart">Smart Select</option>
            </select>
          </div>

          <p v-if="!selectedWorkspace?.path" class="indexing-desc">
            This workspace has no folder path — re-open it to fix the registry entry.
          </p>

          <div v-else class="index-cards">
            <!-- Structural Card -->
            <div class="index-card">
              <div class="index-card-header">
                <span
                  class="status-dot"
                  :class="{
                    'dot-idle': !hasStructural && !structuralIndexing,
                    'dot-indexing': structuralIndexing,
                    'dot-indexed': hasStructural && !structuralIndexing,
                  }"
                />
                <span class="index-card-title">Structural</span>
                <span class="index-card-subtitle">(BM25)</span>
                <template v-if="hasStructural && !structuralIndexing">
                  <span class="stat-sep">&mdash;</span>
                  <span class="stat-inline">{{ indexStatus?.file_count ?? 0 }} files, {{ indexStatus?.fts_entry_count ?? 0 }} symbols</span>
                </template>
                <button
                  class="btn-primary btn-xs index-card-btn"
                  :disabled="structuralIndexing || !selectedWorkspace?.path"
                  @click="startIndexing('structural')"
                >
                  {{ structuralIndexing ? "Indexing..." : hasStructural ? "Re-index" : "Index" }}
                </button>
              </div>
              <div v-if="structuralResult" class="index-card-result">
                <p
                  class="index-result index-result-sm"
                  :class="{ 'index-error': structuralResult.startsWith('Error') }"
                >
                  {{ structuralResult }}
                </p>
              </div>
            </div>

            <!-- Semantic Card -->
            <div class="index-card">
              <div class="index-card-header">
                <span
                  class="status-dot"
                  :class="{
                    'dot-idle': !hasSemantic && !semanticIndexing,
                    'dot-indexing': semanticIndexing,
                    'dot-indexed': hasSemantic && !semanticIndexing,
                  }"
                />
                <span class="index-card-title">Semantic</span>
                <span class="index-card-subtitle">(Vectors)</span>
                <template v-if="hasSemantic && !semanticIndexing">
                  <span class="stat-sep">&mdash;</span>
                  <span class="stat-inline">{{ indexStatus?.file_count ?? 0 }} files, {{ indexStatus?.chunk_count ?? 0 }} chunks</span>
                </template>
                <button
                  class="btn-primary btn-xs index-card-btn"
                  :disabled="semanticIndexing || !selectedWorkspace?.path"
                  @click="startIndexing('semantic')"
                >
                  {{ semanticIndexing ? "Indexing..." : hasSemantic ? "Re-index" : "Index" }}
                </button>
              </div>
              <div class="embed-settings-compact">
                <select v-model="embedProvider" class="form-input form-input-xs">
                  <option value="ollama">Ollama</option>
                  <option value="lmstudio">LM Studio</option>
                </select>
                <input
                  v-model="embedModel"
                  class="form-input form-input-xs embed-model-input"
                  placeholder="model name"
                />
                <input
                  v-model="embedBaseUrl"
                  class="form-input form-input-xs embed-url-input"
                  placeholder="http://localhost:11434"
                />
              </div>
              <div v-if="semanticResult" class="index-card-result">
                <p
                  class="index-result index-result-sm"
                  :class="{ 'index-error': semanticResult.startsWith('Error') }"
                >
                  {{ semanticResult }}
                </p>
              </div>
            </div>
          </div>
        </section>

      </template>

      <!-- Kanban Board — every workspace board plus General (PLAN-KANBAN-UNIFY) -->
      <section
        v-if="(isGeneralSelected || selectedWorkspace) && !showAgentOS"
        class="kanban-section"
      >
          <div class="kanban-header">
            <h2 class="section-title">Kanban Board</h2>
            <button
              class="archive-btn"
              :class="{ 'has-cards': archivedCards.length > 0 }"
              title="Open the archive — every card cleared off the Done column, full-size and readable"
              @click="showArchiveModal = true"
            >
              <svg class="archive-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
                <rect x="1.5" y="2.5" width="13" height="3" rx="1" stroke="currentColor" stroke-width="1.3" />
                <path d="M2.5 5.5v7a1 1 0 001 1h9a1 1 0 001-1v-7" stroke="currentColor" stroke-width="1.3" />
                <path d="M6.5 8.5h3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
              </svg>
              Archive
              <span class="archive-count">{{ archivedCards.length }}</span>
            </button>
            <select
              :value="approvalMode"
              class="form-input form-input-xs approval-mode-select"
              title="Auto Apply: start immediately and merge completed work. Approval: approve the plan before work, then approve the result before merging."
              @change="onApprovalModeChange"
            >
              <option value="auto_apply">Auto Apply</option>
              <option value="approval">Approval</option>
            </select>
          </div>
          <p v-if="archiveError && !showArchiveModal" class="archive-error" role="alert">{{ archiveError }}</p>
          <div class="kanban-board">
            <div
              v-for="col in visibleColumns"
              :key="col.id"
              class="kanban-column"
              :data-kanban-column="col.id"
              :class="[
                `col-${col.id}`,
                { 'drag-over': isColumnDragOver(col.id) },
              ]"
            >
              <div class="col-header">
                <span class="col-name">{{ col.label }}</span>
                <div class="col-header-side">
                  <!-- Done only: clear the column into the archive. Kept
                       for the agents, restorable from the archive modal. -->
                  <button
                    v-if="col.id === 'done' && cardsByColumn.done.length > 0"
                    class="col-action"
                    :disabled="archiveBusy || archivingCardId !== null"
                    title="Archive every Done card — off the board, kept for the agents, restorable from the archive"
                    @click="archiveDone"
                  >
                    {{ archiveBusy ? "Archiving…" : "Archive all" }}
                  </button>
                  <span class="col-count">{{
                    cardsByColumn[col.id].length
                  }}</span>
                </div>
              </div>

              <div class="col-cards">
                <div
                  v-for="card in cardsByColumn[col.id]"
                  :key="card.id"
                  class="kanban-card"
                  :data-kanban-card-id="card.id"
                  :data-kanban-column="col.id"
                  @pointerdown="onCardPointerDown($event, card)"
                  @dblclick="openEditCard(card)"
                  :class="{
                    dragging: draggingCardId === card.id,
                    'drop-before': isCardDropBefore(col.id, card.id),
                    'drop-after': isCardDropAfter(col.id, card.id),
                  }"
                >
                  <div class="card-header-row">
                    <span class="drag-handle" title="Drag to move">⠿</span>
                    <div class="card-title">{{ card.title }}</div>
                    <div class="card-actions card-actions-top">
                      <button
                        class="card-btn"
                        title="Edit card"
                        @click="openEditCard(card)"
                      >
                        ✎
                      </button>
                      <button
                        class="card-btn card-btn-danger"
                        title="Delete card"
                        @click="confirmDeleteCard(card)"
                      >
                        ×
                      </button>
                    </div>
                  </div>
                  <div class="card-meta-row">
                    <span
                      class="card-priority"
                      :class="'priority-' + (card.priority || 'medium')"
                      >{{ card.priority || 'medium' }}</span
                    >
                    <span
                      v-if="card.status && card.status !== 'backlog'"
                      class="card-status"
                      >{{ card.status?.replace('_', ' ') }}</span
                    >
                    <span v-if="card.agent_id" class="card-agent" :title="'Agent: ' + card.agent_id"
                      >{{ card.agent_id }}</span
                    >
                  </div>
                  <div v-if="card.description" class="card-desc">
                    {{ card.description }}
                  </div>
                </div>
                <div
                  v-if="cardsByColumn[col.id].length === 0"
                  class="empty-column-hint"
                >
                  Drag a card here
                </div>
              </div>

              <button class="add-card-btn" @click="openAddCard(col.id)">
                + Add card
              </button>
            </div>
          </div>
      </section>
    </div>
  </div>

  <!-- Card editor — QKanbanCardModal (packages/ui), the same one the drawer's
       Kanban tab opens. It teleports itself and runs on --qss-* tokens, so no
       data-module scope is needed here. -->
  <QKanbanCardModal
    v-model="showCardModal"
    :card="editingCard"
    :column="newCardColumn"
    :columns="visibleColumns"
    :board-label="boardLabel"
    :approval-mode="approvalMode"
    :readonly="approvalMode === 'approval' && !!editingCard?.agent_id"
    :busy="cardModalBusy"
    :error="cardModalError"
    @save="saveCard"
    @delete="deleteFromModal"
    @archive="archiveIndividual"
  />

  <!-- The archive: every card "Archive all" took off the Done column,
       full-size so each one stays readable. -->
  <McpKanbanArchiveModal
    v-model="showArchiveModal"
    :cards="archivedCards"
    :board-label="boardLabel"
    :restoring-id="restoringCardId"
    :error="archiveError"
    @restore="restoreArchived"
  />

  <!-- Delete Card Confirm Modal -->
  <McpConfirmModal
    v-model="showDeleteConfirm"
    title="Delete Card"
    :message="`Are you sure you want to delete &quot;${deletingCard?.title}&quot;?`"
    submessage="This action cannot be undone."
    confirm-label="Delete"
    cancel-label="Cancel"
    danger
    @confirm="executeDeleteCard"
  />
</template>

<style scoped>
/* â”€â”€ Layout â”€â”€ */
.projects-layout {
  display: flex;
  height: calc(100% + 64px);
  gap: 0;
  margin: -32px;
  overflow: hidden;
}

/* â”€â”€ Project Sidebar â”€â”€ */
.project-sidebar {
  width: 220px;
  min-width: 220px;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.project-sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 14px 12px;
  border-bottom: 1px solid var(--border);
}

.sidebar-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.btn-icon {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 16px;
  transition: all var(--transition);
  padding: 0;
}

.btn-icon:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--accent);
}

.sidebar-empty {
  padding: 24px 14px;
  color: var(--text-muted);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  align-items: flex-start;
}

.project-list {
  list-style: none;
  margin: 0;
  padding: 8px;
  overflow-y: auto;
  flex: 1;
}

.project-item {
  padding: 9px 10px;
  border-radius: var(--radius);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
  transition: background var(--transition);
  margin-bottom: 2px;
}

.project-item:hover {
  background: var(--bg-hover);
}

.project-item.active {
  background: var(--bg-hover);
  border-left: 2px solid var(--accent);
  padding-left: 8px;
}

.project-item-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.project-item.active .project-item-name {
  color: var(--accent);
}

.project-item-path {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* â”€â”€ Project Content â”€â”€ */
.project-content {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.no-project {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

.no-project-inner {
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 16px;
  align-items: center;
}

.no-project-text {
  color: var(--text-muted);
  font-size: 14px;
}

/* â”€â”€ Project Header â”€â”€ */
.project-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 24px 28px 16px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.project-header-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.project-title {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.badge-active {
  font-size: 10px;
  font-weight: 600;
  color: var(--accent);
  border: 1px solid var(--accent);
  border-radius: 999px;
  padding: 2px 8px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.project-folder {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 400px;
}

.project-header-actions {
  display: flex;
  flex-wrap: wrap;
  max-width: 100%;
  gap: 8px;
  flex-shrink: 0;
}

/* â”€â”€ Section title â”€â”€ */
.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 0 0 12px;
}

/* ── AgentOS ── */
.agent-section {
  padding: 20px 28px;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.btn-toggled {
  border-color: var(--accent);
  color: var(--accent);
}

/* â”€â”€ Kanban Board â”€â”€ */
.kanban-section {
  padding: 20px 28px;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.kanban-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.kanban-header .section-title {
  white-space: nowrap;
  flex-shrink: 0;
  margin-bottom: 0;
}

/* The archive button sits with the board control on the right: it pushes
 * itself over, the select follows. */
.archive-btn {
  margin-left: auto;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px 4px 9px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  line-height: 1.4;
  cursor: pointer;
  transition: all var(--transition);
}

.archive-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--accent);
}

.archive-icon {
  flex-shrink: 0;
  opacity: 0.8;
}

.archive-count {
  background: var(--bg-hover);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 600;
  padding: 0 6px;
  border-radius: 10px;
  min-width: 20px;
  text-align: center;
  line-height: 16px;
}

.archive-btn.has-cards .archive-count {
  color: var(--text-secondary);
}

.approval-mode-select {
  width: auto;
  max-width: 50%;
  min-width: 0;
  padding: 4px 8px;
  font-size: 12px;
  flex-shrink: 1;
}

.kanban-board {
  display: flex;
  gap: 12px;
  padding-bottom: 8px;
  flex: 1;
  min-height: 0;
}

.kanban-column {
  flex: 1;
  min-width: 120px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}

/* Column header accent bar */
.col-plan {
  border-top: 3px solid #666;
}
.col-work {
  border-top: 3px solid #666;
}
.col-review {
  border-top: 3px solid #666;
}
.col-done {
  border-top: 3px solid #666;
}

.col-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 8px;
  border-bottom: 1px solid var(--border);
}

.col-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.col-header-side {
  display: flex;
  align-items: center;
  gap: 8px;
}

.col-count {
  background: var(--bg-hover);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 10px;
  min-width: 20px;
  text-align: center;
}

/* Quiet text action in a column header ("Archive all" on Done): reads as
 * part of the header until hovered. */
.col-action {
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--text-muted);
  font-family: inherit;
  font-size: 11px;
  font-weight: 500;
  line-height: 1.4;
  padding: 1px 7px;
  cursor: pointer;
  transition: all var(--transition);
}

.col-action:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--border);
  color: var(--text-primary);
}

.col-action:disabled {
  opacity: 0.5;
  cursor: default;
}

.col-cards {
  flex: 1;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 0;
  overflow-y: auto;
  min-height: 60px;
  max-height: none;
}

.empty-column-hint {
  font-size: 11px;
  color: var(--text-muted);
  text-align: center;
  padding: 8px 0 10px;
  pointer-events: none;
}

.kanban-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px;
  margin: 2px 0;
  min-height: 56px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  transition: border-color var(--transition);
  cursor: grab;
  overflow: hidden;
}

.kanban-card:hover {
  border-color: var(--accent);
}

.kanban-card:active {
  cursor: grabbing;
}

.kanban-card.dragging {
  opacity: 0.35;
}

.kanban-card.drop-before {
  border-top-color: var(--accent);
  box-shadow: inset 0 2px 0 0 var(--accent);
}

.kanban-card.drop-after {
  border-bottom-color: var(--accent);
  box-shadow: inset 0 -2px 0 0 var(--accent);
}

.card-header-row {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  user-select: none;
  -webkit-user-select: none;
}

.drag-handle {
  cursor: grab;
  color: var(--text-muted);
  font-size: 14px;
  line-height: 1;
  padding: 1px 2px;
  border-radius: 3px;
  user-select: none;
  -webkit-user-select: none;
  flex-shrink: 0;
  opacity: 0.4;
  transition:
    opacity 0.15s,
    color 0.15s;
  margin-top: 1px;
  -webkit-user-drag: element;
  touch-action: none;
}

.drag-handle:hover {
  opacity: 1;
  color: var(--accent);
}

.drag-handle:active {
  cursor: grabbing;
}

:global(body.kanban-dragging),
:global(body.kanban-dragging *) {
  cursor: grabbing !important;
}

.kanban-column.drag-over {
  border-color: var(--accent);
}

.kanban-column.drag-over .col-cards {
  background: rgba(79, 156, 249, 0.04);
}

.card-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.card-meta-row {
  display: flex;
  gap: 4px;
  align-items: center;
  flex-wrap: wrap;
}

.card-priority {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 5px;
  border-radius: 3px;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}
.priority-low { background: #2a3a2a; color: #6fbf6f; }
.priority-medium { background: #3a3520; color: #c8a84e; }
.priority-high { background: #3a2a20; color: #e08040; }
.priority-critical { background: #3a2020; color: #e04040; }

.card-status {
  font-size: 10px;
  color: var(--text-muted);
  padding: 1px 5px;
  background: var(--bg-hover);
  border-radius: 3px;
}

.card-agent {
  font-size: 10px;
  color: #4a9eff;
  padding: 1px 5px;
  background: #1a2a3a;
  border-radius: 3px;
  max-width: 80px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-desc {
  font-size: 12px;
  color: var(--text-muted);
  line-height: 16px;
  max-height: 32px;
  overflow: hidden;
  display: block;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.card-actions {
  display: flex;
  gap: 4px;
  align-items: center;
  margin-top: 0;
}

.card-actions-top {
  margin-left: auto;
  flex-shrink: 0;
}

.card-btn {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-size: 12px;
  padding: 2px 6px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1.4;
}

.card-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--accent);
}

.archive-error {
  margin: 0 0 8px;
  color: var(--error);
  font-size: 12px;
}

.card-btn-danger:hover {
  background: rgba(239, 68, 68, 0.12);
  color: var(--error);
  border-color: var(--error);
}

/* Same dashed pill as the drawer board's "+ Add card" (QDrawerKanban). */
.add-card-btn {
  margin: 6px 8px 8px;
  padding: 6px 9px;
  background: transparent;
  border: 1px dashed var(--border);
  border-radius: 7px;
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: color var(--transition), border-color var(--transition);
}

.add-card-btn:hover {
  border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  color: var(--text-primary);
}

.folder-icon {
  display: inline-block;
  vertical-align: middle;
  margin-right: 4px;
  color: var(--text-muted);
  flex-shrink: 0;
}

/* â”€â”€ Codebase Indexing â”€â”€ */
.indexing-section {
  padding: 16px 28px;
  border-bottom: 1px solid var(--border);
}

.indexing-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.indexing-header .section-title {
  white-space: nowrap;
  flex-shrink: 0;
  margin-bottom: 0;
}

.index-filter-select {
  margin-left: auto;
  width: auto;
  max-width: 50%;
  min-width: 0;
  padding: 4px 8px;
  font-size: 12px;
  flex-shrink: 1;
}

.badge-indexed {
  background: rgba(34, 197, 94, 0.12);
  color: #22c55e;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 10px;
  border: 1px solid rgba(34, 197, 94, 0.3);
}

.badge-indexing {
  background: rgba(251, 191, 36, 0.12);
  color: #fbbf24;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 10px;
  border: 1px solid rgba(251, 191, 36, 0.3);
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.6;
  }
}

.index-cards {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.index-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
}

.index-card-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
}

.index-card-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.index-card-subtitle {
  font-size: 11px;
  color: var(--text-muted);
  font-weight: 400;
}

.stat-sep {
  color: var(--text-muted);
  font-size: 11px;
}

.stat-inline {
  font-size: 11px;
  color: var(--text-muted);
}

.index-card-btn {
  margin-left: auto;
  flex-shrink: 0;
}

.index-card-result {
  padding: 0 12px 8px;
}

.embed-settings-compact {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
  padding: 0 12px 8px;
  align-items: center;
}

@media (max-width: 1050px) {
  .embed-settings-compact {
    grid-template-columns: 1fr;
  }
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-idle {
  background: var(--text-muted);
}

.dot-indexing {
  background: #fbbf24;
  animation: pulse 1.5s ease-in-out infinite;
}

.dot-indexed {
  background: #22c55e;
}

.mode-label {
  font-size: 12px;
  color: var(--text-secondary);
  font-weight: 500;
}

.form-input-sm {
  padding: 4px 8px;
  font-size: 12px;
  width: auto;
}

.form-input-xs {
  padding: 3px 6px;
  font-size: 11px;
  width: auto;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-family: inherit;
  transition: border-color var(--transition);
}

.form-input-xs:focus {
  outline: none;
  border-color: var(--accent);
}

select.form-input-xs {
  cursor: pointer;
}

.btn-xs {
  padding: 3px 10px;
  font-size: 11px;
}

.index-result {
  font-size: 12px;
  color: #22c55e;
  margin: 0;
  padding: 6px 10px;
  background: rgba(34, 197, 94, 0.08);
  border-radius: var(--radius);
  border: 1px solid rgba(34, 197, 94, 0.2);
}

.index-result-sm {
  font-size: 11px;
  padding: 4px 8px;
}

.index-result.index-error {
  color: var(--error);
  background: rgba(239, 68, 68, 0.08);
  border-color: rgba(239, 68, 68, 0.2);
}

.indexing-desc {
  font-size: 13px;
  color: var(--text-muted);
  margin: 0;
  line-height: 1.5;
}

/* â”€â”€ Buttons â”€â”€ */
.btn-primary {
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--transition);
}

.btn-primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--bg-hover);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}

.btn-secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-danger {
  background: rgba(239, 68, 68, 0.1);
  color: var(--error);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition);
}

.btn-danger:hover {
  background: rgba(239, 68, 68, 0.2);
}

.btn-sm {
  padding: 5px 10px;
  font-size: 12px;
}

/* â”€â”€ Forms (the card editor is QKanbanCardModal, styled in packages/ui) â”€â”€ */
.form-input {
  padding: 8px 12px;
  background: var(--bg-primary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-primary);
  font-family: inherit;
  font-size: 13px;
  width: 100%;
  box-sizing: border-box;
  transition: border-color var(--transition);
}

.form-input:focus {
  outline: none;
  border-color: var(--accent);
}

select.form-input {
  cursor: pointer;
}

.folder-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.folder-row .form-input {
  flex: 1;
}

</style>
