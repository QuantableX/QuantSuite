<script setup lang="ts">
/**
 * The card editor every kanban surface opens (2026-09-03).
 *
 * Replaces the 480px "Edit Card" form that QuantMCP → Workspaces and the
 * drawer's Kanban tab each carried: a three-line description box and no
 * metadata — unusable for the specs agents actually get handed. One
 * component now, so the two boards cannot drift again: a large two-pane
 * dialog, the description as a full-height Markdown editor with a rendered
 * preview (QMarkdownPreview — the same parser and sanitiser QuantCode uses),
 * column and priority in the side rail, and the card's lifecycle (status,
 * agent, branch, worktree, timestamps) read-only beneath them.
 *
 * The host owns persistence: `save` hands back a draft plus the card it
 * edits, the host maps that onto add / update / move and closes the modal
 * (`busy` and `error` feed the footer meanwhile). Read-only when an agent
 * holds the card (`readonly`) — its worktree lives outside the board, and
 * the drawer never let those be edited.
 *
 * Teleported to body and styled on `--qss-*` tokens only, so it renders the
 * same under a module root and inside the drawer — no `data-module` scope
 * needed on the overlay.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import {
  CARD_PRIORITIES,
  KANBAN_COLUMNS,
  GENERAL_BOARD_ID,
  reviewKanbanCard,
  type ApprovalMode,
  type CardStatus,
  type KanbanCard,
  type KanbanCardDraft,
  type KanbanColumn,
  type KanbanTestCommands,
} from '@quantsuite/core'

const props = withDefaults(
  defineProps<{
    modelValue: boolean
    /** The card being edited; `null` opens the editor for a new card. */
    card?: KanbanCard | null
    /** Where a new card lands. */
    column?: KanbanColumn
    /** Columns on offer — the host's visible set. */
    columns?: { id: KanbanColumn; label: string }[]
    /** Board name for the header: "General" or the workspace. */
    boardLabel?: string
    /** An agent holds the card — everything renders, nothing edits. */
    readonly?: boolean
    /** A save is in flight — the footer waits on it. */
    busy?: boolean
    /** The host's last save error, shown in the footer. */
    error?: string | null
    /** Offer the delete action on existing cards. */
    deletable?: boolean
    approvalMode?: ApprovalMode
  }>(),
  {
    card: null,
    column: 'plan',
    columns: () => KANBAN_COLUMNS,
    boardLabel: '',
    readonly: false,
    busy: false,
    error: null,
    deletable: true,
    approvalMode: 'auto_apply',
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  /** The host persists the draft; `card` is null for a new one. */
  save: [draft: KanbanCardDraft, card: KanbanCard | null]
  delete: [card: KanbanCard]
  archive: [card: KanbanCard]
}>()

const isNew = computed(() => props.card === null)

/* ── Draft ───────────────────────────────────────────────────────────── */

function blank(): KanbanCardDraft {
  return { title: '', description: '', column: props.column, priority: 'medium' }
}

function fromCard(card: KanbanCard): KanbanCardDraft {
  return {
    title: card.title,
    description: card.description ?? '',
    column: card.column,
    priority: card.priority || 'medium',
  }
}

const draft = ref<KanbanCardDraft>(blank())
// Snapshot at open — dirty is "differs from what came in", not "was typed".
let initial = JSON.stringify(draft.value)

const isDirty = computed(() => JSON.stringify(draft.value) !== initial)
const reviewBusy = ref(false)
const reviewError = ref('')
const reviewReason = ref('')
const requestChanges = ref(false)
const canApproveWork = computed(() => props.approvalMode === 'approval' && props.card?.column === 'plan'
  && props.card.workspace_id !== GENERAL_BOARD_ID && props.card.status === 'backlog'
  && !props.card.agent_id && !props.card.archived_at && !props.card.start_approved_at)
const canReviewResult = computed(() => props.card?.status === 'awaiting_review' && props.card.column === 'review' && !props.card.archived_at)

