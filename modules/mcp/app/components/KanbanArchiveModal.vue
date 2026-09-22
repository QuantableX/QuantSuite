<script setup lang="ts">
/**
 * The board's archive: cards archived individually or together from Done.
 *
 * Full-size on purpose — the point of the archive is to read what was done,
 * so each card shows its whole title, its dates and its description rendered
 * as Markdown (QMarkdownPreview — the same surface the card editor's preview
 * uses). Restore hands a card back to the host, which owns the command.
 *
 * Teleported to body like the module's other modals; the overlay carries
 * data-module itself so the theme-bridge palette still applies.
 */
import { onBeforeUnmount, watch } from 'vue'
import type { KanbanCard } from '@quantsuite/core'

const props = withDefaults(
  defineProps<{
    modelValue: boolean
    cards: KanbanCard[]
    boardLabel?: string
    /** The card a restore is in flight for — its button shows the wait. */
    restoringId?: string | null
    error?: string | null
  }>(),
  { boardLabel: '', restoringId: null, error: null },
)

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  restore: [card: KanbanCard]
}>()

function close() {
  emit('update:modelValue', false)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    close()
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) window.addEventListener('keydown', onKeydown, true)
    else window.removeEventListener('keydown', onKeydown, true)
  },
  { immediate: true },
)
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true))

// Kanban timestamps are unix seconds.
function shortDate(ts?: number | null): string {
  if (!ts) return ''
  return new Date(ts * 1000).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
}

function fullDate(ts?: number | null): string {
  if (!ts) return ''
  return new Date(ts * 1000).toLocaleString()
}

function statusLabel(card: KanbanCard): string {
  return (card.status || '').replace('_', ' ')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="arch-fade">
      <div
        v-if="modelValue"
        class="arch-overlay"
        data-module="mcp"
        role="dialog"
        aria-modal="true"
        aria-label="Archived cards"
        @pointerdown.self="close"
      >
        <div class="arch-dialog">
          <header class="arch-head">
            <div class="arch-head-main">
              <span class="arch-kicker">Archive</span>
              <span v-if="boardLabel" class="arch-board" title="Board">{{ boardLabel }}</span>
              <span class="arch-total">{{ cards.length }} {{ cards.length === 1 ? 'card' : 'cards' }}</span>
            </div>
            <div class="arch-head-side">
              <span class="arch-hint">Finished work, off the board — agents still see it</span>
              <button class="arch-close" title="Close (Esc)" @click="close">&times;</button>
            </div>
          </header>

          <p v-if="error" class="arch-error">{{ error }}</p>

          <div class="arch-body">
            <div v-if="cards.length === 0" class="arch-empty">
              <p class="arch-empty-title">Nothing archived yet</p>
              <p class="arch-empty-text">
                Archive individual Done cards or use "Archive all" in the column's header. They stay
                readable, and agents keep listing them, so the board can be short without
                losing what was done.
              </p>
            </div>

            <article
              v-for="card in cards"
              :key="card.id"
              class="arch-card"
            >
              <div class="arch-card-head">
                <h3 class="arch-card-title">{{ card.title }}</h3>
                <button
                  class="arch-restore"
                  :disabled="restoringId === card.id"
                  title="Put the card back on the board, at the end of its column"
                  @click="emit('restore', card)"
                >
                  {{ restoringId === card.id ? 'Restoring…' : 'Restore' }}
                </button>
              </div>
              <div class="arch-card-meta">
                <span class="arch-priority" :class="'priority-' + (card.priority || 'medium')">
                  {{ card.priority || 'medium' }}
                </span>
                <span v-if="card.status && card.status !== 'backlog'" class="arch-chip">
                  {{ statusLabel(card) }}
                </span>
                <span v-if="card.agent_id" class="arch-chip arch-agent" :title="'Agent: ' + card.agent_id">
                  {{ card.agent_id }}
                </span>
                <span class="arch-date" :title="'Archived ' + fullDate(card.archived_at)">
                  archived {{ shortDate(card.archived_at) }}
                </span>
                <span v-if="card.merged_at" class="arch-date" :title="'Merged ' + fullDate(card.merged_at)">
                  merged {{ shortDate(card.merged_at) }}
                </span>
                <span class="arch-date" :title="'Created ' + fullDate(card.created_at)">
                  created {{ shortDate(card.created_at) }}
                </span>
              </div>
              <div v-if="card.description.trim()" class="arch-card-desc">
                <QMarkdownPreview :source="card.description" tokens="qss" />
              </div>
            </article>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.arch-overlay {
  position: fixed;
  inset: 0;
  z-index: 1100;
  display: grid;
  place-items: center;
  padding: 24px;
  background: rgb(0 0 0 / 0.55);
}

/* Teleported out of the module root — the box model is set here. */
.arch-dialog,
.arch-dialog *,
.arch-dialog *::before,
.arch-dialog *::after {
  box-sizing: border-box;
}

/* Wide and tall: the archive is for reading, not a confirm box. */
.arch-dialog {
  display: flex;
  flex-direction: column;
  width: min(1120px, 100%);
  height: min(880px, 100%);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  overflow: hidden;
  background: var(--bg-secondary);
  color: var(--text-primary);
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.45);
}

