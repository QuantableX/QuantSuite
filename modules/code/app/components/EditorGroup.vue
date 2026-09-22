<script setup lang="ts">
/**
 * One column of the split: tab strip, breadcrumbs, and the editor itself.
 *
 * The group renders ONE editor for its active tab rather than one per tab.
 * Monaco keys models by path, so switching tabs swaps the model and keeps
 * every buffer's undo history — a dozen editor instances would only cost
 * memory and layout work.
 */
import { useEditorStore } from '#code-root/stores/editor'
import { FileCode2 } from 'lucide-vue-next'
import { useBookmarksStore } from '#code-root/stores/bookmarks'
import type { EditorMarker } from '@quantsuite/ui'
import type { EditorGroup } from '#code-root/shared/types'

const props = defineProps<{
  group: EditorGroup
  theme: 'dark' | 'light'
  /** Workspace root, so breadcrumbs show a relative path. */
  root: string | null
}>()

const store = useEditorStore()
const bookmarksStore = useBookmarksStore()
const editorRef = ref<{ reveal: (line: number, column?: number) => void; focus: () => void } | null>(null)

const activeTab = computed(() => props.group.tabs.find((t) => t.id === props.group.activeTabId))
const isFocused = computed(() => store.activeGroupId === props.group.id)

/** The active file's path, split for the breadcrumb trail. */
const crumbs = computed(() => {
  const tab = activeTab.value
  if (!tab) return []
  const path = tab.path.replace(/\\/g, '/')
  const rootPath = props.root?.replace(/\\/g, '/') ?? ''
  const relative =
    rootPath && path.toLowerCase().startsWith(rootPath.toLowerCase())
      ? path.slice(rootPath.length)
      : path
  return relative.split('/').filter(Boolean)
})

function onInput(content: string) {
  if (activeTab.value) store.updateContent(activeTab.value.id, content)
}

function onSave() {
  if (activeTab.value) void store.saveTab(activeTab.value.id)
}

function onMarkers(markers: EditorMarker[]) {
  if (activeTab.value) store.setMarkers(activeTab.value.path, markers)
}

/** The caret moved — the store remembers the line for a bookmark toggle. */
function onCursor(pos: { line: number; column: number }) {
  if (isFocused.value) store.setCursorLine(pos.line)
}

/** Right-click on the gutter toggles a bookmark on that line — the VS Code
 *  Bookmarks gesture, minus the extension's line drift (stores/bookmarks). */
function onGutterContextMenu(target: { line: number }) {
  const tab = activeTab.value
  if (tab && tab.kind === 'text' && !tab.readOnly) {
    bookmarksStore.toggle(tab.path, target.line)
  }
}

/**
 * Markdown gets a rendered view, and the breadcrumb bar gets the switch for it.
 * The flag lives on the tab, so each split remembers its own side of a document
 * opened in both — source left, preview right.
 */
const isMarkdown = computed(() => activeTab.value?.language === 'markdown')
const showPreview = computed(() => isMarkdown.value && activeTab.value?.preview === true)

function togglePreview() {
  if (activeTab.value) store.toggleMarkdownPreview(activeTab.value.id)
}

/** Jump to a line — the problems panel and search results call this. */
defineExpose({
  reveal: (line: number, column = 1) => editorRef.value?.reveal(line, column),
})
</script>