async function review(action: 'start' | 'approve' | 'reject') {
  if (!props.card || reviewBusy.value || props.busy || isDirty.value) return
  reviewBusy.value = true
  reviewError.value = ''
  try {
    await reviewKanbanCard(props.card.id, action, reviewReason.value)
    emit('update:modelValue', false)
  } catch (error) {
    reviewError.value = String(error)
  } finally {
    reviewBusy.value = false
  }
}
const canSave = computed(
  () =>
    !props.readonly &&
    !props.busy &&
    !reviewBusy.value &&
    draft.value.title.trim().length > 0 &&
    (isNew.value || isDirty.value),
)

// The host's visible set, plus the card's own column when it is folded away
// (review hides while empty in auto_apply — never while this card sits in it,
// but a stale prop must not strand the picker).
const columnOptions = computed(() =>
  KANBAN_COLUMNS.filter(
    (col) => props.columns.some((c) => c.id === col.id) || col.id === props.card?.column,
  ),
)

const view = ref<'write' | 'preview'>('write')
const confirmDiscard = ref(false)
const copied = ref(false)
const titleEl = ref<HTMLInputElement | null>(null)
const descEl = ref<HTMLTextAreaElement | null>(null)

function reset() {
  reviewError.value = ''
  reviewReason.value = ''
  requestChanges.value = false
  draft.value = props.card ? fromCard(props.card) : blank()
  initial = JSON.stringify(draft.value)
  view.value = props.readonly ? 'preview' : 'write'
  confirmDiscard.value = false
  copied.value = false
  copiedTest.value = false
  void nextTick(() => {
    if (props.readonly) return
    if (isNew.value) {
      titleEl.value?.focus()
      return
    }
    // An edit is mostly an append to the spec — land at its end.
    const el = descEl.value
    if (!el) return
    el.focus()
    const end = el.value.length
    el.setSelectionRange(end, end)
  })
}

/* ── Lifecycle metadata (read-only) ──────────────────────────────────── */

const STATUS_LABEL: Record<CardStatus, string> = {
  backlog: 'Backlog',
  in_progress: 'In progress',
  awaiting_review: 'Awaiting review',
  approved: 'Approved',
  merged: 'Merged',
  rejected: 'Rejected',
  cancelled: 'Cancelled',
}