/* ── Header ── */

.arch-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px 12px 24px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.arch-head-main {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.arch-kicker {
  font-size: 12px;
  font-weight: 600;
  line-height: 1;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-secondary);
  white-space: nowrap;
}

.arch-board {
  max-width: 260px;
  padding: 3px 9px;
  border: 1px solid var(--border);
  border-radius: 999px;
  color: var(--text-primary);
  font-size: 11px;
  font-weight: 500;
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.arch-total {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
}

.arch-head-side {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}

.arch-hint {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}

.arch-close {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 7px;
  background: none;
  color: var(--text-muted);
  font-size: 20px;
  line-height: 1;
  cursor: pointer;
  transition: all var(--transition);
}

.arch-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.arch-error {
  margin: 12px 24px 0;
  padding: 6px 10px;
  border: 1px solid rgba(239, 68, 68, 0.2);
  border-radius: var(--radius);
  background: rgba(239, 68, 68, 0.08);
  color: var(--error);
  font-size: 12px;
  flex-shrink: 0;
}

/* ── Body ── */

.arch-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px 24px 24px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.arch-empty {
  margin: auto;
  max-width: 420px;
  text-align: center;
}

.arch-empty-title {
  margin: 0 0 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-secondary);
}

.arch-empty-text {
  margin: 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-muted);
}

/* One card per row, the board card's look at reading size. */
.arch-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex-shrink: 0;
}

.arch-card-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.arch-card-title {
  flex: 1;
  min-width: 0;
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  line-height: 1.4;
  color: var(--text-primary);
  overflow-wrap: anywhere;
}

.arch-restore {
  flex-shrink: 0;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-muted);
  font-family: inherit;
  font-size: 12px;
  padding: 3px 10px;
  cursor: pointer;
  transition: all var(--transition);
}

.arch-restore:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--accent);
}

.arch-restore:disabled {
  opacity: 0.5;
  cursor: default;
}

.arch-card-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 6px;
}

/* Same chips as the board card. */
.arch-priority {
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

.arch-chip {
  font-size: 10px;
  color: var(--text-muted);
  padding: 1px 5px;
  background: var(--bg-hover);
  border-radius: 3px;
}

.arch-agent {
  color: #4a9eff;
  background: #1a2a3a;
}

.arch-date {
  font-size: 11px;
  color: var(--text-muted);
}

.arch-date + .arch-date::before {
  content: '·';
  margin-right: 6px;
  opacity: 0.6;
}

/* The description grows to its content — the modal scrolls, not the card. */
.arch-card-desc {
  margin-top: 2px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-secondary);
}

.arch-card-desc :deep(.q-md) {
  height: auto;
  min-height: 0;
  overflow: visible;
  padding: 0;
  background: transparent;
}

/* Overlay fade */
.arch-fade-enter-active,
.arch-fade-leave-active {
  transition: opacity 150ms ease;
}

.arch-fade-enter-from,
.arch-fade-leave-to {
  opacity: 0;
}
</style>
