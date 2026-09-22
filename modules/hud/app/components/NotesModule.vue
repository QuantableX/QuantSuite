<template>
  <div ref="moduleRef" class="notes-module" :class="{ 'notes-horizontal': horizontal }">
    <!-- Add Section Button -->
    <button v-if="!horizontal || !sections.length" class="btn btn-primary add-section-btn" @click="createSection()">
      + New Section
    </button>

    <!-- Sections (draggable) -->
    <div
      v-for="(section, sIdx) in sections"
      :key="section.id"
      v-show="!horizontal || section.id === activeSection?.id"
      class="section"
      :class="{
        'section-drop-above':
          dropTargetSectionIdx === sIdx &&
          dragType === 'section' &&
          sectionDropPosition === 'above',
        'section-drop-below':
          dropTargetSectionIdx === sIdx &&
          dragType === 'section' &&
          sectionDropPosition === 'below',
        dragging: draggingSectionIdx === sIdx,
      }"
    >
      <!-- Section Header -->
      <div class="section-header">
        <div v-if="horizontal" class="notebook-heading"><span class="hud-eyebrow">NOTEBOOK</span><h2>Your notes</h2></div>
        <!-- Drag handle -->
        <span
          v-if="!horizontal"
          class="drag-handle"
          draggable="true"
          @dragstart="onSectionDragStart(sIdx, $event)"
          @dragend="onDragEnd"
          title="Drag to reorder"
          >⠿</span
        >

        <button
          v-if="!horizontal"
          class="collapse-btn"
          @click="toggleSection(section.id)"
          :title="section.collapsed ? 'Expand' : 'Collapse'"
        >
          <span class="collapse-icon" :class="{ collapsed: section.collapsed }"
            >▼</span
          >
        </button>

        <!-- Section Name (inline rename) -->
        <input
          v-if="editingSectionId === section.id"
          ref="sectionInputRef"
          class="section-name-input"
          :value="section.name"
          @blur="finishRenameSection(section.id, $event)"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
          @keydown.escape="editingSectionId = null"
        />
        <span
          v-else-if="!horizontal"
          class="section-name"
          @dblclick="startRenameSection(section.id)"
        >
          {{ section.name }}
        </span>
        <select v-else v-model="selectedSectionId" class="notebook-select" aria-label="Notes section" @change="resetNoteScroll">
          <option v-for="item in sections" :key="item.id" :value="item.id">{{ item.name }} · {{ item.notes.length }}</option>
        </select>

        <!-- Section Controls -->
        <div class="section-controls">
          <div v-if="horizontal" class="notebook-management">
            <button class="ctrl-btn" title="Rename section" @click="startRenameSection(section.id)"><Pencil :size="13" /></button>
            <button class="notebook-new" title="New section" aria-label="New section" @click="createSection()"><Plus :size="13" /></button>
          </div>
          <template v-if="horizontal">
            <button class="ctrl-btn" title="Move section left" :disabled="sIdx === 0" @click="reorderSection(sIdx, sIdx - 1)"><ChevronLeft :size="13" /></button>
            <button class="ctrl-btn" title="Move section right" :disabled="sIdx === sections.length - 1" @click="reorderSection(sIdx, sIdx + 1)"><ChevronRight :size="13" /></button>
          </template>
          <button
            class="ctrl-btn ctrl-add"
            @click="createNote(section.id)"
            title="Add note"
          >
            {{ horizontal ? '+ New note' : '+' }}
          </button>
          <button
            class="ctrl-btn"
            @click="duplicateSection(section.id)"
            title="Duplicate section"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path
                d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
              />
            </svg>
          </button>
          <button
            class="ctrl-btn ctrl-del"
            @click="confirmDeleteSection(section.id)"
            title="Delete section"
          >
            ✕
          </button>
        </div>
      </div>

      <!-- Notes List (collapsible, drop zone) -->
      <div
        v-if="horizontal || !section.collapsed"
        class="notes-list"
        :class="{
          'note-drop-zone': dragType === 'note' && section.notes.length === 0,
        }"
        @dragover.prevent.stop="
          onNoteListDragOver(section.id, section.notes.length, $event)
        "
        @drop.prevent.stop="
          onNoteDrop(section.id, dropTargetNoteIdx ?? section.notes.length)
        "
      >
        <div
          v-for="(note, nIdx) in section.notes"
          :key="note.id"
          :data-note-id="note.id"
          class="note-item"
          :class="{
            'note-drop-above':
              dropTargetNoteIdx === nIdx &&
              dropTargetSectionId === section.id &&
              dropPosition === 'above',
            'note-drop-below':
              dropTargetNoteIdx === nIdx &&
              dropTargetSectionId === section.id &&
              dropPosition === 'below',
            'note-dragging':
              draggingNote?.noteIdx === nIdx &&
              draggingNote?.sectionId === section.id,
          }"
          @dragover.prevent.stop="onNoteDragOver(section.id, nIdx, $event)"
          @drop.prevent.stop="onNoteDrop(section.id, nIdx)"
        >
          <!-- Note Header -->
          <div class="note-header">
            <span v-if="horizontal" class="note-number">{{ String(nIdx + 1).padStart(2, '0') }}</span>
            <span
              class="drag-handle note-handle"
              draggable="true"
              @dragstart.stop="onNoteDragStart(section.id, nIdx, $event)"
              @dragend="onDragEnd"
              title="Drag to reorder"
              >⠿</span
            >
            <input
              v-if="editingNoteId === note.id"
              ref="noteInputRef"
              class="note-title-input"
              :value="note.title"
              @blur="finishRenameNote(section.id, note.id, $event)"
              @keydown.enter="($event.target as HTMLInputElement).blur()"
              @keydown.escape="editingNoteId = null"
            />
            <button
              v-else
              class="note-title"
              title="Rename note"
              @click="startRenameNote(note.id)"
            >
              {{ note.title }}
            </button>

            <div class="note-controls">
              <select v-if="horizontal && sections.length > 1" class="note-move" value=""
                aria-label="Move note to section" title="Move note to section"
                @change="moveToSection(section.id, nIdx, $event)">
                <option value="" disabled>↗</option>
                <option v-for="target in sections.filter(s => s.id !== section.id)" :key="target.id" :value="target.id">{{ target.name }}</option>
              </select>
              <button
                class="ctrl-btn"
                @click="duplicateNote(section.id, note.id)"
                title="Duplicate note"
              >
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                  <path
                    d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
                  />
                </svg>
              </button>
              <button
                class="ctrl-btn"
                @click="copyNoteContent(note)"
                title="Copy note content"
              >
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path
                    d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
                  />
                  <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
                </svg>
              </button>
              <button
                class="ctrl-btn ctrl-del"
                @click="deleteNote(section.id, note.id)"
                title="Delete note"
              >
                ✕
              </button>
            </div>
          </div>

          <!-- Note Content (markdown textarea) -->
          <textarea
            class="note-content"
            :aria-label="note.title || 'Note content'"
            :value="note.content"
            @input="handleContentChange(section.id, note.id, $event)"
            placeholder="Write your note..."
            rows="3"
          />
        </div>

        <button v-if="horizontal" class="note-create" @click="createNote(section.id)"><Plus :size="22" :stroke-width="1.5" /><strong>{{ section.notes.length ? 'Another idea?' : 'A fresh page' }}</strong><span>Add a note</span></button>
        <p v-else-if="section.notes.length === 0" class="empty-hint">{{ dragType === "note" ? "Drop note here" : "No notes yet" }}</p>
      </div>
    </div>

    <p v-if="sections.length === 0" class="empty-hint" style="margin-top: 12px">
      Create a section to start taking notes
    </p>
  </div>
