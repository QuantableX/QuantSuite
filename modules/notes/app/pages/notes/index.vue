<script setup lang="ts">
/**
 * The collection — the module's home.
 *
 * View tabs, that view's toolbar, and whichever renderer the active view asks
 * for. All five render the same `rows`, so a filter or a sort means the same
 * thing everywhere.
 */
definePageMeta({ layout: 'notes' })

import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'
import { rowsForView } from '#notes/composables/useViewRows'

const notes = useNotesStore()
const schema = useSchemaStore()

// The collection page owns no note; clearing it keeps the header breadcrumb
// and the right panel honest when arriving back from an open note.
onMounted(() => notes.close())

const view = computed(() => schema.activeView)
const rows = computed(() => rowsForView(notes.list, view.value, schema.propertyById))
</script>

<template>
  <div class="qn-coll">
    <header class="qn-coll__top">
      <h1 class="qn-coll__name">All notes</h1>
      <NotesViewsViewTabs />
    </header>

    <NotesViewsViewToolbar />

    <div class="qn-coll__stage">
      <template v-if="view">
        <NotesViewsTableView v-if="view.kind === 'table'" :rows="rows" :view="view" />
        <NotesViewsBoardView v-else-if="view.kind === 'board'" :rows="rows" :view="view" />
        <NotesViewsListView v-else-if="view.kind === 'list'" :rows="rows" :view="view" />
        <NotesViewsGalleryView v-else-if="view.kind === 'gallery'" :rows="rows" :view="view" />
        <NotesViewsCalendarView v-else-if="view.kind === 'calendar'" :rows="rows" :view="view" />
      </template>
      <QEmptyState v-else title="No views yet" description="Add a view with + to organize your notes." />
    </div>
  </div>
</template>

<style scoped>
.qn-coll {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  padding: 0 20px;
}

.qn-coll__top {
  display: flex;
  align-items: baseline;
  gap: 20px;
  padding: 18px 0 0;
  border-bottom: 1px solid var(--qn-border);
}

.qn-coll__name {
  margin: 0;
  color: var(--qn-text);
  font-size: 21px;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.qn-coll__stage {
  flex: 1;
  min-height: 0;
}

.qn-coll__empty {
  padding: 40px;
  color: var(--qn-text-muted);
  font-size: 13px;
  text-align: center;
}
</style>
