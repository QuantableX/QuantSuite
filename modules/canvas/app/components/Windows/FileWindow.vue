<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { marked } from 'marked'
import { useAppStore, type Theme } from '../../../stores/app'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import type { CanvasWindow as CanvasWindowType } from '../../../shared/types'

const props = defineProps<{
  window: CanvasWindowType
}>()

const appStore = useAppStore()
const canvasStore = useCanvasStore()
const workspacesStore = useWorkspacesStore()

const fileConfig = computed(() => props.window.fileConfig)
const loading = ref(true)
const error = ref('')
const isDirty = ref(false)

// Markdown preview toggle: true = rendered, false = editor
const showPreview = ref(true)

// Image state
const imageSrc = ref('')

// Monaco state
const editorContainer = ref<HTMLElement | null>(null)
let editorInstance: any = null
let monacoModule: any = null
let resizeObserver: ResizeObserver | null = null
let ignoreContentChange = false

// Markdown rendered HTML (computed from editor content)
const markdownHtml = ref('')

// Debounced: parsing the whole document and swapping it into the v-html target
// on every keystroke rebuilt the preview DOM at typing rate.
let previewTimer: ReturnType<typeof setTimeout> | null = null

function schedulePreviewUpdate() {
  if (previewTimer) clearTimeout(previewTimer)
  previewTimer = setTimeout(() => {
    const content = editorInstance?.getValue() ?? ''
    // marked.parse returns string | Promise<string> depending on options;
    // wrapping makes both branches awaitable without changing behaviour.
    Promise.resolve(marked.parse(content)).then((html: string) => {
      markdownHtml.value = html
    })
  }, 200)
}

const fileName = computed(() => {
  if (!fileConfig.value) return ''
  return fileConfig.value.filePath.split(/[/\\]/).pop() ?? fileConfig.value.filePath
})

const isImage = computed(() => fileConfig.value?.previewKind === 'image')
const isMarkdown = computed(() => fileConfig.value?.previewKind === 'markdown')
const isEditable = computed(() => {
  const k = fileConfig.value?.previewKind
  return k === 'code' || k === 'markdown' || k === 'text'
})

// Show the editor when: editable AND (not markdown OR not in preview mode)
const showEditor = computed(() => isEditable.value && (!isMarkdown.value || !showPreview.value))