</template>

<script setup lang="ts">
import { ChevronLeft, ChevronRight, Pencil, Plus } from 'lucide-vue-next'
import { useNotes } from '#hud/composables/useNotes'
const props = defineProps<{ horizontal?: boolean }>()
const {
  sections,
  addSection,
  renameSection,
  deleteSection,
  toggleSection,
  addNote,
  renameNote,
  updateNoteContent,
  deleteNote,
  reorderSection,
  reorderNote,
  moveNoteToSectionAt,
  duplicateSection,
  duplicateNote,
} = useNotes();

const selectedSectionId = ref('');
const activeSection = computed(() => sections.value.find(s => s.id === selectedSectionId.value) ?? sections.value[0]);
watch(activeSection, section => { selectedSectionId.value = section?.id ?? ''; });

function resetNoteScroll() {
  moduleRef.value?.closest('.scroll-content')?.scrollTo({ left: 0, behavior: 'instant' });
}
async function createSection() {
  addSection();
  selectedSectionId.value = sections.value.at(-1)?.id ?? '';
  await nextTick();
  resetNoteScroll();
}
async function createNote(sectionId: string) {
  addNote(sectionId);
  if (!props.horizontal) return;
  await nextTick();
  const id = sections.value.find(s => s.id === sectionId)?.notes.at(-1)?.id;
  const card = Array.from(moduleRef.value?.querySelectorAll<HTMLElement>('[data-note-id]') ?? [])
    .find(el => el.dataset.noteId === id);
  card?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  card?.querySelector('textarea')?.focus({ preventScroll: true });
}
function moveToSection(sectionId: string, index: number, event: Event) {
  const targetId = (event.target as HTMLSelectElement).value;
  const target = sections.value.find(s => s.id === targetId);
  if (target) moveNoteToSectionAt(sectionId, index, targetId, target.notes.length);
}

