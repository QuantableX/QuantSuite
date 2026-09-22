<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { marked } from 'marked'
import { workspaceStorageDir } from '../../../lib/specs/engine'
import { useAppStore } from '../../../stores/app'
import { useWorkspacesStore } from '../../../stores/workspaces'

const appStore = useAppStore()
const workspacesStore = useWorkspacesStore()

const content = ref('')
const saving = ref(false)
const dirty = ref(false)
const fileExists = ref(false)
const loading = ref(true)

type ViewMode = 'source' | 'split' | 'preview'
const viewMode = ref<ViewMode>('split')

// Debounced instead of a computed: split view is the default, so an eager
// render re-parsed the whole note and rebuilt the preview DOM per keystroke.
const renderedMarkdown = ref('')
let renderTimer: ReturnType<typeof setTimeout> | null = null

watch(content, (value) => {
  if (renderTimer) clearTimeout(renderTimer)
  renderTimer = setTimeout(() => {
    renderedMarkdown.value = value ? (marked.parse(value) as string) : ''
  }, 200)
}, { immediate: true })

onUnmounted(() => {
  if (renderTimer) clearTimeout(renderTimer)
})

const workspaceName = computed(() => {
  return workspacesStore.contentWorkspace?.name ?? 'General'
})

const notesPath = ref<string | null>(null)
const loadError = ref('')
let loadVersion = 0

async function loadNotes() {
  const version = ++loadVersion
  const workspace = workspacesStore.contentWorkspace
  notesPath.value = null
  content.value = ''
  fileExists.value = false
  dirty.value = false
  loadError.value = ''
  loading.value = true
  try {
    if (!workspace) return
    const storage = await workspaceStorageDir(workspace.path)
    const path = `${storage}/NOTES.md`
    let text = ''
    let exists = false
    try {
      text = await invoke<string>('plugin:canvas|read_file', { path })
      exists = true
    } catch (error) {
      if (!String(error).startsWith('File does not exist:')) throw error
    }
    if (version !== loadVersion) return
    notesPath.value = path
    content.value = text
    fileExists.value = exists
  } catch (error) {
    if (version === loadVersion) loadError.value = String(error)
  } finally {
    if (version === loadVersion) loading.value = false
  }
}

async function createNotesFile() {
  if (!notesPath.value) return
  const version = loadVersion
  saving.value = true
  try {
    const initial = `# ${workspaceName.value} Notes\n\n`
    await invoke('plugin:canvas|write_file', {
      path: notesPath.value,
      content: initial,
    })
    if (version === loadVersion) {
      content.value = initial
      fileExists.value = true
      dirty.value = false
    }
  } catch (e) {
    console.error('Failed to create notes file:', e)
  } finally {
    saving.value = false
  }
}

async function saveNotes() {
  if (!notesPath.value) return
  const version = loadVersion
  const saved = content.value
  saving.value = true
  try {
    await invoke('plugin:canvas|write_file', {
      path: notesPath.value,
      content: saved,
    })
    if (version === loadVersion && content.value === saved) dirty.value = false
  } catch (e) {
    console.error('Failed to save notes:', e)
  } finally {
    saving.value = false
  }
}

function onInput(e: Event) {
  content.value = (e.target as HTMLTextAreaElement).value
  dirty.value = true
}

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 's') {
    e.preventDefault()
    saveNotes()
  }
  if (e.key === 'Tab') {
    e.preventDefault()
    const ta = e.target as HTMLTextAreaElement
    const start = ta.selectionStart
    const end = ta.selectionEnd
    content.value = content.value.substring(0, start) + '  ' + content.value.substring(end)
    dirty.value = true
    nextTick(() => {
      ta.selectionStart = ta.selectionEnd = start + 2
    })
  }
}

onMounted(() => {
  loadNotes()
})

watch(() => workspacesStore.contentWorkspaceId, () => {
  loadNotes()
})
</script>

