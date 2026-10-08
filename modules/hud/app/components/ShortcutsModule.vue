<template>
  <div class="sc-module" :class="{ 'sc-edit-mode': editMode }">
    <div class="sc-controls">
      <div class="sc-header">
        <span class="sc-title">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
          </svg>
          Shortcuts
        </span>
        <button class="btn btn-primary btn-sm" :disabled="saving" @click="startAdd">+ Add</button>
      </div>
      <button class="sc-mode-toggle" :aria-pressed="editMode" :disabled="saving" @click="toggleEditMode">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L9 17l-4 1 1-4z" />
        </svg>
        Edit mode
        <span class="sc-switch" aria-hidden="true"><span /></span>
      </button>
      <p class="sc-mode-hint">{{ editMode ? 'Click to edit · Drag to reorder' : 'Click a shortcut to open it.' }}</p>
    </div>

    <form v-if="adding || editingId" class="sc-edit-form" :aria-label="adding ? 'Add shortcut' : 'Edit shortcut'" @submit.prevent="saveForm" @keydown.esc.prevent="cancelEditor">
      <input ref="labelInput" v-model="editLabel" class="sc-input" placeholder="Name" aria-label="Shortcut name" :disabled="saving" />
      <div class="sc-url-row">
        <input v-model="editUrl" class="sc-input sc-input-url" :placeholder="editType === 'url' ? 'https://...' : 'C:\\path\\to\\app.exe'" aria-label="Shortcut URL or application path" :disabled="saving" />
        <button v-if="editType === 'app'" type="button" class="btn btn-ghost btn-sm" :disabled="saving" @click="browseFile" title="Browse" aria-label="Browse for application">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
        </button>
      </div>
      <div class="sc-edit-actions">
        <select v-model="editType" class="sc-select" aria-label="Shortcut type" :disabled="saving">
          <option value="url">URL</option>
          <option value="app">Application</option>
        </select>
        <div class="sc-edit-btns">
          <button type="submit" class="btn btn-primary btn-sm" :disabled="saving || !editUrl.trim()">{{ saving ? 'Saving…' : 'Save' }}</button>
          <button type="button" class="btn btn-ghost btn-sm" :disabled="saving" @click="cancelEditor">Cancel</button>
          <button v-if="editingId" type="button" class="ctrl-btn ctrl-del" :disabled="saving" @click="removeSelected" title="Delete shortcut" aria-label="Delete shortcut">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M3 6h18M9 6V4h6v2M5 6l1 14h12l1-14M10 10v6M14 10v6" />
            </svg>
          </button>
        </div>
      </div>
    </form>

    <div v-if="!shortcuts.length && !adding" class="sc-empty">
      <p>No shortcuts yet.</p>
      <p class="sc-hint">Add URLs or applications for quick access.</p>
    </div>

    <div v-if="shortcuts.length" class="sc-list" aria-label="Shortcuts">
      <button
        v-for="(s, index) in shortcuts"
        :key="s.id"
        type="button"
        class="sc-tile"
        :class="{
          'sc-selected': editingId === s.id,
          'sc-dragging': dragIndex === index,
          'sc-drag-over': dragOverIndex === index && dragIndex !== index,
        }"
        :aria-label="`${editMode ? 'Edit' : 'Open'} ${s.label}`"
        :title="`${editMode ? 'Edit ' + s.label + '\n' : ''}${s.url}`"
        :disabled="saving"
        :draggable="editMode && !saving"
        @click="activateShortcut(s)"
        @dragstart="onDragStart($event, index)"
        @dragover="onDragOver($event, index)"
        @dragend="onDragEnd"
        @drop="onDrop($event, index)"
      >
        <span v-if="editMode" class="sc-drag-handle" aria-hidden="true">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="8" cy="4" r="2" /><circle cx="16" cy="4" r="2" />
            <circle cx="8" cy="12" r="2" /><circle cx="16" cy="12" r="2" />
            <circle cx="8" cy="20" r="2" /><circle cx="16" cy="20" r="2" />
          </svg>
        </span>
        <svg v-if="editMode" class="sc-edit-badge" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L9 17l-4 1 1-4z" />
        </svg>
        <span class="sc-icon" aria-hidden="true">
          <img v-if="s.favicon" :src="s.favicon" class="sc-favicon" alt="" draggable="false" />
          <svg v-else-if="s.type === 'url'" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10" />
            <line x1="2" y1="12" x2="22" y2="12" />
            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
          </svg>
          <svg v-else width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
            <line x1="8" y1="21" x2="16" y2="21" />
            <line x1="12" y1="17" x2="12" y2="21" />
          </svg>
        </span>
        <span class="sc-label">{{ s.label }}</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useShortcuts, type Shortcut } from '#hud/composables/useShortcuts'