<template>
  <!-- data-group-id is the tab drag's hit-test anchor: a pointer over any of
       this section's descendants — the strip, Monaco, the empty state —
       resolves to this group, and a drop outside a row appends to its working
       row (see TabStrip's updateDropTarget). -->
  <section
    class="group"
    :data-group-id="group.id"
    :class="{ 'is-focused': isFocused && store.groups.length > 1 }"
    @mousedown="store.setActiveGroup(group.id)"
  >
    <CodeTabStrip :group-id="group.id" :tabs="group.tabs" :active-tab-id="group.activeTabId" />

    <!-- Breadcrumbs: the file's place in the workspace, always one line. The
         markdown switch rides the same row rather than adding a second bar —
         one line of chrome per editor is the budget. -->
    <div v-if="activeTab" class="crumbs">
      <span class="crumb-trail">
        <template v-for="(crumb, i) in crumbs" :key="i">
          <span class="crumb" :class="{ 'is-last': i === crumbs.length - 1 }">{{ crumb }}</span>
          <span v-if="i < crumbs.length - 1" class="crumb-sep">›</span>
        </template>
      </span>

      <button
        v-if="isMarkdown"
        class="md-toggle"
        :class="{ 'is-on': showPreview }"
        :title="showPreview ? 'Edit source (Ctrl+Shift+V)' : 'Show preview (Ctrl+Shift+V)'"
        @click="togglePreview"
      >
        {{ showPreview ? 'Source' : 'Preview' }}
      </button>
    </div>

    <!-- Image viewer -->
    <div v-if="activeTab?.kind === 'image'" class="viewer">
      <img v-if="activeTab.content" :src="activeTab.content" :alt="activeTab.fileName" draggable="false" />
      <p v-else class="viewer-empty">Could not read {{ activeTab.fileName }}</p>
    </div>

    <!-- Rendered markdown. The store's buffer is the source, so a preview in
         one split follows the typing in the other. -->
    <div v-else-if="showPreview && activeTab" class="preview">
      <QMarkdownPreview :source="activeTab.content" tokens="qss" />
    </div>

    <!-- The shared editor -->
    <div v-else-if="activeTab" class="editor">
      <QCodeEditor
        ref="editorRef"
        :model-value="activeTab.content"
        :path="activeTab.path"
        :language="activeTab.language"
        :theme="theme"
        :minimap="store.minimap"
        :word-wrap="store.wordWrap"
        :read-only="activeTab.readOnly ?? false"
        tokens="qss"
        @update:model-value="onInput"
        @save="onSave"
        @markers="onMarkers"
        @cursor="onCursor"
        @gutter-context-menu="onGutterContextMenu"
      />
    </div>

    <!-- Nothing open in this group -->
    <div v-else class="empty">
      <QEmptyState title="No file open" description="Choose a file from the explorer.">
        <template #icon><FileCode2 :size="24" /></template>
        <span class="empty-hint"><kbd>Ctrl</kbd> <kbd>P</kbd> Quick open</span>
      </QEmptyState>
    </div>
  </section>
</template>

<style scoped>
.group {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--qss-bg);
  border-left: 1px solid var(--qss-border);
}
.group:first-child {
  border-left: none;
}
/* Which group the keyboard belongs to — only worth saying when there are two. */
.group.is-focused {
  box-shadow: inset 0 -2px 0 color-mix(in srgb, var(--qss-accent) 45%, transparent);
}

.crumbs {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  height: 24px;
  padding: 0 12px;
  overflow: hidden;
  white-space: nowrap;
  font-size: 11px;
  color: var(--qss-text-muted);
  border-bottom: 1px solid var(--qss-border-subtle, var(--qss-border));
}
/* The trail is what a long path eats into; the switch keeps its width. */
.crumb-trail {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
  min-width: 0;
  overflow: hidden;
}
.crumb.is-last {
  color: var(--qss-text-secondary);
}
.crumb-sep {
  opacity: 0.5;
}

.md-toggle {
  flex-shrink: 0;
  padding: 1px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  background: transparent;
  color: var(--qss-text-muted);
  font-size: 10px;
  cursor: pointer;
  transition: color var(--qss-dur-fast) var(--qss-ease-out),
    background var(--qss-dur-fast) var(--qss-ease-out);
}
.md-toggle:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.md-toggle.is-on {
  color: var(--qss-text-secondary);
}

.preview {
  flex: 1;
  min-height: 0;
}

.editor {
  flex: 1;
  min-height: 0;
}

.viewer {
  flex: 1;
  min-height: 0;
  display: grid;
  place-items: center;
  padding: 16px;
  overflow: auto;
  background: repeating-conic-gradient(var(--qss-bg-raised) 0% 25%, var(--qss-bg) 0% 50%) 50% / 16px 16px;
}
.viewer img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.viewer-empty {
  font-size: 12px;
  color: var(--qss-text-muted);
}

.empty {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin: 24px;
  border: 1px solid var(--qss-border);
  border-radius: 18px;
  background: radial-gradient(ellipse at 50% 0%, color-mix(in srgb, var(--qss-text) 4%, transparent), transparent 65%), var(--qss-bg-raised);
}
.empty-title {
  font-size: 13px;
  color: var(--qss-text-secondary);
}
.empty-hint {
  font-size: 12px;
  color: var(--qss-text-muted);
}
.empty-hint kbd {
  padding: 1px 5px;
  margin: 0 1px;
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  background: var(--qss-bg-raised);
  font-family: var(--qss-font-mono);
  font-size: 10px;
}
</style>
