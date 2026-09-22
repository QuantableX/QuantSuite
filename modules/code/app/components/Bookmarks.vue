<script setup lang="ts">
/**
 * The toolkit's Bookmarks section — pinned lines, grouped by file.
 *
 * The list renders whatever the bookmarks store says, and the store keeps the
 * line numbers honest: every bookmark on an open file is a Monaco decoration,
 * so a line typed above it moves the bookmark WITH the code (the drift the
 * VS Code extension suffers is exactly what stores/bookmarks.ts exists to
 * kill). Toggling happens in the editor — Ctrl+Alt+K on the caret's line —
 * and here, per row, for removal.
 */
import { useBookmarksStore } from '#code-root/stores/bookmarks'

const props = defineProps<{ root: string | null }>()
const emit = defineEmits<{ (e: 'reveal', target: { path: string; line: number }): void }>()

const store = useBookmarksStore()

function nameOf(path: string): string {
  return path.split(/[/\\]/).pop() ?? path
}

/** The path relative to the workspace, for the group header's small print. */
function relative(path: string): string {
  const p = path.replace(/\\/g, '/')
  const root = props.root?.replace(/\\/g, '/') ?? ''
  return root && p.toLowerCase().startsWith(root.toLowerCase())
    ? p.slice(root.length).replace(/^\//, '')
    : p
}
</script>

<template>
  <div class="bm">
    <p v-if="!store.bookmarks.length" class="bm-empty">
      No bookmarks. <kbd>Ctrl</kbd><kbd>Alt</kbd><kbd>K</kbd> pins the current line.
    </p>

    <div v-else class="bm-list">
      <div class="bm-toolbar">
        <button class="bm-clear" title="Remove all bookmarks" @click="store.clearAll()">
          Clear all
        </button>
      </div>
      <div v-for="group in store.byFile" :key="group.path" class="bm-group">
        <div class="bm-file" :title="group.path">
          <span class="bm-file-name">{{ nameOf(group.path) }}</span>
          <span class="bm-file-dir">{{ relative(group.path) }}</span>
        </div>
        <div v-for="b in group.list" :key="b.id" class="bm-row">
          <button
            class="bm-main"
            :title="`${group.path}:${b.line}`"
            @click="emit('reveal', { path: b.path, line: b.line })"
          >
            <span class="bm-line">{{ b.line }}</span>
            <span class="bm-text">{{ b.text || '·' }}</span>
          </button>
          <button class="bm-remove" title="Remove bookmark" @click="store.remove(b.id)">
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bm {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.bm-empty {
  padding: 8px 12px 12px;
  font-size: 11.5px;
  color: var(--qss-text-muted);
}
.bm-empty kbd {
  padding: 0 3px;
  border: 1px solid var(--qss-border);
  border-radius: 3px;
  font-family: var(--qss-font-mono);
  font-size: 9px;
}

.bm-list {
  padding: 2px 0 8px;
}

.bm-toolbar {
  display: flex;
  justify-content: flex-end;
  padding: 2px 8px;
}
.bm-clear {
  padding: 2px 6px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  font-size: 10px;
  cursor: pointer;
}
.bm-clear:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.bm-group + .bm-group {
  margin-top: 6px;
}

.bm-file {
  display: flex;
  align-items: baseline;
  gap: 7px;
  padding: 2px 12px;
  font-size: 11px;
  color: var(--qss-text-secondary);
}
.bm-file-name {
  flex-shrink: 0;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bm-file-dir {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  color: var(--qss-text-muted);
  opacity: 0.7;
}

.bm-row {
  display: flex;
  align-items: center;
}
.bm-row:hover {
  background: var(--qss-bg-hover);
}

.bm-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 2px 4px 2px 12px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.bm-row:hover .bm-main {
  color: var(--qss-text);
}

.bm-line {
  flex-shrink: 0;
  min-width: 26px;
  font-family: var(--qss-font-mono);
  font-size: 10px;
  text-align: right;
  color: var(--qss-text-muted);
}

.bm-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--qss-font-mono);
  font-size: 11px;
}

.bm-remove {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  margin-right: 8px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  opacity: 0;
}
.bm-row:hover .bm-remove {
  opacity: 1;
}
.bm-remove:hover {
  background: var(--qss-bg-active, var(--qss-bg-hover));
  color: var(--qss-text);
}
</style>

<!-- Monaco renders the gutter mark; its DOM is outside this component, so the
     class it carries has to be global. -->
<style>
.qcode-bookmark-line {
  position: relative;
}
.qcode-bookmark-line::after {
  content: '';
  position: absolute;
  top: 50%;
  left: 50%;
  width: 6px;
  height: 6px;
  transform: translate(-50%, -50%);
  border-radius: 50%;
  background: var(--qss-text-secondary, #a0a0a8);
}
</style>