<template>
  <div class="notes-bar">
    <!-- Header -->
    <div class="notes-header">
      <div class="notes-header-left">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <line x1="16" y1="13" x2="8" y2="13" />
          <line x1="16" y1="17" x2="8" y2="17" />
          <polyline points="10 9 9 9 8 9" />
        </svg>
        <span class="notes-title">NOTES</span>
        <span v-if="dirty" class="notes-dirty" title="Unsaved changes" />
      </div>
      <div class="notes-header-right">
        <!-- View mode dropdown -->
        <select
          v-if="fileExists"
          class="notes-mode-select"
          :value="viewMode"
          @change="viewMode = ($event.target as HTMLSelectElement).value as ViewMode"
        >
          <option value="split">Split</option>
          <option value="source">Source</option>
          <option value="preview">Preview</option>
        </select>
        <button
          v-if="fileExists"
          class="notes-btn"
          :disabled="saving || !dirty"
          title="Save (Ctrl+S)"
          @click="saveNotes"
        >
          {{ saving ? 'Saving...' : 'Save' }}
        </button>
        <button
          class="notes-btn"
          title="Close (Ctrl+J)"
          @click="appStore.toggleNotesBar()"
        >
          &#10005;
        </button>
      </div>
    </div>

    <div v-if="loadError" class="notes-empty" role="alert">
      <span>{{ loadError }}</span>
      <button class="notes-create-btn" @click="loadNotes">Retry</button>
    </div>

    <!-- Create file prompt -->
    <div v-else-if="!loading && !fileExists" class="notes-empty">
      <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" style="opacity: 0.4; margin-bottom: 10px;">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="12" y1="18" x2="12" y2="12" />
        <line x1="9" y1="15" x2="15" y2="15" />
      </svg>
      <span class="notes-empty-text">No notes for this workspace</span>
      <button class="notes-create-btn" :disabled="saving || !notesPath" @click="createNotesFile">
        {{ saving ? 'Creating...' : 'Create notes' }}
      </button>
    </div>

    <!-- Content area (when file exists) -->
    <div v-else-if="fileExists" class="notes-content">
      <!-- Source only -->
      <textarea
        v-if="viewMode === 'source'"
        class="notes-editor"
        :value="content"
        placeholder="Write your notes here... (Markdown supported)"
        spellcheck="false"
        @input="onInput"
        @keydown="onKeydown"
      />

      <!-- Split view -->
      <template v-else-if="viewMode === 'split'">
        <textarea
          class="notes-editor notes-split-pane"
          :value="content"
          placeholder="Write your notes here..."
          spellcheck="false"
          @input="onInput"
          @keydown="onKeydown"
        />
        <div class="notes-split-divider" />
        <div
          class="notes-preview notes-split-pane markdown-body"
          v-html="renderedMarkdown"
        />
      </template>

      <!-- Preview only -->
      <div
        v-else
        class="notes-preview markdown-body"
        v-html="renderedMarkdown"
      />
    </div>
  </div>
</template>

<style scoped>
.notes-bar {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--qc-bg-titlebar);
}

.notes-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 12px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--qc-border);
}

.notes-header-left {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--qc-text-muted);
}

.notes-title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--qc-text-muted);
}

.notes-dirty {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #a0a0a8;
}

.notes-header-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

.notes-mode-select {
  font-size: 10px;
  padding: 1px 4px;
  border-radius: 4px;
  border: 1px solid var(--qc-border);
  background: transparent;
  color: var(--qc-text-muted);
  cursor: pointer;
  outline: none;
  transition: color 0.15s, border-color 0.15s;
}

.notes-mode-select:hover {
  color: var(--qc-text);
  border-color: var(--qc-text-dim);
}

.notes-mode-select option {
  background: var(--qc-bg-surface);
  color: var(--qc-text);
}

.notes-btn {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--qc-text-muted);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: color 0.15s;
}

.notes-btn:hover:not(:disabled) {
  color: var(--qc-text);
}

.notes-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.notes-content {
  flex: 1;
  min-height: 0;
  display: flex;
}

