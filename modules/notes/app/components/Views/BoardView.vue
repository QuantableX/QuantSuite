<script setup lang="ts">
/**
 * The board view — the same notes, grouped into columns by a select property.
 *
 * The columns ARE the property's options: dropping a card into a column sets
 * that property, which is the whole point of a board being a view rather than
 * a second data model. No `groupBy` means one "All notes" column and a hint in
 * the toolbar, not an error.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import { groupByProperty } from '#notes/composables/useViewRows'
import type { NoteSummary, View } from '#notes/types'

const props = defineProps<{ rows: NoteSummary[]; view: View }>()

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const groupProperty = computed(() =>
  props.view.config.groupBy ? schema.propertyById.get(props.view.config.groupBy) : undefined,
)
const groups = computed(() => groupByProperty(props.rows, groupProperty.value))

/** Card chips: the visible properties minus the one that defines the column. */
const cardProperties = computed(() =>
  schema.visibleProperties(props.view).filter((p) => p.id !== props.view.config.groupBy),
)

const draggingId = ref<string | null>(null)
const overKey = ref<string | null | undefined>(undefined)

function onDragStart(note: NoteSummary) {
  draggingId.value = note.id
}

function onDragEnd() {
  draggingId.value = null
  overKey.value = undefined
}

async function onDrop(key: string | null) {
  const id = draggingId.value
  onDragEnd()
  if (!id || !groupProperty.value) return
  await notes.setProperty(id, groupProperty.value.id, key)
}

async function addTo(key: string | null) {
  const properties = groupProperty.value && key ? { [groupProperty.value.id]: key } : undefined
  const note = await notes.create({ properties })
  router.push(`/notes/n/${note.id}`)
}
</script>

<template>
  <div class="qn-board">
    <div class="qn-board__scroll">
      <section
        v-for="group in groups"
        :key="group.key ?? '__none__'"
        class="qn-board__col"
        :class="{ 'is-over': overKey === group.key && draggingId }"
        @dragover.prevent="overKey = group.key"
        @dragleave="overKey === group.key ? (overKey = undefined) : null"
        @drop.prevent="onDrop(group.key)"
      >
        <header class="qn-board__head">
          <span class="qn-tag" :data-color="group.color">{{ group.label }}</span>
          <span class="qn-board__count">{{ group.notes.length }}</span>
        </header>

        <div class="qn-board__cards">
          <article
            v-for="note in group.notes"
            :key="note.id"
            class="qn-board__card"
            :class="{ 'is-dragging': draggingId === note.id }"
            draggable="true"
            @dragstart="onDragStart(note)"
            @dragend="onDragEnd"
            @click="router.push(`/notes/n/${note.id}`)"
          >
            <div class="qn-board__card-title">
              <NotesNoteIcon v-if="note.icon" :icon="note.icon" :size="14" />
              <span>{{ note.title || 'Untitled' }}</span>
            </div>
            <div v-if="cardProperties.length" class="qn-board__card-props">
              <NotesViewsPropertyCell
                v-for="property in cardProperties"
                :key="property.id"
                :note-id="note.id"
                :property="property"
                :value="note.properties[property.id]"
                readonly
                hide-empty
              />
            </div>
          </article>

          <button class="qn-board__add" @click="addTo(group.key)">+ New</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qn-board {
  height: 100%;
  min-height: 0;
}

.qn-board__scroll {
  height: 100%;
  display: flex;
  gap: 12px;
  padding: 4px 2px 12px;
  overflow-x: auto;
  overflow-y: hidden;
}

.qn-board__col {
  flex: 0 0 272px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border: 1px solid transparent;
  border-radius: 10px;
  transition: border-color var(--qss-dur-fast, 120ms) ease, background var(--qss-dur-fast, 120ms) ease;
}

.qn-board__col.is-over {
  border-color: var(--qn-accent);
  background: color-mix(in srgb, var(--qn-accent) 8%, transparent);
}

.qn-board__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
}

.qn-board__count {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-board__cards {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 0 8px 8px;
  overflow-y: auto;
}

.qn-board__card {
  padding: 9px 11px;
  border: 1px solid var(--qn-border);
  border-radius: 8px;
  background: var(--qn-bg-card);
  cursor: pointer;
  transition: border-color var(--qss-dur-fast, 120ms) ease, transform var(--qss-dur-fast, 120ms) ease;
}

.qn-board__card:hover {
  border-color: var(--qn-accent);
}

.qn-board__card.is-dragging {
  opacity: 0.4;
}

.qn-board__card-title {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--qn-text);
  font-size: 13px;
  font-weight: 500;
  line-height: 1.4;
}

.qn-board__card-props {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 7px;
}

.qn-board__add {
  padding: 7px 9px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.qn-board__add:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>
