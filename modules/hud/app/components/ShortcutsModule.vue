<template>
  <div class="sc-module">
    <div class="hud-top-group">
    <div class="sc-header">
      <span class="sc-title">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="vertical-align: middle; margin-right: 4px"
        >
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
        Shortcuts
      </span>
      <button class="btn btn-primary btn-sm" @click="startAdd">
        + Add
      </button>
    </div>

    <!-- Add form -->
    <div v-if="adding" class="sc-edit-form">
      <input
        v-model="editLabel"
        class="sc-input"
        placeholder="Label"
        @keydown.enter="saveNew"
        @keydown.escape="cancelAdd"
      />
      <div class="sc-url-row">
        <input
          v-model="editUrl"
          class="sc-input sc-input-url"
          :placeholder="editType === 'url' ? 'https://...' : 'C:\\path\\to\\app.exe'"
          @keydown.enter="saveNew"
          @keydown.escape="cancelAdd"
        />
        <button
          v-if="editType === 'app'"
          class="btn btn-ghost btn-sm"
          @click="browseFile"
          title="Browse"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
        </button>
      </div>
      <div class="sc-edit-actions">
        <select v-model="editType" class="sc-select">
          <option value="url">URL</option>
          <option value="app">Application</option>
        </select>
        <div class="sc-edit-btns">
          <button class="btn btn-primary btn-sm" @click="saveNew">Save</button>
          <button class="btn btn-ghost btn-sm" @click="cancelAdd">Cancel</button>
        </div>
      </div>
    </div>

    </div>
    <div v-if="!shortcuts.length && !adding" class="sc-empty">
      <p>No shortcuts yet.</p>
      <p class="sc-hint">Add URLs or applications for quick access.</p>
    </div>

    <div v-if="shortcuts.length" class="sc-list">
      <div
        v-for="(s, index) in shortcuts"
        :key="s.id"
        class="sc-row"
        :class="{
          'sc-dragging': dragIndex === index,
          'sc-drag-over': dragOverIndex === index && dragIndex !== index,
        }"
        :draggable="editingId !== s.id"
        @dragstart="onDragStart($event, index)"
        @dragover="onDragOver($event, index)"
        @dragend="onDragEnd"
        @drop="onDrop($event, index)"
      >
        <!-- Normal view -->
        <template v-if="editingId !== s.id">
          <div class="sc-drag-handle" title="Drag to reorder">
            <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor">
              <circle cx="8" cy="4" r="2" /><circle cx="16" cy="4" r="2" />
              <circle cx="8" cy="12" r="2" /><circle cx="16" cy="12" r="2" />
              <circle cx="8" cy="20" r="2" /><circle cx="16" cy="20" r="2" />
            </svg>
          </div>
          <div class="sc-icon" @click="openShortcut(s)">
            <img
              v-if="s.favicon"
              :src="s.favicon"
              class="sc-favicon"
            />
            <svg
              v-else-if="s.type === 'url'"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="12" cy="12" r="10" />
              <line x1="2" y1="12" x2="22" y2="12" />
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
            <svg
              v-else
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
              <line x1="8" y1="21" x2="16" y2="21" />
              <line x1="12" y1="17" x2="12" y2="21" />
            </svg>
          </div>
          <div class="sc-content" @click="openShortcut(s)">
            <span class="sc-label">{{ s.label }}</span>
            <span class="sc-url-hint">{{
              s.url.length > 40 ? s.url.slice(0, 40) + "…" : s.url
            }}</span>
          </div>
          <div class="sc-actions">
            <button
              class="ctrl-btn"
              @click="startEdit(s)"
              title="Edit"
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
              </svg>
            </button>
            <button
              class="ctrl-btn ctrl-del"
              @click="deleteShortcut(s.id)"
              title="Delete"
            >
              ✕
            </button>
          </div>
        </template>

        <!-- Inline edit view -->
        <template v-else>
          <div class="sc-edit-inline">
            <input
              v-model="editLabel"
              class="sc-input"
              placeholder="Label"
              @keydown.enter="saveEdit(s.id)"
              @keydown.escape="cancelEdit"
            />
            <div class="sc-url-row">
              <input
                v-model="editUrl"
                class="sc-input sc-input-url"
                :placeholder="editType === 'url' ? 'https://...' : 'C:\\path\\to\\app.exe'"
                @keydown.enter="saveEdit(s.id)"
                @keydown.escape="cancelEdit"
              />
              <button
                v-if="editType === 'app'"
                class="btn btn-ghost btn-sm"
                @click="browseFile"
                title="Browse"
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
                </svg>
              </button>
            </div>
            <div class="sc-edit-actions">
              <select v-model="editType" class="sc-select">
                <option value="url">URL</option>
                <option value="app">Application</option>
              </select>
              <div class="sc-edit-btns">
                <button class="btn btn-primary btn-sm" @click="saveEdit(s.id)">Save</button>
                <button class="btn btn-ghost btn-sm" @click="cancelEdit">Cancel</button>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useShortcuts } from '#hud/composables/useShortcuts'
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

const adding = ref(false);
const editingId = ref<string | null>(null);
const editLabel = ref("");
const editUrl = ref("");
const editType = ref<"url" | "app">("url");

