<script setup lang="ts">
/**
 * The calendar surface — whichever grid the active view asks for, plus the
 * editor when it is open.
 *
 * View and cursor live in the query so a deep link works and the V3 warm cache
 * restores the right week rather than today's.
 */
definePageMeta({ layout: 'flow' })

import { useAppStore } from '#plan/stores/app'
import { useEventsStore } from '#plan/stores/events'
import type { EventPatch, Occurrence, SeriesScope, ViewKind } from '#plan/types'
import { fromKey, toKey } from '#plan/utils/datetime'

const app = useAppStore()
const events = useEventsStore()
const route = useRoute()
const router = useRouter()

const editing = ref<Occurrence | null>(null)
const createAt = ref<Date | null>(null)

/** A drag on a recurring block has to ask before it writes. */
const pendingSeries = ref<{ occurrence: Occurrence; patch: EventPatch } | null>(null)

const VIEWS: ViewKind[] = ['day', 'week', 'month', 'agenda']

// Query → store, once on arrival and on back/forward.
watch(
  () => route.query,
  (query) => {
    const v = String(query.v ?? '')
    if (VIEWS.includes(v as ViewKind)) app.setView(v as ViewKind)
    const d = String(query.d ?? '')
    if (/^\d{4}-\d{2}-\d{2}$/.test(d)) app.setCursor(fromKey(d))
  },
  { immediate: true },
)

// Store → query. `replace`, so paging through weeks does not fill the history.
watch(
  () => [app.view, app.cursor] as const,
  ([view, cursor]) => {
    const next = { v: view, d: toKey(cursor) }
    if (route.query.v === next.v && route.query.d === next.d) return
    void router.replace({ query: { ...route.query, ...next } })
  },
)

/**
 * Set by `openEditor`, and read once by the flag watcher below.
 *
 * The watcher cannot tell an open that came from this page from one the
 * sidebar or the `c` shortcut flipped, because both end at the same boolean —
 * so the page says which it was.
 */
let openedHere = false

function openEditor(occ: Occurrence | null, at: Date | null = null) {
  editing.value = occ
  createAt.value = at
  openedHere = true
  app.editorOpen = true
}

function closeEditor() {
  app.editorOpen = false
  editing.value = null
  createAt.value = null
}

// The sidebar's Create button and the `c` shortcut both just flip the flag.
watch(
  () => app.editorOpen,
  (open) => {
    if (!open) {
      editing.value = null
      createAt.value = null
      openedHere = false
      return
    }
    const fromPage = openedHere
    openedHere = false
    // Opened from OUTSIDE this page with a selection: edit that. A click on
    // the grid must never land here — it carries the time that was clicked,
    // and falling back to whatever is still selected would throw that away
    // and reopen the last event instead.
    if (!fromPage && !editing.value && events.selected) editing.value = events.selected
  },
)

function onSelect(occ: Occurrence) {
  events.selected = occ
}

async function applyPendingSeries(scope: SeriesScope) {
  const pending = pendingSeries.value
  pendingSeries.value = null
  if (!pending) return
  await events.applyToSeries(pending.occurrence, scope, pending.patch)
}
</script>

<template>
  <div class="qp-page">
    <PlanGridTimeGrid
      v-if="app.view === 'day' || app.view === 'week'"
      @select="onSelect"
      @edit="openEditor($event)"
      @create-at="openEditor(null, $event)"
      @series-change="pendingSeries = $event"
    />
    <PlanGridMonthGrid
      v-else-if="app.view === 'month'"
      @select="onSelect"
      @edit="openEditor($event)"
      @create-at="openEditor(null, $event)"
    />
    <PlanGridAgendaList v-else @select="onSelect" @edit="openEditor($event)" />

    <PlanEventEditor
      v-if="app.editorOpen"
      :occurrence="editing"
      :at="createAt"
      @close="closeEditor"
    />

    <!-- Dragging one instance of a series is the other place the three-way
         question has to be asked; the editor owns the same question for the
         form path. -->
    <div v-if="pendingSeries" class="qp-page__ask">
      <span>Move which events?</span>
      <button class="qp-btn" @click="applyPendingSeries('this')">This event</button>
      <button class="qp-btn" @click="applyPendingSeries('following')">This and following</button>
      <button class="qp-btn" @click="applyPendingSeries('all')">All events</button>
      <button class="qp-btn" @click="pendingSeries = null">Cancel</button>
    </div>
  </div>
</template>

<style scoped>
.qp-page {
  position: relative;
  height: 100%;
  min-height: 0;
}

/* Absolute, never fixed — `.qss-module-root` has translateZ(0) and is the
   containing block (PLAN-V3 §3). */
.qp-page__ask {
  position: absolute;
  left: 50%;
  bottom: 20px;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius-lg);
  background: var(--qp-bg-raised);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.45);
  color: var(--qp-text-secondary);
  font-size: 12px;
  transform: translateX(-50%);
}
</style>