const editingSectionId = ref<string | null>(null);
const editingNoteId = ref<string | null>(null);
const sectionInputRef = ref<HTMLInputElement[] | null>(null);
const noteInputRef = ref<HTMLInputElement[] | null>(null);

const moduleRef = ref<HTMLElement | null>(null);

const pendingContent = new Map<string, { timer: ReturnType<typeof setTimeout>; save: () => void }>();

onUnmounted(() => {
  for (const pending of pendingContent.values()) { clearTimeout(pending.timer); pending.save(); }
  pendingContent.clear();
  document.removeEventListener("dragover", onDocSectionDragOver);
  document.removeEventListener("drop", onDocSectionDrop);
});

// --- Drag state ---
const dragType = ref<"section" | "note" | null>(null);
const draggingSectionIdx = ref<number | null>(null);
const draggingNote = ref<{ sectionId: string; noteIdx: number } | null>(null);
const dropTargetSectionId = ref<string | null>(null);
const dropTargetNoteIdx = ref<number | null>(null);
const dropPosition = ref<"above" | "below" | null>(null);
const sectionDropPosition = ref<"above" | "below" | null>(null);
const dropTargetSectionIdx = ref<number | null>(null);

// --- Section drag handlers ---
function onSectionDragStart(idx: number, e: DragEvent) {
  dragType.value = "section";
  draggingSectionIdx.value = idx;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `section:${idx}`);
  }
  // Attach document-level listeners so the ENTIRE window is a valid drop zone
  document.addEventListener("dragover", onDocSectionDragOver);
  document.addEventListener("drop", onDocSectionDrop);
}

// --- Document-level drag handlers (entire viewport = drop zone) ---
function onDocSectionDragOver(e: DragEvent) {
  e.preventDefault();
  if (dragType.value !== "section" || !moduleRef.value) return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";

  const sectionEls = Array.from(
    moduleRef.value.querySelectorAll(":scope > .section"),
  );
  if (sectionEls.length === 0) return;

  const cursorY = e.clientY;

  // Build slot boundaries: N sections → N+1 slots
  // Slot 0 = before first, Slot N = after last
  // Between sections: midpoint of the gap
  const slotYs: number[] = [];

  const firstRect = sectionEls[0].getBoundingClientRect();
  slotYs.push(firstRect.top);

  for (let i = 1; i < sectionEls.length; i++) {
    const prevBottom = sectionEls[i - 1].getBoundingClientRect().bottom;
    const currTop = sectionEls[i].getBoundingClientRect().top;
    slotYs.push((prevBottom + currTop) / 2);
  }

  const lastRect = sectionEls[sectionEls.length - 1].getBoundingClientRect();
  slotYs.push(lastRect.bottom);

  // Above first section → always slot 0 (infinite upward)
  if (cursorY <= slotYs[0]) {
    dropTargetSectionIdx.value = 0;
    sectionDropPosition.value = "above";
    return;
  }
  // Below last section → always last slot (infinite downward)
  if (cursorY >= slotYs[slotYs.length - 1]) {
    dropTargetSectionIdx.value = sectionEls.length - 1;
    sectionDropPosition.value = "below";
    return;
  }

  // Find closest slot between sections
  let bestSlot = 0;
  let bestDist = Infinity;
  for (let i = 0; i <= sectionEls.length; i++) {
    const dist = Math.abs(cursorY - slotYs[i]);
    if (dist < bestDist) {
      bestDist = dist;
      bestSlot = i;
    }
  }

  if (bestSlot < sectionEls.length) {
    dropTargetSectionIdx.value = bestSlot;
    sectionDropPosition.value = "above";
  } else {
    dropTargetSectionIdx.value = sectionEls.length - 1;
    sectionDropPosition.value = "below";
  }
}