/** kanban.db stores seconds. */
function fmt(ts?: number | null): string {
  if (!ts) return ''
  return new Date(ts * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

const timeline = computed(() => {
  const c = props.card
  if (!c) return []
  const rows: [string, number | null | undefined][] = [
    ['Created', c.created_at],
    ['Updated', c.updated_at],
    ['Work approved', c.start_approved_at],
    ['Claimed', c.claimed_at],
    ['Completed', c.completed_at],
    ['Merged', c.merged_at],
  ]
  return rows.filter(([, t]) => !!t).map(([label, t]) => ({ label, value: fmt(t) }))
})

const copiedTest = ref(false)

async function copy(text: string, flag: typeof copied) {
  try {
    await navigator.clipboard.writeText(text)
    flag.value = true
    setTimeout(() => (flag.value = false), 1200)
  } catch {
    // no clipboard here — the text stays selectable on screen
  }
}

function copyId() {
  if (props.card) void copy(props.card.id, copied)
}

/* ── Test commands (`test_commands`, derived in Rust — one per shell) ── */

type Shell = keyof KanbanTestCommands

const SHELLS: { id: Shell; label: string }[] = [
  { id: 'powershell', label: 'PowerShell' },
  { id: 'cmd', label: 'cmd' },
  { id: 'bash', label: 'bash' },
]

// The shell is a per-machine habit, not per card — remembered locally.
const SHELL_KEY = 'qkc-test-shell'

function loadShell(): Shell {
  try {
    const v = localStorage.getItem(SHELL_KEY)
    if (v === 'powershell' || v === 'cmd' || v === 'bash') return v
  } catch {
    // no storage here — PowerShell it is
  }
  return 'powershell'
}

const shell = ref<Shell>(loadShell())

function onShellChange(e: Event) {
  const s = (e.target as HTMLSelectElement).value as Shell
  shell.value = s
  try {
    localStorage.setItem(SHELL_KEY, s)
  } catch {
    // fine — the choice holds for this session
  }
}

const testCommand = computed(() => props.card?.test_commands?.[shell.value] ?? null)

function copyTestCommand() {
  if (testCommand.value) void copy(testCommand.value, copiedTest)
}

/** A plain (vertical) wheel over the one-line box drives it sideways. */
function onTestWheel(e: WheelEvent) {
  const el = e.currentTarget as HTMLElement
  if (el.scrollWidth <= el.clientWidth) return
  const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
  if (!delta) return
  e.preventDefault()
  el.scrollLeft += delta
}

/* ── Open / close / save ─────────────────────────────────────────────── */

function close() {
  emit('update:modelValue', false)
}

function requestClose() {
  if (props.busy || reviewBusy.value) return
  if (isDirty.value && !props.readonly) {
    confirmDiscard.value = true
    return
  }
  close()
}

function save() {
  if (!canSave.value) return
  emit('save', { ...draft.value, title: draft.value.title.trim() }, props.card)
}

function remove() {
  if (props.card && !reviewBusy.value) emit('delete', props.card)
}

function archive() {
  if (!props.card || props.card.column !== 'done' || props.card.archived_at != null || props.readonly || props.busy || isDirty.value) return
  emit('archive', props.card)
}

// Captured on window so the palette and module shortcuts stay quiet while
// the editor is up; a textarea does nothing with Escape or Ctrl+Enter.
function onKeydown(e: KeyboardEvent) {
  if (!props.modelValue) return
  if (e.key === 'Escape') {
    e.stopPropagation()
    e.preventDefault()
    requestClose()
    return
  }
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    save()
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      reset()
      window.addEventListener('keydown', onKeydown, true)
    } else {
      window.removeEventListener('keydown', onKeydown, true)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true))
</script>

<template>
  <Teleport to="body">
    <Transition name="qkc-fade">
      <div
        v-if="modelValue"
        class="qkc-overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="isNew ? 'New card' : 'Edit card'"
        @pointerdown.self="requestClose"
      >
        <div class="qkc-dialog" :class="{ 'is-readonly': readonly }">
          <header class="qkc-head">
            <div class="qkc-head-main">
              <span class="qkc-kicker">{{ isNew ? 'New card' : 'Edit card' }}</span>
              <span v-if="boardLabel" class="qkc-board" title="Board">{{ boardLabel }}</span>
              <span v-if="readonly" class="qkc-lock">agent holds this card · read-only</span>
            </div>
            <div class="qkc-head-side">
              <span v-if="!readonly" class="qkc-hint"><kbd>Ctrl</kbd><kbd>↵</kbd> save · <kbd>Esc</kbd> close</span>
              <button class="qkc-close" title="Close" @click="requestClose">&times;</button>
            </div>
          </header>

          <div class="qkc-body">
            <section class="qkc-main">
              <input
                v-if="!readonly"
                ref="titleEl"
                v-model="draft.title"
                class="qkc-title"
                placeholder="Card title"
                maxlength="200"
                spellcheck="true"
                @keydown.enter.prevent="descEl?.focus()"
              />
              <h2 v-else class="qkc-title is-static">{{ draft.title }}</h2>

              <div class="qkc-desc">
                <div class="qkc-desc-bar">
                  <span class="qkc-label">Description</span>
                  <span class="qkc-muted">Markdown</span>
                  <div v-if="!readonly" class="qkc-seg" role="tablist">
                    <button
                      role="tab"
                      :aria-selected="view === 'write'"
                      :class="{ 'is-on': view === 'write' }"
                      @click="view = 'write'"
                    >
                      Write
                    </button>
                    <button
                      role="tab"
                      :aria-selected="view === 'preview'"
                      :class="{ 'is-on': view === 'preview' }"
                      @click="view = 'preview'"
                    >
                      Preview
                    </button>
                  </div>
                </div>
                <textarea
                  v-if="view === 'write'"
                  ref="descEl"
                  v-model="draft.description"
                  class="qkc-textarea"
                  placeholder="Describe the work: goal, scope, acceptance criteria. Markdown renders in Preview."
                  spellcheck="true"
                />
                <div v-else class="qkc-preview">
                  <QMarkdownPreview v-if="draft.description.trim()" :source="draft.description" tokens="qss" />
                  <p v-else class="qkc-empty">No description.</p>
                </div>
              </div>

              <!-- The agent's version, runnable: the same one-liner the MCP
                   tools quote to the agent, here for the user to copy. Always
                   present on an existing card (user decision 2026-09-03) —
                   the command only exists while the worktree does. -->
              <div v-if="card" class="qkc-test">
                <code v-if="testCommand" class="qkc-test-cmd" :title="testCommand" @wheel="onTestWheel">{{ testCommand }}</code>
                <code v-else class="qkc-test-cmd is-empty">No live worktree found</code>
                <select
                  v-if="card.test_commands"
                  class="qkc-test-shell"
                  :value="shell"
                  aria-label="Shell"
                  title="Shell"
                  @change="onShellChange"
                >
                  <option v-for="s in SHELLS" :key="s.id" :value="s.id">{{ s.label }}</option>
                </select>
                <button
                  class="qkc-test-copy"
                  :disabled="!testCommand"
                  :title="copiedTest ? 'Copied' : 'Copy the test command (stop the main app\'s tauri:dev first)'"
                  aria-label="Copy test command"
                  @click="copyTestCommand"
                >
                  <svg v-if="copiedTest" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M20 6 9 17l-5-5" />
                  </svg>
                  <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
                    <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
                  </svg>
                </button>
              </div>
            </section>

            <aside class="qkc-side">
              <div class="qkc-field">
                <span class="qkc-label">Column</span>
                <div class="qkc-seg is-fill">
                  <button
                    v-for="col in columnOptions"
                    :key="col.id"
                    :class="{ 'is-on': draft.column === col.id }"
                    :disabled="readonly"
                    @click="draft.column = col.id"
                  >
                    {{ col.label }}
                  </button>
                </div>
              </div>

              <div class="qkc-field">
                <span class="qkc-label">Priority</span>
                <div class="qkc-seg is-fill">
                  <button
                    v-for="p in CARD_PRIORITIES"
                    :key="p.id"
                    :class="['is-' + p.id, { 'is-on': draft.priority === p.id }]"
                    :disabled="readonly"
                    @click="draft.priority = p.id"
                  >
                    <i class="qkc-dot" />{{ p.label }}
                  </button>
                </div>
              </div>

              <template v-if="card">
                <div class="qkc-rule" />
                <dl class="qkc-meta">
                  <dt>Status</dt>
                  <dd>
                    <span class="qkc-status" :class="'is-' + card.status">
                      {{ STATUS_LABEL[card.status] ?? card.status }}
                    </span>
                  </dd>
                  <template v-if="card.agent_id">
                    <dt>Agent</dt>
                    <dd class="mono">{{ card.agent_id }}</dd>
                  </template>
                  <template v-if="card.branch">
                    <dt>Branch</dt>
                    <dd class="mono">{{ card.branch }}</dd>
                  </template>
                  <template v-if="card.worktree_path">
                    <dt>Worktree</dt>
                    <dd class="mono" :title="card.worktree_path">{{ card.worktree_path }}</dd>
                  </template>
                  <template v-for="t in timeline" :key="t.label">
                    <dt>{{ t.label }}</dt>
                    <dd>{{ t.value }}</dd>
                  </template>
                  <dt>ID</dt>
                  <dd>
                    <button class="qkc-id" :title="'Copy ' + card.id" @click="copyId">
                      {{ copied ? 'copied' : card.id.slice(0, 8) }}
                    </button>
                  </dd>
                </dl>
              </template>
            </aside>
          </div>

          <div v-if="canApproveWork || canReviewResult || (approvalMode === 'approval' && card?.column === 'plan' && card.start_approved_at)" class="qkc-review">
            <p v-if="canApproveWork">Review the requirement, then approve work so the agent can start.</p>
            <p v-else-if="canReviewResult">Test this result before approving. Approval merges the work and finishes the card.</p>
            <p v-else>Work approved. The agent may claim this card.</p>
            <textarea v-if="requestChanges" v-model="reviewReason" class="qkc-review-reason" aria-label="Requested changes" placeholder="Describe the changes you want" :disabled="reviewBusy" />
            <div class="qkc-review-actions">
              <button v-if="canApproveWork" class="qkc-btn is-primary" :disabled="busy || reviewBusy || isDirty" @click="review('start')">Approve work</button>
              <template v-if="canReviewResult">
                <button class="qkc-btn" :disabled="busy || reviewBusy" @click="requestChanges = !requestChanges">{{ requestChanges ? 'Cancel request' : 'Request changes' }}</button>
                <button v-if="requestChanges" class="qkc-btn" :disabled="busy || reviewBusy || !reviewReason.trim()" @click="review('reject')">Send changes</button>
                <button v-else class="qkc-btn is-primary" :disabled="busy || reviewBusy || isDirty" @click="review('approve')">Approve result</button>
              </template>
              <span v-if="isDirty" class="qkc-foot-msg">Save changes before approving.</span>
              <span v-if="reviewBusy" role="status">Applying…</span>
            </div>
            <p v-if="reviewError" class="qkc-foot-err" role="alert">{{ reviewError }}</p>
          </div>
          <footer class="qkc-foot">
            <template v-if="confirmDiscard">
              <span class="qkc-foot-msg">Discard unsaved changes?</span>
              <span class="qkc-foot-spacer" />
              <button class="qkc-btn" @click="confirmDiscard = false">Keep editing</button>
              <button class="qkc-btn is-danger" @click="close">Discard</button>
            </template>
            <template v-else>
              <button
                v-if="card && deletable && !readonly"
                class="qkc-btn is-ghost-danger"
                :disabled="busy"
                @click="remove"
              >
                Delete
              </button>
              <button
                v-if="card?.column === 'done' && card.archived_at == null && !readonly"
                class="qkc-btn"
                :disabled="busy || isDirty"
                :title="isDirty ? 'Save or discard your changes before archiving' : 'Archive card'"
                @click="archive"
              >
                Archive
              </button>
              <span v-if="error" class="qkc-foot-err" role="alert" :title="error">{{ error }}</span>
              <span v-else-if="isDirty && !readonly" class="qkc-foot-msg">Unsaved changes</span>
              <span class="qkc-foot-spacer" />
              <button class="qkc-btn" :disabled="busy" @click="requestClose">
                {{ readonly ? 'Close' : 'Cancel' }}
              </button>
              <button v-if="!readonly" class="qkc-btn is-primary" :disabled="!canSave" @click="save">
                {{ busy ? 'Saving…' : isNew ? 'Add card' : 'Save' }}
              </button>
            </template>
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.qkc-review {
  padding: 12px 20px;
  border-top: 1px solid var(--qss-border);
  font-size: 12px;
  flex-shrink: 0;
}
.qkc-review p { margin: 0 0 8px; }
.qkc-review-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
.qkc-review-reason {
  width: 100%; min-height: 60px; margin-bottom: 8px; padding: 8px;
  color: var(--qss-text); background: var(--qss-bg); border: 1px solid var(--qss-border); border-radius: 6px;
}
.qkc-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: grid;
  place-items: center;
  padding: 24px;
  background: rgb(0 0 0 / 0.55);
}

