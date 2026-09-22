<script setup lang="ts">
definePageMeta({ layout: 'notes' })

/**
 * Suite notes inside QuantNotes (PLAN-V2 E3) — the rich-editing surface over
 * the suite's **shared** notes: the same `notes/sections` document the shell
 * drawer and the QuantHUD overlay render. Edit here, and both show it on the
 * next bus tick.
 *
 * Deliberately NOT QuantNotes' own pages: those are workspace-scoped rich
 * documents with trees and backlinks, a different thing with a different
 * model. This page is where a shared quick note gets room to breathe; the
 * module's pages stay the module's. (This resolves the "zen rewired onto the
 * shared store" line in PLAN-V2 E3 — rewiring the page model onto flat quick
 * notes would have destroyed information for no gain, so the shared store
 * gets a first-class surface inside the module instead.)
 */
import { dataplaneId, notesStore, type NoteSection } from '@quantsuite/core'

const sections = ref<NoteSection[]>([])
const selected = ref<{ sectionId: string; noteId: string } | null>(null)
const loaded = ref(false)

const selectedNote = computed(() => {
  if (!selected.value) return null
  return (
    sections.value
      .find((s) => s.id === selected.value!.sectionId)
      ?.notes.find((n) => n.id === selected.value!.noteId) ?? null
  )
})

const noteCount = computed(() => sections.value.reduce((n, s) => n + s.notes.length, 0))

let saveTimer: ReturnType<typeof setTimeout> | null = null
function persist() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
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

let off: (() => void) | undefined

onMounted(async () => {
  sections.value = (await notesStore.load().catch(() => null)) ?? []
  loaded.value = true
  off = notesStore.onChange((next) => {
    if (JSON.stringify(next) !== JSON.stringify(sections.value)) sections.value = next
  })
})
onUnmounted(() => off?.())
</script>

<template>
  <div class="qn-suite">
    <aside class="qn-suite-list">
      <header class="qn-suite-list-head">
        <div>
          <h2>Suite notes</h2>
          <p>{{ noteCount }} shared with the drawer &amp; HUD</p>
        </div>
        <button class="qn-suite-add" title="Add section" @click="addSection">+</button>
      </header>

      <div v-for="s in sections" :key="s.id" class="qn-suite-section">
        <div class="qn-suite-section-head">
          <input v-model="s.name" class="qn-suite-section-name" @change="persist" />
          <button class="qn-suite-mini" title="Add note" @click="addNote(s.id)">+</button>
          <button class="qn-suite-mini" title="Delete section" @click="deleteSection(s.id)">&times;</button>
        </div>
        <button
          v-for="n in s.notes"
          :key="n.id"
          class="qn-suite-note"
          :class="{ 'is-active': selected?.noteId === n.id }"
          @click="selected = { sectionId: s.id, noteId: n.id }"
        >
          {{ n.title || 'Untitled' }}
        </button>
      </div>

      <p v-if="loaded && !sections.length" class="qn-suite-hint">
        Nothing shared yet — add a section, or capture from the drawer or the HUD overlay.
      </p>
    </aside>

    <main class="qn-suite-editor">
      <template v-if="selectedNote">
        <input
          class="qn-suite-title"
          :value="selectedNote.title"
          placeholder="Title"
          @input="selectedNote!.title = ($event.target as HTMLInputElement).value; persist()"
        />
        <textarea
          class="qn-suite-content"
          :value="selectedNote.content"
          placeholder="Write — every window sees this note."
          @input="selectedNote!.content = ($event.target as HTMLTextAreaElement).value; persist()"
        />
        <footer class="qn-suite-foot">
          <span>Shared across the suite — drawer, HUD overlay and here are one note.</span>
          <button @click="deleteNote(selected!.sectionId, selected!.noteId)">Delete note</button>
        </footer>
      </template>
      <div v-else class="qn-suite-empty">
        <p>Select a shared note to edit it with room to breathe.</p>
        <span>QuantNotes' own pages stay under Pages — this surface is the suite-wide quick notes.</span>
      </div>
    </main>
  </div>
</template>

<style scoped>
.qn-suite {
  display: flex;
  height: 100%;
  min-height: 0;
  background: var(--qn-bg);
  color: var(--qn-text);
}

.qn-suite-list {
  width: 280px;
  flex-shrink: 0;
  overflow-y: auto;
  border-right: 1px solid var(--qn-border);
  background: var(--qn-bg-sidebar);
  padding: 14px 12px;
}

.qn-suite-list-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 14px;
}
.qn-suite-list-head h2 {
  margin: 0;
  font: 600 13px/1.3 inherit;
  color: var(--qn-text);
}
.qn-suite-list-head p {
  margin: 2px 0 0;
  font-size: 11px;
  color: var(--qn-text-muted);
}
.qn-suite-add {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: 1px solid var(--qn-border);
  border-radius: 7px;
  background: var(--qn-bg-card);
  color: var(--qn-text-muted);
  font-size: 14px;
  cursor: pointer;
}
.qn-suite-add:hover {
  color: var(--qn-text);
  border-color: var(--qn-accent);
}

.qn-suite-section {
  margin-bottom: 12px;
}
.qn-suite-section-head {
  display: flex;
  align-items: center;
  gap: 2px;
}
.qn-suite-section-name {
  flex: 1;
  min-width: 0;
  border: none;
  background: none;
  color: var(--qn-text-muted);
  font: 600 10.5px/1.8 inherit;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}
.qn-suite-section-name:focus {
  outline: none;
  color: var(--qn-text);
}
.qn-suite-mini {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--qn-text-muted);
  font-size: 12px;
  cursor: pointer;
  opacity: 0;
}
.qn-suite-section-head:hover .qn-suite-mini {
  opacity: 1;
}
.qn-suite-mini:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-suite-note {
  display: block;
  width: 100%;
  padding: 6px 9px;
  border: none;
  border-radius: 7px;
  background: none;
  color: var(--qn-text-secondary);
  font-size: 12.5px;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}
.qn-suite-note:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
.qn-suite-note.is-active {
  background: var(--qn-bg-card);
  color: var(--qn-text);
}

.qn-suite-hint {
  margin: 10px 2px;
  color: var(--qn-text-muted);
  font-size: 11.5px;
}

.qn-suite-editor {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: clamp(18px, 4vh, 40px) clamp(20px, 6vw, 96px);
  gap: 12px;
}

.qn-suite-title {
  border: none;
  background: none;
  color: var(--qn-text);
  font: 600 22px/1.25 inherit;
  letter-spacing: -0.01em;
}
.qn-suite-title:focus {
  outline: none;
}

.qn-suite-content {
  flex: 1;
  min-height: 0;
  resize: none;
  border: none;
  background: none;
  color: var(--qn-text-secondary);
  font: 400 14px/1.75 inherit;
}
.qn-suite-content:focus {
  outline: none;
}

.qn-suite-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--qn-text-muted);
  font-size: 11px;
}
.qn-suite-foot button {
  padding: 4px 10px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: none;
  color: var(--qn-text-muted);
  font-size: 11px;
  cursor: pointer;
}
.qn-suite-foot button:hover {
  color: var(--qn-error, #ff4757);
  border-color: color-mix(in srgb, var(--qn-error, #ff4757) 45%, var(--qn-border));
}

.qn-suite-empty {
  margin: auto;
  max-width: 46ch;
  text-align: center;
}
.qn-suite-empty p {
  margin: 0 0 6px;
  color: var(--qn-text-secondary);
  font-size: 13px;
}
.qn-suite-empty span {
  color: var(--qn-text-muted);
  font-size: 11.5px;
}
</style>
