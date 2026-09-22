<script setup lang="ts">
/**
 * The one Monaco surface in the suite.
 *
 * Written for QuantCanvas' right-hand editor panel and extracted here on
 * 2026-08-26 (docs/PLAN-QUANTSPACE.md) when QuantCode needed the same editor:
 * one theme definition, one set of options, one place where a Monaco quirk gets
 * fixed. Two copies would have drifted the moment one of them got a bug fix.
 *
 * **Host-agnostic.** It owns no tabs, no files and no store. The host says
 * which path is open and what its text is; the editor says when the text, the
 * cursor or the diagnostics changed, and when the user asked to save.
 *
 * **One model per path.** Monaco models are keyed by URI and shared process-
 * wide, so opening the same file in two editor groups — or in QuantCanvas and
 * QuantCode at once — gives one buffer with one undo history, which is what a
 * user expects and what `setValue` on a single model can never provide.
 *
 * Colours come from the host's CSS custom properties, read once per theme
 * change, so the editor sits inside `--qss-*` (QuantCode) and `--qc-*`
 * (QuantCanvas) surfaces without either host restyling it.
 */
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import type { EditorMarker } from '../editor-types'
import { defineQuantsuiteTheme, EDITOR_FONT, THEME_NAME } from '../monaco-theme'

const props = withDefaults(
  defineProps<{
    /** The buffer's text. The host stays the source of truth. */
    modelValue: string
    /** Absolute file path — the model's identity. Empty means a scratch buffer. */
    path?: string
    /** Monaco language id. Ignored for a path Monaco already recognises. */
    language?: string
    theme?: 'dark' | 'light'
    minimap?: boolean
    readOnly?: boolean
    wordWrap?: boolean
    /** Which CSS custom properties to read colours from. */
    tokens?: 'qss' | 'qc'
    /**
     * Diagnostics the HOST computed for this buffer — squiggles and hover
     * messages for a language Monaco has no worker for (QuantScript's
     * Python lint). Owned under one marker owner, replaced as a whole on
     * every change and cleared when the editor leaves the file, so they never
     * mix with Monaco's own TypeScript / JSON / CSS markers.
     */
    diagnostics?: EditorMarker[]
  }>(),
  {
    path: '',
    language: 'plaintext',
    theme: 'dark',
    minimap: true,
    readOnly: false,
    wordWrap: false,
    tokens: 'qss',
    diagnostics: () => [],
  }
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  /** Ctrl+S. The host writes the file — the editor does not touch disk. */
  (e: 'save', value: string): void
  (e: 'cursor', position: { line: number; column: number }): void
  /** Right-click on the gutter (line numbers / glyph margin). The host decides
   *  what that means — QuantCode toggles a bookmark. */
  (e: 'gutterContextMenu', target: { line: number }): void
  /** Monaco's own diagnostics for this model — feeds a problems panel. */
  (e: 'markers', markers: EditorMarker[]): void
  (e: 'ready'): void
}>()

const container = ref<HTMLElement | null>(null)
const editor = shallowRef<any>(null)
const monaco = shallowRef<any>(null)

/** Set while the editor writes text the host just gave it, so the echo does
 *  not come back as a user edit and mark a clean file dirty. */
let echoing = false
let markerDisposable: { dispose(): void } | null = null
/** Set on unmount. `init` is async (the Monaco import), and a session restore
 *  can replace the editor group while it is in flight — the ghost init must
 *  not run `modelFor`, or it writes its STALE `modelValue` into the shared
 *  model a freshly mounted editor is already showing (startup mismatch bug,
 *  2026-08-27). */
let destroyed = false

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/**
 * The palette itself lives in `../monaco-theme` since 2026-09-12, shared
 * with `QDiffEditor` (QuantScript's version compare): one definition, so an
 * editor and a diff next to each other read as one surface.
 */