/* Teleported out of every module root — the box model is set here, not
 * inherited from a `[data-module] *` reset. */
.qkc-dialog,
.qkc-dialog *,
.qkc-dialog *::before,
.qkc-dialog *::after {
  box-sizing: border-box;
}

.qkc-dialog {
  display: flex;
  flex-direction: column;
  width: min(980px, 100%);
  height: min(800px, 100%);
  border: 1px solid var(--qss-border);
  border-radius: 12px;
  overflow: hidden;
  background: var(--qss-bg);
  color: var(--qss-text);
  font-family: var(--qss-font-sans);
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.45);
}

/* ── Header ── */

.qkc-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px 12px 24px;
  border-bottom: 1px solid var(--qss-border);
}
.qkc-head-main {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.qkc-kicker {
  font: 600 12px/1 var(--qss-font-sans);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--qss-text-secondary);
  white-space: nowrap;
}
.qkc-board {
  max-width: 260px;
  padding: 3px 9px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  color: var(--qss-text);
  font: 500 11px/1.4 var(--qss-font-sans);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.qkc-lock {
  font-size: 11px;
  color: var(--qss-warning);
  white-space: nowrap;
}
.qkc-head-side {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}
.qkc-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qkc-hint kbd {
  padding: 3px 5px;
  border: 1px solid var(--qss-border);
  border-bottom-width: 2px;
  border-radius: 5px;
  background: var(--qss-bg-raised);
  color: var(--qss-text-secondary);
  font: 500 10px/1 var(--qss-font-sans);
}
.qkc-close {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 7px;
  background: none;
  color: var(--qss-text-muted);
  font-size: 20px;
  line-height: 1;
  cursor: pointer;
}
.qkc-close:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

/* ── Body: editor left, rail right ── */

.qkc-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 272px;
}