async function initMonaco(content: string, language: string) {
  if (!editorContainer.value) return

  try {
    monacoModule = await import('monaco-editor')

    try {
      monacoModule.editor.defineTheme('quantcode-dark', {
        base: 'vs-dark',
        inherit: true,
        rules: [
          { token: 'comment', foreground: 'a5a5a5', fontStyle: 'italic' },
          { token: 'keyword', foreground: '4472c4' },
          { token: 'keyword.operator', foreground: '4472c4' },
          { token: 'string', foreground: 'ed7d31' },
          { token: 'string.escape', foreground: 'f4b183' },
          { token: 'regexp', foreground: 'f4b183' },
          { token: 'number', foreground: 'ffd965' },
          { token: 'constant', foreground: 'ffd965' },
          { token: 'type', foreground: '8064a2' },
          { token: 'type.identifier', foreground: '70ad47' },
          { token: 'function', foreground: '5b9bd5' },
          { token: 'variable', foreground: 'ffffff' },
          { token: 'variable.parameter', foreground: 'b2a1c7' },
          { token: 'operator', foreground: '4472c4' },
          { token: 'delimiter', foreground: '4472c4' },
          { token: 'tag', foreground: '5b9bd5' },
          { token: 'attribute.name', foreground: '70ad47' },
          { token: 'attribute.value', foreground: 'ed7d31' },
          { token: 'metatag', foreground: 'a5a5a5' },
        ],
        colors: {
          'editor.background': '#18181e',
          'editor.foreground': '#f6f6f6',
          'editor.lineHighlightBackground': '#24242a40',
          'editor.selectionBackground': '#4f4f4f',
          'editorCursor.foreground': '#f97316',
          'editor.inactiveSelectionBackground': '#4f4f4f50',
          'editorLineNumber.foreground': '#5a5a65',
          'editorLineNumber.activeForeground': '#d4d4d8',
          'editorGutter.background': '#18181e',
          'editorWidget.background': '#1f1f25',
          'editorWidget.border': '#37373f',
          'scrollbarSlider.background': '#37373f80',
          'scrollbarSlider.hoverBackground': '#4a4a5280',
        },
      })
      monacoModule.editor.defineTheme('quantcode-light', {
        base: 'vs',
        inherit: true,
        rules: [
          { token: 'comment', foreground: 'a5a5a5', fontStyle: 'italic' },
          { token: 'keyword', foreground: '4472c4' },
          { token: 'keyword.operator', foreground: '4472c4' },
          { token: 'string', foreground: 'ed7d31' },
          { token: 'string.escape', foreground: 'f4b183' },
          { token: 'regexp', foreground: 'f4b183' },
          { token: 'number', foreground: 'e0a903' },
          { token: 'constant', foreground: 'e0a903' },
          { token: 'type', foreground: '8064a2' },
          { token: 'type.identifier', foreground: '70ad47' },
          { token: 'function', foreground: '5b9bd5' },
          { token: 'variable', foreground: '000000' },
          { token: 'variable.parameter', foreground: 'b2a1c7' },
          { token: 'operator', foreground: '4472c4' },
          { token: 'delimiter', foreground: '4472c4' },
          { token: 'tag', foreground: '5b9bd5' },
          { token: 'attribute.name', foreground: '70ad47' },
          { token: 'attribute.value', foreground: 'ed7d31' },
          { token: 'metatag', foreground: 'a5a5a5' },
        ],
        colors: {
          'editor.background': '#d8d8de',
          'editor.foreground': '#24242c',
          'editor.lineHighlightBackground': '#d0d0d640',
          'editor.selectionBackground': '#a0a0a830',
          'editorCursor.foreground': '#f97316',
          'editorLineNumber.foreground': '#888892',
          'editorLineNumber.activeForeground': '#24242c',
          'editorGutter.background': '#d8d8de',
          'editorWidget.background': '#e0e0e5',
          'editorWidget.border': '#babac2',
        },
      })
    } catch {
      // Themes already defined
    }

    editorInstance = monacoModule.editor.create(editorContainer.value, {
      value: content,
      language,
      theme: appStore.theme === 'light' ? 'quantcode-light' : 'quantcode-dark',
      fontSize: 13,
      fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
      fontLigatures: true,
      lineNumbers: 'on',
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      automaticLayout: false,
      padding: { top: 6, bottom: 6 },
      renderLineHighlight: 'line',
      cursorBlinking: 'smooth',
      cursorSmoothCaretAnimation: 'on',
      smoothScrolling: true,
      bracketPairColorization: { enabled: true },
      wordWrap: 'off',
      tabSize: 2,
      insertSpaces: true,
      scrollbar: {
        vertical: 'auto',
        horizontal: 'auto',
        verticalScrollbarSize: 8,
        horizontalScrollbarSize: 8,
      },
    })

    // Track content changes for dirty state + markdown preview
    editorInstance.onDidChangeModelContent(() => {
      if (ignoreContentChange) return
      isDirty.value = true
      updateTitle()
      // Update markdown preview live
      if (isMarkdown.value) {
        schedulePreviewUpdate()
      }
    })

    // Ctrl+S to save
    editorInstance.addAction({
      id: 'quantcode-save',
      label: 'Save File',
      keybindings: [monacoModule.KeyMod.CtrlCmd | monacoModule.KeyCode.KeyS],
      run: () => saveFile(),
    })

    // Layout after creation
    resizeObserver = new ResizeObserver(() => {
      editorInstance?.layout()
    })
    resizeObserver.observe(editorContainer.value)
    // Force initial layout
    await nextTick()
    editorInstance.layout()
  } catch (err) {
    console.error('Failed to init Monaco in FileWindow:', err)
  }
}