.notes-editor {
  flex: 1;
  min-height: 0;
  min-width: 0;
  width: 100%;
  padding: 10px 14px;
  background: var(--qc-bg-window);
  color: var(--qc-text);
  border: none;
  outline: none;
  resize: none;
  font-family: "JetBrains Mono", "Fira Code", "Cascadia Code", monospace;
  font-size: 13px;
  line-height: 1.6;
  tab-size: 2;
}

.notes-editor::placeholder {
  color: var(--qc-text-dim);
}

.notes-split-pane {
  flex: 1;
  min-width: 0;
  width: 50%;
}

.notes-split-divider {
  width: 1px;
  flex-shrink: 0;
  background: var(--qc-border);
}

.notes-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  background: var(--qc-bg-window);
  color: var(--qc-text-muted);
}

.notes-empty-text {
  font-size: 11px;
  color: var(--qc-text-dim);
}

.notes-create-btn {
  margin-top: 8px;
  font-size: 11px;
  padding: 5px 14px;
  border-radius: 6px;
  color: var(--qc-text);
  background: var(--qc-bg-surface);
  border: 1px solid var(--qc-border);
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}

.notes-create-btn:hover:not(:disabled) {
  background: var(--qc-bg-header);
  border-color: var(--qc-text-dim);
}

.notes-create-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.notes-preview {
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: auto;
  padding: 10px 14px;
  background: var(--qc-bg-window);
}

/* Markdown rendered output styling (matches EditorPanel) */
.markdown-body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  font-size: 13px;
  line-height: 1.6;
  color: var(--qc-text);
}

.markdown-body :deep(h1) {
  font-size: 1.6em;
  font-weight: 700;
  margin: 0.6em 0 0.4em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--qc-border);
}

.markdown-body :deep(h2) {
  font-size: 1.3em;
  font-weight: 600;
  margin: 0.6em 0 0.3em;
  padding-bottom: 0.2em;
  border-bottom: 1px solid var(--qc-border);
}

.markdown-body :deep(h3) {
  font-size: 1.1em;
  font-weight: 600;
  margin: 0.5em 0 0.3em;
}

.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  margin: 0.4em 0 0.2em;
}

.markdown-body :deep(p) {
  margin: 0.4em 0;
}

.markdown-body :deep(a) {
  color: #60a5fa;
  text-decoration: none;
}

.markdown-body :deep(a:hover) {
  text-decoration: underline;
}

.markdown-body :deep(strong) {
  font-weight: 700;
  color: var(--qc-text);
}

.markdown-body :deep(em) {
  font-style: italic;
}

.markdown-body :deep(code) {
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 0.88em;
  padding: 0.15em 0.35em;
  border-radius: 4px;
  background: var(--qc-bg-surface);
  color: #f59e0b;
}

.markdown-body :deep(pre) {
  margin: 0.5em 0;
  padding: 10px 14px;
  border-radius: 6px;
  background: var(--qc-bg-surface);
  overflow-x: auto;
}

.markdown-body :deep(pre code) {
  padding: 0;
  background: none;
  color: var(--qc-text);
  font-size: 12px;
  line-height: 1.5;
}

.markdown-body :deep(blockquote) {
  margin: 0.5em 0;
  padding: 0.3em 1em;
  border-left: 3px solid var(--qc-border);
  color: var(--qc-text-muted);
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin: 0.4em 0;
  padding-left: 1.5em;
}

.markdown-body :deep(li) {
  margin: 0.15em 0;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--qc-border);
  margin: 0.8em 0;
}

.markdown-body :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 0.5em 0;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  padding: 6px 10px;
  border: 1px solid var(--qc-border);
  font-size: 12px;
}

.markdown-body :deep(th) {
  font-weight: 600;
  background: var(--qc-bg-surface);
}

.markdown-body :deep(img) {
  max-width: 100%;
  border-radius: 4px;
}

.markdown-body :deep(input[type="checkbox"]) {
  margin-right: 0.4em;
}
</style>