const {
  shortcuts,
  addShortcut,
  updateShortcut,
  deleteShortcut,
  openShortcut,
  reorderShortcuts,
  fetchFavicon,
  fetchAppIcon,
  pickFile,
} = useShortcuts();

const editMode = ref(false);
const adding = ref(false);
const editingId = ref<string | null>(null);
const editLabel = ref("");
const editUrl = ref("");
const editType = ref<"url" | "app">("url");
const labelInput = ref<HTMLInputElement | null>(null);
const saving = ref(false);

// --- Drag and drop (only while arranging shortcuts) ---
const dragIndex = ref<number | null>(null);
const dragOverIndex = ref<number | null>(null);

function onDragStart(e: DragEvent, index: number) {
  if (!editMode.value || saving.value) {
    e.preventDefault();
    return;
  }
  dragIndex.value = index;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", shortcuts.value[index].id);
  }
}

function onDragOver(e: DragEvent, index: number) {
  if (dragIndex.value === null) return;
  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dragOverIndex.value = index;
}

function onDrop(e: DragEvent, toIndex: number) {
  e.preventDefault();
  if (editMode.value && dragIndex.value !== null && dragIndex.value !== toIndex) {
    reorderShortcuts(dragIndex.value, toIndex);
  }
  onDragEnd();
}

function onDragEnd() {
  dragIndex.value = null;
  dragOverIndex.value = null;
}

function toggleEditMode() {
  editMode.value = !editMode.value;
  cancelEditor();
  onDragEnd();
}

function activateShortcut(shortcut: Shortcut) {
  if (saving.value || dragIndex.value !== null) return;
  if (editMode.value) startEdit(shortcut);
  else openShortcut(shortcut);
}

async function focusEditor() {
  await nextTick();
  labelInput.value?.focus();
}

function startAdd() {
  adding.value = true;
  editingId.value = null;
  editLabel.value = "";
  editUrl.value = "";
  editType.value = "url";
  focusEditor();
}

function startEdit(s: Shortcut) {
  editingId.value = s.id;
  adding.value = false;
  editLabel.value = s.label;
  editUrl.value = s.url;
  editType.value = s.type;
  focusEditor();
}

function cancelEditor() {
  if (saving.value) return;
  adding.value = false;
  editingId.value = null;
}

function removeSelected() {
  if (editingId.value) deleteShortcut(editingId.value);
  cancelEditor();
}

async function saveForm() {
  const url = editUrl.value.trim();
  if (!url || saving.value) return;
  saving.value = true;
  try {
    const label = editLabel.value.trim() || url;
    const type = editType.value;
    const existing = shortcuts.value.find((s) => s.id === editingId.value);
    const data: Partial<Shortcut> = { label, url, type };

    // Keep the existing icon on a name-only edit, including stored app icons.
    if (!existing || existing.url !== url || existing.type !== type) {
      data.favicon = type === "url" ? await fetchFavicon(url) : await fetchAppIcon(url);
    }
    if (existing) {
      updateShortcut(existing.id, data);
    } else {
      addShortcut(label, url, type);
      const added = shortcuts.value[shortcuts.value.length - 1];
      if (added && data.favicon) updateShortcut(added.id, { favicon: data.favicon });
    }
    adding.value = false;
    editingId.value = null;
  } finally {
    saving.value = false;
  }
}

async function browseFile() {
  const path = await pickFile();
  if (path) {
    editUrl.value = path;
    if (!editLabel.value.trim()) {
      const parts = path.replace(/\\/g, "/").split("/");
      const filename = parts[parts.length - 1] || "";
      editLabel.value = filename.replace(/\.\w+$/, "");
    }
  }
}
</script>