function updateTitle() {
  const wsId = workspacesStore.contentWorkspaceId
  if (!wsId) return
  const prefix = isDirty.value ? '\u25CF ' : ''
  canvasStore.updateWindow(wsId, props.window.id, {
    title: prefix + fileName.value,
  })
}

async function saveFile() {
  if (!fileConfig.value || !editorInstance) return
  try {
    const content = editorInstance.getValue()
    await invoke('plugin:canvas|write_file', {
      path: fileConfig.value.filePath,
      content,
    })
    isDirty.value = false
    updateTitle()
    appStore.notifyFileSaved(fileConfig.value.filePath, content)
  } catch (e) {
    console.error('Failed to save file:', e)
  }
}

async function loadFile() {
  if (!fileConfig.value) return

  // A pending preview update belongs to the old content — it would read the
  // disposed (or not yet rebuilt) editor and blank the fresh preview.
  if (previewTimer) {
    clearTimeout(previewTimer)
    previewTimer = null
  }

  loading.value = true
  error.value = ''
  imageSrc.value = ''
  markdownHtml.value = ''
  isDirty.value = false

  try {
    const kind = fileConfig.value.previewKind

    if (kind === 'image') {
      const result = await invoke<{ base64Data: string; mimeType: string; sizeBytes: number }>('plugin:canvas|read_file_binary', {
        path: fileConfig.value.filePath,
      })
      imageSrc.value = `data:${result.mimeType};base64,${result.base64Data}`
      loading.value = false
    } else if (kind === 'code' || kind === 'markdown' || kind === 'text') {
      const content = await invoke<string>('plugin:canvas|read_file', {
        path: fileConfig.value.filePath,
      })
      if (kind === 'markdown') {
        markdownHtml.value = await marked.parse(content)
      }
      // Set loading false so the editor container is visible (v-show), then init Monaco
      loading.value = false
      await nextTick()
      await initMonaco(content, fileConfig.value.language ?? 'plaintext')
    } else {
      // Binary
      loading.value = false
    }
  } catch (e) {
    error.value = String(e)
    loading.value = false
  }
}

// When toggling from preview back to editor, relayout Monaco
watch(showPreview, (preview) => {
  if (!preview && editorInstance) {
    nextTick(() => editorInstance.layout())
  }
})

onMounted(() => {
  loadFile()
})

watch(() => fileConfig.value?.filePath, () => {
  editorInstance?.dispose()
  editorInstance = null
  resizeObserver?.disconnect()
  resizeObserver = null
  loadFile()
})

// Sync from external saves (sidebar editor or other canvas windows)
watch(() => appStore.lastSavedFile, async (saved) => {
  if (!saved || !fileConfig.value) return
  if (!appStore.pathsMatch(fileConfig.value.filePath, saved.filePath)) return
  // Update Monaco if it exists and content differs
  if (editorInstance && editorInstance.getValue() !== saved.content) {
    ignoreContentChange = true
    editorInstance.setValue(saved.content)
    ignoreContentChange = false
  }
  isDirty.value = false
  updateTitle()
  // Update markdown preview if applicable
  if (isMarkdown.value) {
    markdownHtml.value = await marked.parse(saved.content)
  }
})

// Close this canvas window when the file is deleted
watch(() => appStore.lastDeletedFile, (deleted) => {
  if (!deleted || !fileConfig.value) return
  if (!appStore.pathsMatch(fileConfig.value.filePath, deleted.filePath)) return
  const wsId = workspacesStore.contentWorkspaceId
  if (wsId) {
    canvasStore.removeWindow(wsId, props.window.id)
  }
})

watch(() => appStore.theme, (t: Theme) => {
  if (monacoModule) {
    monacoModule.editor.setTheme(t === 'light' ? 'quantcode-light' : 'quantcode-dark')
  }
})

