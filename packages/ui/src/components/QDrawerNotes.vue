<script setup lang="ts">
/**
 * The drawer's Notes tab (PLAN-V2 E3) — a live view of the suite's shared
 * notes (`notes/sections` in core.db). The QuantHUD overlay renders the same
 * document; edits here appear there on the next bus tick and vice versa.
 * This surface is browse + capture; heavier editing belongs to the modules.
 */
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { dataplaneId, notesStore, type NoteSection } from '@quantsuite/core'

const props = defineProps<{
  /** Bumped by the drawer for quick capture: create a note, focus the title. */
  capture?: number
}>()

const sections = ref<NoteSection[]>([])
const titleInput = ref<HTMLInputElement | null>(null)
const selected = ref<{ sectionId: string; noteId: string } | null>(null)
const loaded = ref(false)

const selectedNote = () => {
  if (!selected.value) return null
  return (
    sections.value
      .find((s) => s.id === selected.value!.sectionId)
      ?.notes.find((n) => n.id === selected.value!.noteId) ?? null
  )
}

let saveTimer: ReturnType<typeof setTimeout> | null = null
function persist() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    void notesStore.save(JSON.parse(JSON.stringify(sections.value)))
  }, 250)
}

function addSection() {
  sections.value.push({ id: dataplaneId(), name: 'New section', collapsed: false, notes: [] })
  persist()
}
function addNote(sectionId: string) {
  const s = sections.value.find((s) => s.id === sectionId)
  if (!s) return
  const note = { id: dataplaneId(), title: 'New note', content: '' }
  s.notes.push(note)
  selected.value = { sectionId, noteId: note.id }
  persist()
}
function deleteNote(sectionId: string, noteId: string) {
  const s = sections.value.find((s) => s.id === sectionId)
  if (!s) return
  s.notes = s.notes.filter((n) => n.id !== noteId)
  if (selected.value?.noteId === noteId) selected.value = null
  persist()
}
function deleteSection(sectionId: string) {
  sections.value = sections.value.filter((s) => s.id !== sectionId)
  if (selected.value?.sectionId === sectionId) selected.value = null
  persist()
}

/** Quick capture (E5): a fresh note in the first section, title focused. */
async function captureNote() {
  if (!sections.value.length) addSection()
  const s = sections.value[0]!
  const note = { id: dataplaneId(), title: '', content: '' }
  s.notes.push(note)
  selected.value = { sectionId: s.id, noteId: note.id }
  persist()
  await new Promise((r) => setTimeout(r, 0))
  titleInput.value?.focus()
}

watch(
  () => props.capture,
  (tick) => {
    if (tick) void captureNote()
  }
)

let off: (() => void) | undefined
let disposed = false

onMounted(async () => {
  sections.value = (await notesStore.load().catch(() => null)) ?? []
  loaded.value = true
  // A tab switch unmounts this inside the load above — the unmount hook then
  // ran with `off` still undefined and the subscription outlived the tab.
  const stop = notesStore.onChange((next) => {
    // The store echoes this window's own save too. While a save is still
    // pending, local state is the newer one — adopting the echo would drop
    // every keystroke typed since it went out, and then persist the rollback.
    if (saveTimer) return
    if (JSON.stringify(next) !== JSON.stringify(sections.value)) sections.value = next
  })
  if (disposed) stop()
  else off = stop
  // Opened *for* capture: the mount races the drawer's tick bump.
  if (props.capture) void captureNote()
})
onUnmounted(() => {
  disposed = true
  off?.()
})
</script>

