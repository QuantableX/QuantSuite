<script setup lang="ts">
/**
 * The gallery view — cards in a grid, cover image first.
 *
 * A note with no cover gets a plate whose tone comes from its id, so a wall of
 * coverless notes still reads as distinct cards. Grey tones only: the suite is
 * monochrome, and colour is reserved for what the user picks.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { NoteSummary, View } from '#notes/types'

const props = defineProps<{ rows: NoteSummary[]; view: View }>()

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const chips = computed(() => schema.visibleProperties(props.view))

const minWidth = computed(() => {
  switch (props.view.config.cardSize) {
    case 'small':
      return '150px'
    case 'large':
      return '300px'
    default:
      return '220px'
  }
})

/** A stable tone per note — same note, same plate, across reloads. */
function plate(note: NoteSummary): string {
  let hash = 0
  for (const ch of note.id) hash = (hash * 31 + ch.charCodeAt(0)) % 360
  return `hsl(0 0% ${16 + (hash % 9)}%)`
}

async function addCard() {
  const note = await notes.create({})
  router.push(`/notes/n/${note.id}`)
}
</script>

<template>
  <div class="qn-gallery">
    <div class="qn-gallery__grid" :style="{ '--qn-card-min': minWidth }">
      <article
        v-for="note in rows"
        :key="note.id"
        class="qn-gallery__card"
        @click="router.push(`/notes/n/${note.id}`)"
      >
        <div class="qn-gallery__cover" :style="note.cover ? { backgroundImage: `url(${note.cover})` } : { background: plate(note) }">
          <NotesNoteIcon v-if="note.icon" class="qn-gallery__icon" :icon="note.icon" :size="28" :stroke="1.5" />
        </div>
        <div class="qn-gallery__body">
          <h3>{{ note.title || 'Untitled' }}</h3>
          <div v-if="chips.length" class="qn-gallery__chips">
            <NotesViewsPropertyCell
              v-for="property in chips"
              :key="property.id"
              :note-id="note.id"
              :property="property"
              :value="note.properties[property.id]"
              readonly
              hide-empty
            />
          </div>
        </div>
      </article>

      <button class="qn-gallery__add" @click="addCard">+ New note</button>
    </div>
  </div>
</template>

<style scoped>
.qn-gallery {
  height: 100%;
  min-height: 0;
  overflow-y: auto;
}

.qn-gallery__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(var(--qn-card-min), 1fr));
  gap: var(--qss-gap);
}

.qn-gallery__card {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--qn-border-subtle);
  border-radius: var(--qss-panel-radius);
  background: var(--qn-bg-sidebar);
  overflow: hidden;
  cursor: pointer;
  transition: border-color var(--qss-dur-fast, 120ms) ease;
}

.qn-gallery__card:hover {
  border-color: var(--qn-accent);
}

.qn-gallery__cover {
  flex-shrink: 0;
  height: 72px;
  display: grid;
  place-items: center;
  background-size: cover;
  background-position: center;
}

/* The plate behind it is dark in both themes (see plate()), so the symbol is
   light in both — not --qn-text, which inverts. */
.qn-gallery__icon {
  color: rgba(255, 255, 255, 0.92);
}

.qn-gallery__body {
  padding: var(--qss-gap-tight) 10px 10px;
}

.qn-gallery__body h3 {
  margin: 0;
  color: var(--qn-text);
  font-size: 13px;
  font-weight: 600;
  line-height: 1.4;
}

.qn-gallery__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 6px;
  margin-top: 6px;
  font-variant-numeric: tabular-nums;
}

.qn-gallery__add {
  min-height: 112px;
  border: 1px dashed var(--qn-border);
  border-radius: var(--qss-panel-radius);
  background: transparent;
  color: var(--qn-text-muted);
  font-size: var(--qss-text-label);
  cursor: pointer;
}

.qn-gallery__add:hover {
  border-color: var(--qn-accent);
  color: var(--qn-text);
}
</style>