.qkc-main {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 0;
  padding: 20px 24px;
  overflow: auto;
}

.qkc-title {
  width: 100%;
  margin: 0;
  padding: 4px 0 8px;
  border: none;
  border-bottom: 1px solid transparent;
  background: none;
  color: var(--qss-text);
  font: 600 20px/1.3 var(--qss-font-sans);
  outline: none;
}
.qkc-title::placeholder {
  color: var(--qss-text-muted);
}
.qkc-title:focus {
  border-bottom-color: var(--qss-border);
}
.qkc-title.is-static {
  overflow-wrap: anywhere;
  user-select: text;
}

.qkc-desc {
  flex: 1;
  /* Never below its own editor's minimum (bar + gap + 200px textarea):
   * at `min-height: 0` the box shrank under the textarea on short windows
   * and whatever follows (the test command) was drawn on top of it. The
   * main column scrolls instead. */
  min-height: 236px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qkc-desc-bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
.qkc-desc-bar .qkc-seg {
  margin-left: auto;
}

.qkc-label {
  font: 600 11px/1 var(--qss-font-sans);
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--qss-text-secondary);
}
.qkc-muted {
  font-size: 11px;
  color: var(--qss-text-muted);
}

.qkc-textarea {
  flex: 1;
  width: 100%;
  min-height: 200px;
  padding: 12px 14px;
  border: 1px solid var(--qss-border);
  border-radius: 10px;
  background: var(--qss-bg-raised);
  color: var(--qss-text);
  font: 400 13.5px/1.6 var(--qss-font-sans);
  resize: none;
  outline: none;
  tab-size: 2;
}
.qkc-textarea::placeholder {
  color: var(--qss-text-muted);
}
.qkc-textarea:focus {
  border-color: color-mix(in srgb, var(--qss-accent) 55%, var(--qss-border));
}

