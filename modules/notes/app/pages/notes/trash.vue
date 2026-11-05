<script setup lang="ts">
/** Archived notes: restore one, delete one, or empty the whole thing. */
definePageMeta({ layout: 'notes' })

import { useNotesStore } from '#notes/stores/notes'
import { formatDateTime } from '#notes/utils/format'

const notes = useNotesStore()

onMounted(() => notes.loadArchived())

const rows = computed(() =>
  [...notes.archived].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
)

const confirmingEmpty = ref(false)

async function emptyTrash() {
  confirmingEmpty.value = false
  await notes.emptyTrash()
}
</script>

<template>
  <div class="qn-trash">
    <header class="qn-trash__head">
      <h1>Trash</h1>
      <span class="qn-trash__sub">
        {{ rows.length }} archived note{{ rows.length === 1 ? '' : 's' }}
      </span>
      <button v-if="rows.length" class="qn-btn" @click="confirmingEmpty = true">Empty trash</button>
    </header>

    <!-- Permanent deletion of everything deserves a second click, not a
         confirm() dialog the shell cannot style. -->
    <div v-if="confirmingEmpty" class="qn-trash__confirm">
      <span>Delete all {{ rows.length }} notes permanently?</span>
      <button class="qn-btn" @click="emptyTrash">Delete forever</button>
      <button class="qn-btn qn-btn--ghost" @click="confirmingEmpty = false">Cancel</button>
    </div>

    <div class="qn-trash__scroll">
      <table>
        <thead>
          <tr>
            <th>Title</th>
            <th>Archived</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.id">
            <td>
              <NotesNoteIcon class="qn-trash__icon" :icon="row.icon" :size="14" />
              {{ row.title || 'Untitled' }}
            </td>
            <td class="qn-trash__when">{{ formatDateTime(row.updatedAt) }}</td>
            <td class="qn-trash__actions">
              <button class="qn-btn qn-btn--ghost" @click="notes.restore(row.id)">Restore</button>
              <button class="qn-btn" @click="notes.remove(row.id)">Delete</button>
            </td>
          </tr>
          <tr v-if="!rows.length">
            <td colspan="3" class="qn-trash__empty">Nothing in the trash.</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.qn-trash {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  min-height: 0;
  padding: var(--qss-page-edge);
  background: var(--qn-bg);
}

.qn-trash__head {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight) var(--qss-gap);
  min-height: var(--qss-ctl-h);
}

.qn-trash__head h1 {
  margin: 0;
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -0.01em;
}

.qn-trash__sub {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
}

.qn-trash__head .qn-btn {
  margin-left: auto;
}

.qn-trash__confirm {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight);
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
  border: 1px solid var(--qn-error);
  border-radius: var(--qss-ctl-radius);
  background: color-mix(in srgb, var(--qn-error) 10%, transparent);
  color: var(--qn-text);
  font-size: 13px;
}

.qn-trash__confirm > span {
  margin-right: auto;
}

/* Hugs its rows, and scrolls inside itself once they outgrow the page. */
.qn-trash__scroll {
  flex: 0 1 auto;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--qn-border-subtle);
  border-radius: var(--qss-panel-radius);
}

table {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0;
}

th,
td {
  height: var(--qss-row-h);
  padding: 0 10px;
  border-bottom: 1px solid var(--qn-border-subtle);
  font-size: 13px;
  text-align: left;
}

tbody tr:last-child td {
  border-bottom: none;
}

th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--qn-bg-sidebar);
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-meta);
  font-weight: 500;
}

tbody tr:hover td {
  background: color-mix(in srgb, var(--qn-bg-hover) 55%, transparent);
}

.qn-trash__icon {
  display: inline-block;
  margin-right: 6px;
  vertical-align: -2px;
}

.qn-trash__when {
  color: var(--qn-text-secondary);
  font-size: var(--qss-text-label);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.qn-trash__actions {
  width: 1%;
  padding-right: 4px;
  text-align: right;
  white-space: nowrap;
}

.qn-trash__actions .qn-btn + .qn-btn {
  margin-left: 4px;
}

.qn-trash__empty {
  color: var(--qn-text-muted);
}

.qn-btn {
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qss-ctl-radius);
  background: var(--qn-bg-card);
  color: var(--qn-text);
  font-size: var(--qss-text-label);
  cursor: pointer;
  transition: all 150ms ease;
}

.qn-btn:hover {
  border-color: var(--qn-error);
  background: var(--qn-bg-hover);
  color: var(--qn-error);
}

.qn-btn--ghost {
  background: transparent;
  color: var(--qn-text-secondary);
}

.qn-btn--ghost:hover {
  border-color: var(--qn-accent);
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>
