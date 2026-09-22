<script setup lang="ts">
/**
 * The list view — one line per note, properties trailing on the right.
 *
 * The compact reading mode: no column headers, no cards, nothing to scan
 * horizontally. Properties render read-only here; the table is where you edit
 * in bulk and the note itself is where you edit one.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { NoteSummary, View } from '#notes/types'

const props = defineProps<{ rows: NoteSummary[]; view: View }>()

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const trailing = computed(() => schema.visibleProperties(props.view))

async function addRow() {
  const note = await notes.create({})
  router.push(`/notes/n/${note.id}`)
}
</script>

<template>
  <div class="qn-list">
    <button
      v-for="note in rows"
      :key="note.id"
      class="qn-list__row"
      @click="router.push(`/notes/n/${note.id}`)"
    >
      <NotesNoteIcon class="qn-list__icon" :icon="note.icon" :size="14" />
      <span class="qn-list__title">{{ note.title || 'Untitled' }}</span>
      <span class="qn-list__props">
        <NotesViewsPropertyCell
          v-for="property in trailing"
          :key="property.id"
          :note-id="note.id"
          :property="property"
          :value="note.properties[property.id]"
          readonly
          hide-empty
        />
      </span>
    </button>

    <button class="qn-list__new" @click="addRow">+ New note</button>
  </div>
</template>

<style scoped>
.qn-list {
  height: 100%;
  min-height: 0;
  overflow-y: auto;
  padding-bottom: 12px;
}

.qn-list__row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 9px 10px;
  border: none;
  border-bottom: 1px solid var(--qn-border-subtle);
  background: transparent;
  color: var(--qn-text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qn-list__row:hover {
  background: var(--qn-bg-hover);
}

.qn-list__icon {
  flex-shrink: 0;
  width: 16px;
  color: var(--qn-text-muted);
  text-align: center;
}

.qn-list__title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-list__props {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.qn-list__new {
  width: 100%;
  padding: 10px;
  border: none;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qn-list__new:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>