<style scoped>
.sc-module {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.sc-controls { flex-shrink: 0; }
.sc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.sc-title {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.sc-mode-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 9px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.sc-mode-toggle[aria-pressed="true"] { color: var(--text-primary); border-color: var(--accent-blue); }
.sc-switch { display: flex; align-items: center; width: 26px; height: 16px; padding: 3px; margin-left: auto; border-radius: 10px; background: var(--border-color); box-sizing: border-box; }
.sc-switch > span { width: 10px; height: 10px; border-radius: 50%; background: var(--text-secondary); }
.sc-mode-toggle[aria-pressed="true"] .sc-switch { background: var(--accent-blue); }
.sc-mode-toggle[aria-pressed="true"] .sc-switch > span { transform: translateX(10px); background: var(--bg-primary); }
.sc-mode-hint { margin: 7px 0 0; font-size: 11px; line-height: 1.5; color: var(--text-secondary); }
.sc-empty { text-align: center; padding: 24px 0; color: var(--text-secondary); font-size: 13px; }
.sc-hint { font-size: 11px; margin-top: 4px; }
.sc-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  align-content: start;
  gap: 10px;
  min-height: 0;
}
.sc-tile {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-width: 0;
  min-height: 116px;
  padding: 18px 10px 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  color: var(--text-primary);
  font: inherit;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}
.sc-tile:hover { background: var(--bg-secondary); border-color: var(--text-secondary); }
.sc-edit-mode .sc-tile { border-style: dashed; }
.sc-tile.sc-selected { border-style: solid; border-color: var(--accent-blue); background: var(--bg-secondary); }
.sc-tile:focus-visible, .sc-mode-toggle:focus-visible, .ctrl-btn:focus-visible { outline: 2px solid var(--accent-blue); outline-offset: 2px; }
.sc-drag-handle, .sc-edit-badge { position: absolute; top: 7px; color: var(--text-secondary); }
.sc-drag-handle { left: 7px; display: flex; cursor: grab; }
.sc-edit-badge { right: 7px; }
.sc-dragging { opacity: 0.3; }
.sc-drag-over { border-color: var(--accent-blue); box-shadow: inset 0 0 0 1px var(--accent-blue); }
.sc-icon { flex-shrink: 0; width: 48px; height: 48px; display: flex; align-items: center; justify-content: center; color: var(--text-secondary); }
.sc-favicon { width: 48px; height: 48px; object-fit: contain; border-radius: 6px; }
.sc-label {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  width: 100%;
  min-height: 32px;
  overflow: hidden;
  overflow-wrap: anywhere;
  text-align: center;
  font-size: 12px;
  font-weight: 500;
  line-height: 16px;
}
.ctrl-btn { background: none; border: none; color: var(--text-secondary); cursor: pointer; width: 26px; height: 26px; display: flex; align-items: center; justify-content: center; border-radius: 4px; }
.ctrl-del:hover { color: var(--accent-red); background: var(--bg-primary); }
.sc-edit-form {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 8px;
  padding: 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  width: 100%;
  box-sizing: border-box;
  flex-shrink: 0;
}
.sc-input { width: 100%; min-width: 0; padding: 6px 8px; font-size: 12px; border: 1px solid var(--border-color); border-radius: 4px; background: var(--input-bg, var(--bg-primary)); color: var(--text-primary); outline: none; box-sizing: border-box; }
.sc-input:focus { border-color: var(--accent-blue); }
.sc-url-row { display: flex; gap: 4px; align-items: center; }
.sc-input-url { flex: 1; }
.sc-select { min-width: 0; padding: 4px 6px; font-size: 11px; border: 1px solid var(--border-color); border-radius: 4px; background: var(--input-bg, var(--bg-primary)); color: var(--text-primary); }
.sc-edit-actions { display: flex; align-items: center; justify-content: space-between; gap: 6px; }
.sc-edit-btns { display: flex; align-items: center; gap: 4px; }
.btn-sm { font-size: 12px; padding: 4px 10px; }
button:disabled { cursor: default; opacity: 0.5; }
</style>