.qkc-preview {
  flex: 1;
  min-height: 200px;
  overflow: auto;
  border: 1px solid var(--qss-border);
  border-radius: 10px;
  background: var(--qss-bg-raised);
}
/* The preview scrolls as a whole — not the markdown surface inside it. */
.qkc-preview :deep(.q-md) {
  height: auto;
  min-height: 100%;
  overflow: visible;
}
.qkc-empty {
  margin: 0;
  padding: 16px 20px;
  font-size: 13px;
  color: var(--qss-text-muted);
}

/* ── Test command (a claimed card's worktree) ── */

/* One box: the command on the left, the shell switch and the copy button
 * fixed on the right; the command is ONE line that scrolls sideways under
 * them, scrollbar hidden (user decision 2026-09-03). */
.qkc-test {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 6px 5px 12px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
}
.qkc-test-cmd {
  flex: 1;
  min-width: 0;
  padding: 4px 0;
  color: var(--qss-text);
  font: 400 11.5px/1.5 var(--qss-font-mono);
  white-space: nowrap;
  overflow-x: auto;
  scrollbar-width: none;
  user-select: text;
  cursor: text;
}
.qkc-test-cmd::-webkit-scrollbar {
  display: none;
}
.qkc-test-cmd.is-empty {
  color: var(--qss-text-muted);
  user-select: none;
  cursor: default;
}
.qkc-test-shell {
  flex-shrink: 0;
  height: 24px;
  padding: 0 6px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
  color: var(--qss-text-secondary);
  font: 500 11px/1 var(--qss-font-sans);
  cursor: pointer;
}
.qkc-test-shell:hover {
  color: var(--qss-text);
}
.qkc-test-shell:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}
.qkc-test-copy {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.qkc-test-copy:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qkc-test-copy:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* ── Segmented controls (Write/Preview, column, priority) ── */

.qkc-seg {
  display: inline-flex;
  padding: 2px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
}
.qkc-seg.is-fill {
  display: flex;
  width: 100%;
}
.qkc-seg button {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  min-width: 0;
  padding: 5px 9px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--qss-text-secondary);
  font: 500 11.5px/1.2 var(--qss-font-sans);
  white-space: nowrap;
  cursor: pointer;
}
.qkc-seg button:hover:not(:disabled) {
  color: var(--qss-text);
}
.qkc-seg button.is-on {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
}
.qkc-seg button:disabled {
  cursor: default;
  opacity: 0.7;
}

