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
    <div class="qn-trash__card">
      <header class="qn-trash__head">
        <div>
          <h1>Trash</h1>
          <span class="qn-trash__sub">
            {{ rows.length }} archived note{{ rows.length === 1 ? '' : 's' }}
          </span>
        </div>
        <button v-if="rows.length" class="qn-btn" @click="confirmingEmpty = true">Empty trash</button>
      </header>

      <!-- Permanent deletion of everything deserves a second click, not a
           confirm() dialog the shell cannot style. -->
      <div v-if="confirmingEmpty" class="qn-trash__confirm">
        <span>Delete all {{ rows.length }} notes permanently?</span>
        <button class="qn-btn" @click="emptyTrash">Delete forever</button>
        <button class="qn-btn qn-btn--ghost" @click="confirmingEmpty = false">Cancel</button>
      </div>

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
            <td>{{ formatDateTime(row.updatedAt) }}</td>
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
  height: 100%;
  padding: 28px;
  overflow: auto;
  background: var(--qn-bg);
}

.qn-trash__card {
  max-width: 900px;
  margin: 0 auto;
  padding: 24px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius-lg);
  background: var(--qn-bg-sidebar);
}

.qn-trash__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}

.qn-trash__head h1 {
  margin: 0;
  font-size: 21px;
  font-weight: 700;
}

.qn-trash__sub {
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-trash__confirm {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
  padding: 10px 12px;
  border: 1px solid var(--qn-error);
  border-radius: var(--qn-radius);
  background: color-mix(in srgb, var(--qn-error) 10%, transparent);
  color: var(--qn-text);
  font-size: 13px;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--qn-border-subtle);
  font-size: 13px;
  text-align: left;
}

th {
  color: var(--qn-text-secondary);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.qn-trash__icon {
  display: inline-block;
  margin-right: 6px;
  vertical-align: -2px;
}

.qn-trash__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.qn-trash__empty {
  padding: 16px 12px;
  color: var(--qn-text-muted);
}

.qn-btn {
  padding: 5px 12px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius);
  background: var(--qn-bg-card);
  color: var(--qn-text);
  font-size: 12px;
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