function onDocSectionDrop(e: DragEvent) {
  e.preventDefault();
  if (dragType.value !== "section" || draggingSectionIdx.value === null) {
    resetDragState();
    return;
  }
  if (
    dropTargetSectionIdx.value === null ||
    sectionDropPosition.value === null
  ) {
    resetDragState();
    return;
  }
  let finalIdx =
    sectionDropPosition.value === "below"
      ? dropTargetSectionIdx.value + 1
      : dropTargetSectionIdx.value;
  const fromIdx = draggingSectionIdx.value;
  if (fromIdx < finalIdx) finalIdx--;
  reorderSection(fromIdx, finalIdx);
  resetDragState();
}

// --- Note drag handlers ---
function onNoteDragStart(sectionId: string, noteIdx: number, e: DragEvent) {
  dragType.value = "note";
  draggingNote.value = { sectionId, noteIdx };
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `note:${sectionId}:${noteIdx}`);
  }
}

function onNoteListDragOver(
  sectionId: string,
  noteCount: number,
  e: DragEvent,
) {
  if (dragType.value !== "note") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dropTargetSectionId.value = sectionId;
  if (noteCount > 0) {
    // Point to last note with "below" so the insertion line shows
    dropTargetNoteIdx.value = noteCount - 1;
    dropPosition.value = "below";
  } else {
    dropTargetNoteIdx.value = 0;
    dropPosition.value = "above";
  }
}

function onNoteDragOver(sectionId: string, noteIdx: number, e: DragEvent) {
  if (dragType.value !== "note") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const before = props.horizontal ? e.clientX < rect.left + rect.width / 2 : e.clientY < rect.top + rect.height / 2;
  dropTargetSectionId.value = sectionId;
  dropTargetNoteIdx.value = noteIdx;
  dropPosition.value = before ? "above" : "below";
}

function onNoteDrop(toSectionId: string, toIdx: number) {
  if (dragType.value !== "note" || !draggingNote.value) {
    resetDragState();
    return;
  }
  const { sectionId: fromSectionId, noteIdx: fromIdx } = draggingNote.value;
  // Adjust target index: "below" means insert after this note
  let finalIdx = dropPosition.value === "below" ? toIdx + 1 : toIdx;

  if (fromSectionId === toSectionId) {
    // When moving within the same section, account for removal shifting indices
    if (fromIdx < finalIdx) finalIdx--;
    reorderNote(fromSectionId, fromIdx, finalIdx);
  } else {
    moveNoteToSectionAt(fromSectionId, fromIdx, toSectionId, finalIdx);
  }
  resetDragState();
}

function onDragEnd() {
  resetDragState();
}

function resetDragState() {
  // Remove document-level listeners added during section drag
  document.removeEventListener("dragover", onDocSectionDragOver);
  document.removeEventListener("drop", onDocSectionDrop);

  dragType.value = null;
  draggingSectionIdx.value = null;
  draggingNote.value = null;
  dropTargetSectionId.value = null;
  dropTargetSectionIdx.value = null;
  dropTargetNoteIdx.value = null;
  dropPosition.value = null;
  sectionDropPosition.value = null;
}

// --- Rename helpers ---
function startRenameSection(id: string) {
  editingSectionId.value = id;
  nextTick(() => {
    if (sectionInputRef.value?.[0]) {
      sectionInputRef.value[0].focus();
      sectionInputRef.value[0].select();
    }
  });
}

function finishRenameSection(id: string, e: Event) {
  const val = (e.target as HTMLInputElement).value.trim();
  if (val) renameSection(id, val);
  editingSectionId.value = null;
}

function startRenameNote(id: string) {
  editingNoteId.value = id;
  nextTick(() => {
    if (noteInputRef.value?.[0]) {
      noteInputRef.value[0].focus();
      noteInputRef.value[0].select();
    }
  });
}

function finishRenameNote(sectionId: string, noteId: string, e: Event) {
  const val = (e.target as HTMLInputElement).value.trim();
  if (val) renameNote(sectionId, noteId, val);
  editingNoteId.value = null;
}

function handleContentChange(sectionId: string, noteId: string, e: Event) {
  const val = (e.target as HTMLTextAreaElement).value;
  const previous = pendingContent.get(noteId);
  if (previous) clearTimeout(previous.timer);
  // Several notes are editable at once in the rail. Keep each draft separate
  // and update it immediately so section switches cannot restore stale text.
  const note = sections.value.find(s => s.id === sectionId)?.notes.find(n => n.id === noteId);
  if (note) note.content = val;
  const save = () => updateNoteContent(sectionId, noteId, val);
  const timer = setTimeout(() => { pendingContent.delete(noteId); save(); }, 400);
  pendingContent.set(noteId, { timer, save });
}

function confirmDeleteSection(id: string) {
  deleteSection(id);
}