<template>
  <div class="dnw">
    <aside class="dnw-list">
      <div v-for="s in sections" :key="s.id" class="dnw-section">
        <div class="dnw-section-head">
          <input v-model="s.name" class="dnw-section-name" @change="persist" />
          <button class="dnw-mini" title="Add note" @click="addNote(s.id)">+</button>
          <button class="dnw-mini" title="Delete section" @click="deleteSection(s.id)">&times;</button>
        </div>
        <button
          v-for="n in s.notes"
          :key="n.id"
          class="dnw-note"
          :class="{ 'is-active': selected?.noteId === n.id }"
          @click="selected = { sectionId: s.id, noteId: n.id }"
        >
          {{ n.title || 'Untitled' }}
        </button>
      </div>

      <button class="dnw-add" @click="addSection">+ Section</button>
    </aside>

    <div class="dnw-editor">
      <template v-if="selectedNote()">
        <input
          ref="titleInput"
          class="dnw-title"
          :value="selectedNote()!.title"
          placeholder="Title"
          @input="selectedNote()!.title = ($event.target as HTMLInputElement).value; persist()"
        />
        <textarea
          class="dnw-content"
          :value="selectedNote()!.content"
          placeholder="Write…"
          @input="selectedNote()!.content = ($event.target as HTMLTextAreaElement).value; persist()"
        />
        <button class="dnw-delete" @click="deleteNote(selected!.sectionId, selected!.noteId)">Delete note</button>
      </template>
      <p v-else-if="loaded && !sections.length" class="dnw-empty">
        No notes yet. They are shared with the QuantHUD overlay — create one on either side.
      </p>
      <p v-else class="dnw-empty">Select a note, or add one.</p>
    </div>
  </div>
</template>

<style scoped>
.dnw {
  display: flex;
  height: 100%;
  min-height: 0;
}

.dnw-list {
  width: 240px;
  flex-shrink: 0;
  overflow-y: auto;
  border-right: 1px solid var(--qss-border-subtle);
  padding: 8px;
}

.dnw-section {
  margin-bottom: 10px;
}
.dnw-section-head {
  display: flex;
  align-items: center;
  gap: 2px;
}
.dnw-section-name {
  flex: 1;
  min-width: 0;
  border: none;
  background: none;
  color: var(--qss-text-secondary);
  font: 600 11px/1.6 var(--qss-font-sans);
  letter-spacing: 0.03em;
  text-transform: uppercase;
}
.dnw-section-name:focus {
  outline: none;
  color: var(--qss-text);
}
.dnw-mini {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--qss-text-muted);
  font-size: 12px;
  cursor: pointer;
  opacity: 0;
}
.dnw-section-head:hover .dnw-mini {
  opacity: 1;
}
.dnw-mini:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.dnw-note {
  display: block;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--qss-text-secondary);
  font: 400 12px/1.3 var(--qss-font-sans);
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}
.dnw-note:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.dnw-note.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

.dnw-add {
  width: 100%;
  padding: 6px;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  background: none;
  color: var(--qss-text-muted);
  font: 500 11px/1 var(--qss-font-sans);
  cursor: pointer;
}
.dnw-add:hover {
  color: var(--qss-text);
  background: var(--qss-bg-hover);
}

.dnw-editor {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: 10px 14px;
  gap: 8px;
}
.dnw-title {
  border: none;
  background: none;
  color: var(--qss-text);
  font: 600 13.5px/1.3 var(--qss-font-sans);
}
.dnw-title:focus {
  outline: none;
}
.dnw-content {
  flex: 1;
  min-height: 0;
  resize: none;
  border: none;
  background: none;
  color: var(--qss-text-secondary);
  font: 400 12.5px/1.6 var(--qss-font-sans);
}
.dnw-content:focus {
  outline: none;
}
.dnw-delete {
  align-self: flex-start;
  padding: 4px 9px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: none;
  color: var(--qss-text-muted);
  font: 500 11px/1 var(--qss-font-sans);
  cursor: pointer;
}
.dnw-delete:hover {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 45%, var(--qss-border));
}

.dnw-empty {
  margin: auto;
  color: var(--qss-text-muted);
  font-size: 12px;
  max-width: 48ch;
  text-align: center;
}
</style>