.qkc-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}
/* The board's priority colours (projects.vue), on the dot; the label takes
 * them only when selected. */
.qkc-seg .is-low .qkc-dot {
  background: #6fbf6f;
}
.qkc-seg .is-medium .qkc-dot {
  background: #c8a84e;
}
.qkc-seg .is-high .qkc-dot {
  background: #e08040;
}
.qkc-seg .is-critical .qkc-dot {
  background: #e04040;
}
.qkc-seg button.is-on.is-low {
  color: #8fd08f;
}
.qkc-seg button.is-on.is-medium {
  color: #d8bb64;
}
.qkc-seg button.is-on.is-high {
  color: #ec9560;
}
.qkc-seg button.is-on.is-critical {
  color: #f06060;
}

/* ── Side rail ── */

.qkc-side {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 0;
  padding: 20px 18px;
  border-left: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
  overflow: auto;
}
/* On the raised rail the controls sit sunken instead. */
.qkc-side .qkc-seg {
  background: var(--qss-bg);
}

.qkc-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.qkc-rule {
  height: 1px;
  flex-shrink: 0;
  background: var(--qss-border);
}

.qkc-meta {
  display: grid;
  grid-template-columns: 72px minmax(0, 1fr);
  gap: 8px 10px;
  margin: 0;
  font-size: 12px;
}
.qkc-meta dt {
  color: var(--qss-text-muted);
}
.qkc-meta dd {
  min-width: 0;
  margin: 0;
  color: var(--qss-text);
  overflow-wrap: anywhere;
  user-select: text;
}
.qkc-meta dd.mono {
  font: 400 11px/1.5 var(--qss-font-mono);
}