onUnmounted(() => {
  if (previewTimer) clearTimeout(previewTimer)
  editorInstance?.dispose()
  resizeObserver?.disconnect()
})
</script>

<template>
  <div class="flex flex-col h-full overflow-hidden" :style="{ background: 'var(--qc-bg-window)' }">
    <!-- File path bar with controls -->
    <div
      class="flex items-center gap-2 px-3 py-1.5 flex-shrink-0 text-[10px]"
      :style="{ background: 'var(--qc-bg-surface)', borderBottom: '1px solid var(--qc-border)', color: 'var(--qc-text-muted)' }"
    >
      <span class="opacity-60 truncate flex-1">{{ fileConfig?.filePath }}</span>

      <!-- Save button (when dirty) -->
      <button
        v-if="isDirty"
        class="flex-shrink-0 px-1.5 py-0.5 rounded transition-colors"
        :style="{ background: 'var(--qc-bg-window)', border: '1px solid var(--qc-border)', color: '#4ade80' }"
        title="Save (Ctrl+S)"
        @click.stop="saveFile"
      >
        Save
      </button>

      <!-- Preview / Source toggle for markdown -->
      <button
        v-if="isMarkdown && !loading && !error"
        class="flex-shrink-0 px-1.5 py-0.5 rounded transition-colors"
        :style="{
          background: 'var(--qc-bg-window)',
          border: '1px solid var(--qc-border)',
          color: 'var(--qc-text-muted)',
        }"
        :title="showPreview ? 'Edit source' : 'Show preview'"
        @click.stop="showPreview = !showPreview"
      >
        {{ showPreview ? '&lt;/&gt;' : 'Preview' }}
      </button>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="flex-1 flex items-center justify-center" :style="{ color: 'var(--qc-text-muted)' }">
      <span class="text-xs">Loading...</span>
    </div>

    <!-- Error -->
    <div v-else-if="error" class="flex-1 flex items-center justify-center p-4">
      <div class="text-center">
        <div class="text-red-400 text-xs mb-2">Failed to load file</div>
        <div class="text-[10px] max-w-[300px] break-words" :style="{ color: 'var(--qc-text-muted)' }">{{ error }}</div>
      </div>
    </div>

    <!-- Image preview -->
    <div
      v-else-if="isImage"
      class="flex-1 min-h-0 overflow-auto flex items-center justify-center p-4"
      :style="{ background: 'repeating-conic-gradient(var(--qc-bg-surface) 0% 25%, var(--qc-bg) 0% 50%) 50% / 16px 16px' }"
    >
      <img
        :src="imageSrc"
        :alt="fileName"
        class="max-w-full max-h-full object-contain"
        draggable="false"
      />
    </div>

    <!-- Markdown rendered preview (shown when in preview mode) -->
    <div
      v-else-if="isMarkdown && showPreview"
      class="flex-1 min-h-0 overflow-auto markdown-body p-4"
      v-html="markdownHtml"
    />

    <!-- Binary / unsupported -->
    <div
      v-else-if="!isEditable"
      class="flex-1 flex items-center justify-center p-4"
    >
      <div class="text-center">
        <div class="text-3xl mb-3 opacity-30">&#128196;</div>
        <div class="text-xs mb-1" :style="{ color: 'var(--qc-text)' }">{{ fileName }}</div>
        <div class="text-[10px]" :style="{ color: 'var(--qc-text-muted)' }">Binary file &mdash; no preview available</div>
      </div>
    </div>

    <!--
      Monaco editor container: always in DOM via v-show so Monaco can mount.
      Visible for code/text, and for markdown when not in preview mode.
    -->
    <div
      v-show="!loading && !error && showEditor"
      ref="editorContainer"
      class="flex-1 min-h-0"
    />
  </div>
</template>

<style scoped>
/* Markdown rendered output styling */
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