// --- Drag and drop ---
const dragIndex = ref<number | null>(null);
const dragOverIndex = ref<number | null>(null);

function onDragStart(e: DragEvent, index: number) {
  dragIndex.value = index;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
  }
}

function onDragOver(e: DragEvent, index: number) {
  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dragOverIndex.value = index;
}

function onDrop(e: DragEvent, toIndex: number) {
  e.preventDefault();
  if (dragIndex.value !== null && dragIndex.value !== toIndex) {
    reorderShortcuts(dragIndex.value, toIndex);
  }
  dragIndex.value = null;
  dragOverIndex.value = null;
}

function onDragEnd() {
  dragIndex.value = null;
  dragOverIndex.value = null;
}

function startAdd() {
  adding.value = true;
  editingId.value = null;
  editLabel.value = "";
  editUrl.value = "";
  editType.value = "url";
}

function cancelAdd() {
  adding.value = false;
}

async function saveNew() {
  if (!editLabel.value.trim() && !editUrl.value.trim()) return;
  const label = editLabel.value.trim() || editUrl.value.trim();
  addShortcut(label, editUrl.value.trim(), editType.value);

  // Auto-fetch icon
  const last = shortcuts.value[shortcuts.value.length - 1];
  if (last && editUrl.value.trim()) {
    if (editType.value === "url") {
      const fav = await fetchFavicon(editUrl.value.trim());
      if (fav) updateShortcut(last.id, { favicon: fav });
    } else if (editType.value === "app") {
      const icon = await fetchAppIcon(editUrl.value.trim());
      if (icon) updateShortcut(last.id, { favicon: icon });
    }
  }

  adding.value = false;
}

function startEdit(s: { id: string; label: string; url: string; type: "url" | "app" }) {
  editingId.value = s.id;
  adding.value = false;
  editLabel.value = s.label;
  editUrl.value = s.url;
  editType.value = s.type;
}

function cancelEdit() {
  editingId.value = null;
}

async function saveEdit(id: string) {
  if (!editLabel.value.trim() && !editUrl.value.trim()) return;
  const label = editLabel.value.trim() || editUrl.value.trim();
  const data: Record<string, unknown> = {
    label,
    url: editUrl.value.trim(),
    type: editType.value,
  };

  // Auto-fetch favicon for URLs
  if (editType.value === "url" && editUrl.value.trim()) {
    const fav = await fetchFavicon(editUrl.value.trim());
    if (fav) data.favicon = fav;
  } else if (editType.value === "app" && editUrl.value.trim()) {
    const icon = await fetchAppIcon(editUrl.value.trim());
    data.favicon = icon || null;
  }

  updateShortcut(id, data);
  editingId.value = null;
}

async function browseFile() {
  const path = await pickFile();
  if (path) {
    editUrl.value = path;
    if (!editLabel.value.trim()) {
      // Auto-fill label from filename
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
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.sc-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.sc-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.sc-empty {
  text-align: center;
  padding: 24px 0;
  color: var(--text-secondary);
  font-size: 13px;
}
.sc-hint {
  font-size: 11px;
  margin-top: 4px;
}
.sc-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--bg-secondary);
}
.sc-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  transition: background 0.15s;
  flex-shrink: 0;
}
.sc-row:hover {
  background: var(--bg-secondary);
}
.sc-drag-handle {
  flex-shrink: 0;
  width: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
  color: var(--text-secondary);
  opacity: 0.4;
  transition: opacity 0.15s;
}
.sc-drag-handle:active {
  cursor: grabbing;
}
.sc-row:hover .sc-drag-handle {
  opacity: 0.8;
}
.sc-dragging {
  opacity: 0.3;
}
.sc-drag-over {
  border-color: var(--accent-blue) !important;
  box-shadow: 0 0 0 1px var(--accent-blue);
}
.sc-icon {
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: var(--text-secondary);
}
.sc-favicon {
  width: 16px;
  height: 16px;
  border-radius: 2px;
}
.sc-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  cursor: pointer;
}
.sc-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--accent-blue);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sc-label:hover {
  text-decoration: underline;
}
.sc-url-hint {
  font-size: 10px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sc-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
}
.ctrl-btn:hover {
  color: var(--text-primary);
}
.ctrl-del:hover {
  color: var(--accent-red);
}
.sc-edit-form,
.sc-edit-inline {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  margin-bottom: 6px;
  width: 100%;
}
.sc-input {
  width: 100%;
  padding: 4px 8px;
  font-size: 12px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: var(--input-bg, var(--bg-primary));
  color: var(--text-primary);
  outline: none;
  box-sizing: border-box;
}
.sc-input:focus {
  border-color: var(--accent-blue);
}
.sc-url-row {
  display: flex;
  gap: 4px;
  align-items: center;
}
.sc-input-url {
  flex: 1;
}
.sc-select {
  padding: 3px 6px;
  font-size: 11px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: var(--input-bg, var(--bg-primary));
  color: var(--text-primary);
  outline: none;
}
.sc-edit-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}
.sc-edit-btns {
  display: flex;
  gap: 4px;
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
</style>
