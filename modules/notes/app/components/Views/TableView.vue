<script setup lang="ts">
/**
 * The table view — one row per note, one column per visible property.
 *
 * The title column is sticky and always first: it is the note's identity and
 * the only cell that navigates rather than edits.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { NoteSummary, View } from '#notes/types'

const props = defineProps<{ rows: NoteSummary[]; view: View }>()

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const columns = computed(() => schema.visibleProperties(props.view))

function open(note: NoteSummary) {
  router.push(`/notes/n/${note.id}`)
}

async function addRow() {
  const note = await notes.create({})
  router.push(`/notes/n/${note.id}`)
}
</script>

<template>
  <div class="qn-table">
    <div class="qn-table__scroll">
      <table>
        <thead>
          <tr>
            <th class="qn-table__title-col">Name</th>
            <th v-for="column in columns" :key="column.id">{{ column.name }}</th>
            <th class="qn-table__pad" />
          </tr>
        </thead>
        <tbody>
          <tr v-for="note in rows" :key="note.id">
            <td class="qn-table__title-col">
              <button class="qn-table__title" @click="open(note)">
                <NotesNoteIcon v-if="note.icon" class="qn-table__icon" :icon="note.icon" :size="14" />
                <span class="qn-table__name">{{ note.title || 'Untitled' }}</span>
              </button>
            </td>
            <td v-for="column in columns" :key="column.id">
              <NotesViewsPropertyCell
                :note-id="note.id"
                :property="column"
                :value="note.properties[column.id]"
              />
            </td>
            <td class="qn-table__pad" />
          </tr>
          <tr class="qn-table__new">
            <td :colspan="columns.length + 2">
              <button class="qn-table__new-btn" @click="addRow">+ New note</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.qn-table {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.qn-table__scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

table {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0;
  font-size: 13px;
}

thead th {
  position: sticky;
  top: 0;
  z-index: 2;
  padding: 8px 10px;
  border-bottom: 1px solid var(--qn-border);
  background: var(--qn-bg);
  color: var(--qn-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  text-align: left;
  white-space: nowrap;
}

tbody td {
  padding: 2px 4px;
  border-bottom: 1px solid var(--qn-border-subtle);
  vertical-align: middle;
}

tbody tr:hover td {
  background: color-mix(in srgb, var(--qn-bg-hover) 55%, transparent);
}

/* The identity column stays put while the properties scroll sideways. */
.qn-table__title-col {
  position: sticky;
  left: 0;
  z-index: 1;
  min-width: 240px;
  background: var(--qn-bg);
}

thead .qn-table__title-col {
  z-index: 3;
}

tbody tr:hover .qn-table__title-col {
  background: color-mix(in srgb, var(--qn-bg-hover) 55%, var(--qn-bg));
}

.qn-table__title {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
}

.qn-table__title:hover {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.qn-table__icon {
  flex-shrink: 0;
}

.qn-table__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-table__pad {
  width: 100%;
}

.qn-table__new td {
  border-bottom: none;
}

.qn-table__new-btn {
  width: 100%;
  padding: 9px 10px;
  border: none;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qn-table__new-btn:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>
