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
      <NotesViewsViewToolbar />
    </header>

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
}

/* Title, view tabs and the view's toolbar share one row; they wrap when the middle is narrow.
   The hairline is the rule the active tab sits on. */
.qn-coll__top {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0 var(--qss-gap);
  padding: 0 var(--qss-page-edge);
  border-bottom: 1px solid var(--qn-border-subtle);
}

.qn-coll__name {
  flex-shrink: 0;
  margin: 0;
  color: var(--qn-text);
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -0.01em;
  white-space: nowrap;
}

.qn-coll__stage {
  flex: 1 1 0;
  min-height: 0;
  padding: var(--qss-gap) var(--qss-page-edge) var(--qss-page-edge);
  container-type: inline-size;
}
</style>