function defineThemes() {
  if (!monaco.value) return
  defineQuantsuiteTheme(monaco.value, container.value, props.tokens, props.theme)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/**
 * A stable URI per file — `file:` for real paths, so Monaco's own language
 * detection and its TypeScript worker both recognise them. A scratch buffer
 * still needs a unique URI, or two of them would share one model.
 */
let scratchCounter = 0

function modelFor(path: string, value: string, language: string) {
  const m = monaco.value
  const uri = path ? m.Uri.file(path) : m.Uri.parse(`quantsuite://scratch-${++scratchCounter}`)

  const existing = m.editor.getModel(uri)
  if (existing) {
    if (existing.getValue() !== value) {
      echoing = true
      existing.setValue(value)
      echoing = false
    }
    return existing
  }

  // Only pass a language Monaco actually registers. Hosts hand us ids like
  // `typescriptreact`, which Monaco does not have — passing it through would
  // silently downgrade a .tsx file to plaintext, where letting Monaco read the
  // extension gets it right.
  const known = m.languages.getLanguages().some((l: { id: string }) => l.id === language)
  return m.editor.createModel(value, known ? language : undefined, uri)
}

/** The marker owner the host's diagnostics live under. */
const HOST_OWNER = 'quantsuite-host'

/** Write the host's diagnostics onto the current model — theirs alone. */
function applyDiagnostics() {
  const m = monaco.value
  const model = editor.value?.getModel()
  if (!m || !model) return
  const severity: Record<EditorMarker['severity'], number> = {
    error: m.MarkerSeverity.Error,
    warning: m.MarkerSeverity.Warning,
    info: m.MarkerSeverity.Info,
    hint: m.MarkerSeverity.Hint,
  }
  const lines = model.getLineCount()
  m.editor.setModelMarkers(
    model,
    HOST_OWNER,
    props.diagnostics.map((d) => {
      const line = Math.min(Math.max(1, d.line), lines)
      const column = Math.max(1, d.column)
      const endLine = Math.min(Math.max(line, d.endLine ?? line), lines)
      // A marker without a span underlines to the end of its line — visible,
      // instead of a one-character nick nobody notices.
      const endColumn = d.endColumn ?? (endLine === line ? Math.max(column + 1, model.getLineMaxColumn(line)) : 1)
      return {
        severity: severity[d.severity],
        message: d.message,
        source: d.source,
        startLineNumber: line,
        startColumn: column,
        endLineNumber: endLine,
        endColumn,
      }
    })
  )
}

function clearDiagnostics() {
  const m = monaco.value
  const model = editor.value?.getModel()
  if (m && model) m.editor.setModelMarkers(model, HOST_OWNER, [])
}

function collectMarkers() {
  const m = monaco.value
  const model = editor.value?.getModel()
  if (!m || !model) return
  const severity: Record<number, EditorMarker['severity']> = {
    8: 'error',
    4: 'warning',
    2: 'info',
    1: 'hint',
  }
  emit(
    'markers',
    m.editor.getModelMarkers({ resource: model.uri }).map((k: any) => ({
      severity: severity[k.severity] ?? 'info',
      message: k.message,
      line: k.startLineNumber,
      column: k.startColumn,
      source: k.source,
      path: props.path,
    }))
  )
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

const editorOptions = computed(() => ({
  ...EDITOR_FONT,
  lineNumbers: 'on' as const,
  minimap: { enabled: props.minimap, maxColumn: 80 },
  readOnly: props.readOnly,
  wordWrap: (props.wordWrap ? 'on' : 'off') as 'on' | 'off',
  scrollBeyondLastLine: false,
  // Monaco's own ResizeObserver. Driving `layout()` from an observer of our
  // own looked cheaper and was simply wrong: the editor measured once, at a
  // moment the container could still be zero-sized, and then painted nothing
  // into a correctly sized box.
  automaticLayout: true,
  padding: { top: 8, bottom: 8 },
  renderLineHighlight: 'line' as const,
  cursorBlinking: 'smooth' as const,
  cursorSmoothCaretAnimation: 'on' as const,
  smoothScrolling: true,
  bracketPairColorization: { enabled: true },
  tabSize: 2,
  insertSpaces: true,
  // Suggest/hover/find widgets escape the editor box so they are not clipped
  // by a narrow split pane. Monaco implements that with `position: fixed` and
  // VIEWPORT coordinates — see `overflowHost` for why that needs a DOM node of
  // our own.
  fixedOverflowWidgets: true,
}))

/**
 * Where Monaco parks its overflowing widgets.
 *
 * `fixedOverflowWidgets` positions the suggest widget `fixed`, at coordinates
 * measured against the viewport. The shell's module panel carries
 * `transform: translateZ(0)` (ModuleStage.vue — modules' own `position: fixed`
 * pills anchor to the panel, deliberately), and a transformed ancestor becomes
 * the containing block for `fixed` descendants. Monaco's viewport coordinates
 * were then resolved against the PANEL, so every popup landed one rail width
 * (60px) right and one titlebar height (38px) below the cursor — the
 * "suggestions float away from the caret" bug, 2026-08-27.
 *
 * Hosting the widgets on `document.body` puts them back outside that transform,
 * where Monaco's own math is correct. One host per editor: `View.dispose()`
 * only detaches the content-widget node, so the host is ours to remove.
 *
 * `monaco-editor` is not decoration — the standalone theme service defines the
 * `--vscode-*` colour variables on that class, so an unclassed host renders an
 * unstyled widget.
 */
let overflowHost: HTMLElement | null = null

function themeSelector() {
  return props.theme === 'dark' ? 'vs-dark' : 'vs'
}

function createOverflowHost() {
  const el = document.createElement('div')
  el.className = `monaco-editor ${themeSelector()}`
  // Zero-sized and out of flow: only the `fixed` children it holds are ever
  // visible. Above the module chrome, below the shell's modals (9999/10000).
  el.style.cssText = 'position:absolute;top:0;left:0;width:0;height:0;z-index:2000;'
  document.body.appendChild(el)
  return el
}

async function init() {
  if (!container.value) return
  try {
    // The worker factory is registered by the shell's `monaco.client` plugin,
    // early enough for every Monaco surface in the suite. Called again here
    // (it is idempotent) so this component still works in a host that has no
    // such plugin.
    const { installMonacoEnvironment } = await import('../monaco-env')
    installMonacoEnvironment()

    monaco.value = await import('monaco-editor')
    if (destroyed) return
    defineThemes()

    overflowHost = createOverflowHost()

    editor.value = monaco.value.editor.create(container.value, {
      ...editorOptions.value,
      model: modelFor(props.path, props.modelValue, props.language),
      theme: THEME_NAME[props.theme],
      overflowWidgetsDomNode: overflowHost,
    })

    editor.value.onDidChangeModelContent(() => {
      if (echoing) return
      emit('update:modelValue', editor.value.getValue())
    })

    editor.value.onDidChangeCursorPosition((e: any) => {
      emit('cursor', { line: e.position.lineNumber, column: e.position.column })
    })

    // Right-click on the gutter. Handled on mousedown so the action fires
    // whether or not a context menu would have appeared, and the contextmenu
    // event itself is swallowed so none does.
    const isGutter = (type: number) => {
      const T = monaco.value.editor.MouseTargetType
      return (
        type === T.GUTTER_GLYPH_MARGIN ||
        type === T.GUTTER_LINE_NUMBERS ||
        type === T.GUTTER_LINE_DECORATIONS
      )
    }
    editor.value.onMouseDown((e: any) => {
      if (e.event?.rightButton && isGutter(e.target?.type) && e.target?.position) {
        e.event.preventDefault()
        e.event.stopPropagation()
        emit('gutterContextMenu', { line: e.target.position.lineNumber })
      }
    })
    editor.value.onContextMenu((e: any) => {
      if (isGutter(e.target?.type)) {
        e.event.preventDefault()
        e.event.stopPropagation()
      }
    })

    editor.value.addAction({
      id: 'quantsuite-save',
      label: 'Save File',
      keybindings: [monaco.value.KeyMod.CtrlCmd | monaco.value.KeyCode.KeyS],
      run: () => emit('save', editor.value.getValue()),
    })

    markerDisposable = monaco.value.editor.onDidChangeMarkers(() => collectMarkers())
    collectMarkers()

    // The import above took real time. Prop changes that arrived while it was
    // in flight were dropped by the watchers (guarded on `editor.value`), and
    // the create() call may have caught any intermediate state of them — at
    // startup a session restore flips the active tab several times, and the
    // editor was observed ending up on a model its own props no longer named
    // (the "header says A, editor shows B" bug, 2026-08-27). One hard re-sync
    // against the props AS THEY ARE NOW closes every such window.
    const wantedUri = props.path ? monaco.value.Uri.file(props.path).toString() : null
    if (wantedUri && editor.value.getModel()?.uri.toString() !== wantedUri) {
      editor.value.setModel(modelFor(props.path, props.modelValue, props.language))
      collectMarkers()
    }
    applyDiagnostics()

    // Monaco sizes every glyph from a measurement it takes at creation time. On
    // a cold load that happens before the suite's webfont has arrived, the
    // measurement comes back zero, and the editor paints NOTHING into a
    // correctly sized box with a correctly loaded model — no error, no clue.
    // Re-measuring once the fonts settle is the fix, and it is cheap.
    if (typeof document !== 'undefined' && document.fonts?.ready) {
      void document.fonts.ready.then(() => {
        monaco.value?.editor.remeasureFonts()
        editor.value?.layout()
      })
    }

    emit('ready')
  } catch (e) {
    // Surfaced rather than swallowed: a failed Monaco import leaves an empty
    // grey box, which reads as "the editor is broken" with no clue why.
    console.error('[QCodeEditor] Monaco failed to load', e)
  }
}

onMounted(init)

onBeforeUnmount(() => {
  destroyed = true
  markerDisposable?.dispose()
  // The host's diagnostics belong to this editor's tenure on the file, not
  // to the shared model — another host showing it must not inherit them.
  clearDiagnostics()
  // The MODEL is deliberately not disposed: it is shared by path, and another
  // editor group (or another module) may still be showing this file.
  editor.value?.dispose()
  overflowHost?.remove()
  overflowHost = null
})

// Host swapped the open file.
watch(
  () => [props.path, props.language] as const,
  () => {
    if (!editor.value || !monaco.value) return
    clearDiagnostics()
    editor.value.setModel(modelFor(props.path, props.modelValue, props.language))
    collectMarkers()
    applyDiagnostics()
  }
)

// The host's diagnostics changed (a lint came back).
watch(() => props.diagnostics, applyDiagnostics, { deep: true })

// Host changed the text underneath us (a reverted file, an agent's write).
watch(
  () => props.modelValue,
  (value) => {
    const model = editor.value?.getModel()
    if (!model || value == null || model.getValue() === value) return
    // The value belongs to `props.path` — and during a tab switch both props
    // change in one flush, where this watcher has been observed running while
    // the editor still holds the PREVIOUS file's model. Writing then puts one
    // file's text into another file's buffer, which the other split's editor
    // happily echoes back to its host as an edit (found 2026-08-27, split
    // overwrite bug). The path watcher swaps the model and syncs the text
    // itself, so a model that is not `props.path` is not ours to write.
    if (props.path && model.uri.toString() !== monaco.value.Uri.file(props.path).toString()) return
    echoing = true
    model.setValue(value)
    echoing = false
  }
)

watch(() => props.theme, () => {
  defineThemes()
  // The host lives outside the editor, so Monaco never restyles it.
  if (overflowHost) overflowHost.className = `monaco-editor ${themeSelector()}`
})
watch(
  () => [props.minimap, props.readOnly, props.wordWrap],
  () => editor.value?.updateOptions(editorOptions.value)
)

defineExpose({
  focus: () => editor.value?.focus(),
  layout: () => editor.value?.layout(),
  /** Jump to a position — used by the problems panel and the search results. */
  reveal: (line: number, column = 1) => {
    editor.value?.revealLineInCenter(line)
    editor.value?.setPosition({ lineNumber: line, column })
    editor.value?.focus()
  },
})
</script>

<template>
  <div ref="container" class="q-code-editor" />
</template>

<style scoped>
.q-code-editor {
  width: 100%;
  height: 100%;
  min-height: 0;
  min-width: 0;
}
</style>