async function copyNoteContent(note: { title: string; content: string }) {
  const text = note.content || note.title;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    console.warn("Failed to copy note content");
  }
}
</script>

<style scoped>
.notes-module {
  padding: 4px 0;
}

.add-section-btn {
  width: 100%;
  margin-bottom: 8px;
  font-size: 13px;
  padding: 6px 12px;
}

/* --- Drag handle --- */
.drag-handle {
  cursor: grab;
  color: var(--text-secondary);
  font-size: 14px;
  line-height: 1;
  padding: 2px 2px;
  border-radius: 3px;
  user-select: none;
  flex-shrink: 0;
  opacity: 0.5;
  transition:
    opacity 0.15s ease,
    color 0.15s ease;
}

.drag-handle:hover {
  opacity: 1;
  color: var(--accent-blue);
}

.drag-handle:active {
  cursor: grabbing;
}

.note-handle {
  font-size: 12px;
}

/* --- Section --- */
.section {
  position: relative;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  margin-bottom: 8px;
  overflow: visible;
}

.section.dragging {
  opacity: 0.4;
}

.section.section-drop-above::before {
  content: "";
  position: absolute;
  top: -5px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
  z-index: 1;
}

.section.section-drop-below::after {
  content: "";
  position: absolute;
  bottom: -5px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
  z-index: 1;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  min-height: 34px;
}

.collapse-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px 4px;
  font-size: 10px;
  line-height: 1;
  flex-shrink: 0;
}

.collapse-icon {
  display: inline-block;
  transition: transform 0.2s ease;
}

.collapse-icon.collapsed {
  transform: rotate(-90deg);
}

.section-name {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  cursor: default;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.section-name-input {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  background: var(--input-bg);
  border: 1px solid var(--accent-blue);
  border-radius: 4px;
  color: var(--text-primary);
  padding: 2px 6px;
  min-width: 0;
}

.section-name-input:focus {
  outline: none;
}

.section-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

/* --- Shared control buttons --- */
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 10px;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  transition: all 0.12s ease;
}

.ctrl-btn:hover:not(:disabled) {
  background: var(--btn-primary);
  color: var(--text-primary);
}

.ctrl-add {
  font-size: 14px;
  font-weight: 700;
  color: var(--accent-green-dim);
}

.ctrl-add:hover {
  color: var(--accent-green);
}

.ctrl-del:hover {
  color: var(--accent-red) !important;
  background: rgba(255, 68, 102, 0.15) !important;
}

/* --- Notes list --- */
.notes-list {
  padding: 6px 6px 12px;
  min-height: 20px;
  transition: background 0.2s ease;
}

.notes-list.note-drop-zone {
  background: rgba(74, 144, 217, 0.06);
  border-radius: 0 0 8px 8px;
}

.note-item {
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 6px 8px;
  margin-bottom: 6px;
  transition:
    border-color 0.15s ease,
    opacity 0.15s ease,
    box-shadow 0.15s ease;
}

.note-item:last-child {
  margin-bottom: 0;
}

.note-item.note-dragging {
  opacity: 0.35;
}

.note-item.note-drop-above {
  position: relative;
}

.note-item.note-drop-above::before {
  content: "";
  position: absolute;
  top: -4px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
}

.note-item.note-drop-below {
  position: relative;
}

.note-item.note-drop-below::after {
  content: "";
  position: absolute;
  bottom: -4px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
}

.note-header {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: 4px;
}

.note-title {
  border: 0;
  padding: 0;
  background: none;
  color: inherit;
  font-family: inherit;
  text-align: left;
  cursor: text;
  flex: 1;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  cursor: default;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.note-title-input {
  flex: 1;
  font-size: 12px;
  font-weight: 600;
  background: var(--bg-card);
  border: 1px solid var(--accent-blue);
  border-radius: 4px;
  color: var(--text-primary);
  padding: 1px 6px;
  min-width: 0;
}

.note-title-input:focus {
  outline: none;
}

.note-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

/* --- Note content textarea --- */
.note-content {
  width: 100%;
  min-height: 48px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 12px;
  font-family: "Segoe UI", system-ui, sans-serif;
  padding: 6px 8px;
  resize: vertical;
  line-height: 1.5;
}

.note-content:focus {
  outline: none;
  border-color: var(--accent-blue);
}

.note-content::placeholder {
  color: var(--text-secondary);
  opacity: 0.6;
}

/* --- Empty state --- */
.empty-hint {
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary);
  padding: 8px 0;
  opacity: 0.7;
}
</style>