.qkc-status {
  display: inline-block;
  padding: 2px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  color: var(--qss-text-secondary);
  font: 500 11px/1.4 var(--qss-font-sans);
}
.qkc-status.is-in_progress {
  color: var(--qss-accent);
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}
.qkc-status.is-awaiting_review {
  color: var(--qss-warning);
  border-color: color-mix(in srgb, var(--qss-warning) 50%, var(--qss-border));
}
.qkc-status.is-approved,
.qkc-status.is-merged {
  color: var(--qss-success);
  border-color: color-mix(in srgb, var(--qss-success) 50%, var(--qss-border));
}
.qkc-status.is-rejected {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 50%, var(--qss-border));
}

.qkc-id {
  padding: 0;
  border: none;
  background: none;
  color: var(--qss-text-secondary);
  font: 400 11px/1.5 var(--qss-font-mono);
  cursor: pointer;
}
.qkc-id:hover {
  color: var(--qss-text);
  text-decoration: underline;
}

/* ── Footer ── */

.qkc-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--qss-border);
}
.qkc-foot-spacer {
  flex: 1;
}
.qkc-foot-msg {
  font-size: 12px;
  color: var(--qss-text-muted);
}
.qkc-foot-err {
  min-width: 0;
  font-size: 12px;
  color: var(--qss-error);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qkc-btn {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-raised);
  color: var(--qss-text);
  font: 500 12.5px/1 var(--qss-font-sans);
  cursor: pointer;
  transition:
    border-color 120ms ease,
    background 120ms ease,
    color 120ms ease;
}
.qkc-btn:hover:not(:disabled) {
  background: var(--qss-bg-hover);
}
.qkc-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.qkc-btn.is-primary {
  border-color: transparent;
  background: var(--qss-accent);
  color: var(--qss-accent-ink, #131315);
  font-weight: 600;
}
.qkc-btn.is-primary:hover:not(:disabled) {
  background: var(--qss-accent-hover, color-mix(in srgb, var(--qss-accent) 86%, #fff));
}
.qkc-btn.is-danger {
  border-color: color-mix(in srgb, var(--qss-error) 45%, var(--qss-border));
  background: color-mix(in srgb, var(--qss-error) 14%, transparent);
  color: var(--qss-error);
}
.qkc-btn.is-danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--qss-error) 24%, transparent);
}
.qkc-btn.is-ghost-danger {
  border-color: transparent;
  background: none;
  color: var(--qss-text-muted);
}
.qkc-btn.is-ghost-danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--qss-error) 10%, transparent);
  color: var(--qss-error);
}

/* ── Motion ── */

.qkc-fade-enter-active {
  transition: opacity 140ms ease;
}
.qkc-fade-leave-active {
  transition: opacity 100ms ease;
}
.qkc-fade-enter-from,
.qkc-fade-leave-to {
  opacity: 0;
}
.qkc-fade-enter-active .qkc-dialog {
  transition: transform 140ms ease;
}
.qkc-fade-enter-from .qkc-dialog {
  transform: scale(0.985);
}

/* ── Narrow windows: the rail drops under the editor ── */

@media (max-width: 760px) {
  .qkc-overlay {
    padding: 12px;
  }
  /* A column, not a grid: an `auto` grid row kept stretching past the
   * rail's max-height and left a dead band above the footer. */
  .qkc-body {
    display: flex;
    flex-direction: column;
  }
  .qkc-main {
    flex: 1 1 auto;
  }
  .qkc-side {
    flex: 0 1 auto;
    max-height: 45%;
    border-left: none;
    border-top: 1px solid var(--qss-border);
  }
  .qkc-hint {
    display: none;
  }
}

@media (prefers-reduced-motion: reduce) {
  .qkc-fade-enter-active,
  .qkc-fade-leave-active,
  .qkc-fade-enter-active .qkc-dialog,
  .qkc-btn {
    transition: none;
  }
}
</style>
