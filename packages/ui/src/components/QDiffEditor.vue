<script setup lang="ts">
/**
 * The suite's Monaco diff surface — two texts side by side, read-only.
 *
 * Written for QuantScript's version history (compare a recorded version
 * with the editor's buffer, then restore it) and kept next to `QCodeEditor`
 * so both share one theme (`../monaco-theme`) and one set of quirks.
 *
 * **Host-agnostic and model-private.** `QCodeEditor` keys its models by
 * `file:` URI and shares them process-wide; a diff must never touch those
 * (writing the original into the file's model would overwrite the buffer
 * the editor shows), so this component creates two models of its own under
 * a `quantsuite-diff:` scheme and disposes them on unmount. Their URIs are
 * unique across instances (two diffs can be mounted at once, one per warm
 * module) and carry a path: the TypeScript worker cannot resolve a URI
 * without one and throws on every diagnostics pass.
 */
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { defineQuantsuiteTheme, EDITOR_FONT, THEME_NAME, type MonacoTheme, type MonacoTokens } from '../monaco-theme'

const props = withDefaults(
  defineProps<{
    /** The left side — what the text was. */
    original: string
    /** The right side — what it is now. */
    modified: string
    /** Monaco language id for both sides. */
    language?: string
    theme?: MonacoTheme
    tokens?: MonacoTokens
    /** Side by side (default) or inline. */
    sideBySide?: boolean
    /** Wrap long lines on both sides. */
    wordWrap?: boolean
  }>(),
  {
    language: 'plaintext',
    theme: 'dark',
    tokens: 'qss',
    sideBySide: true,
    wordWrap: false,
  }
)

const container = ref<HTMLElement | null>(null)
const editor = shallowRef<any>(null)
const monaco = shallowRef<any>(null)
let originalModel: any = null
let modifiedModel: any = null
let destroyed = false

async function init() {
  if (!container.value) return
  try {
    const { installMonacoEnvironment } = await import('../monaco-env')
    installMonacoEnvironment()
    monaco.value = await import('monaco-editor')
    if (destroyed || !container.value) return
    const m = monaco.value
    defineQuantsuiteTheme(m, container.value, props.tokens, props.theme)

    const id = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
    originalModel = m.editor.createModel(props.original, props.language, m.Uri.parse(`quantsuite-diff://diff/${id}/original`))
    modifiedModel = m.editor.createModel(props.modified, props.language, m.Uri.parse(`quantsuite-diff://diff/${id}/modified`))

    editor.value = m.editor.createDiffEditor(container.value, {
      ...EDITOR_FONT,
      theme: THEME_NAME[props.theme],
      readOnly: true,
      originalEditable: false,
      renderSideBySide: props.sideBySide,
      automaticLayout: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      renderOverviewRuler: true,
      ignoreTrimWhitespace: false,
      lineNumbers: 'on',
      padding: { top: 8, bottom: 8 },
      diffWordWrap: props.wordWrap ? 'on' : 'off',
      wordWrap: props.wordWrap ? 'on' : 'off',
    })
    editor.value.setModel({ original: originalModel, modified: modifiedModel })

    if (typeof document !== 'undefined' && document.fonts?.ready) {
      void document.fonts.ready.then(() => {
        monaco.value?.editor.remeasureFonts()
        editor.value?.layout()
      })
    }
  } catch (e) {
    console.error('[QDiffEditor] Monaco failed to load', e)
  }
}

onMounted(init)

onBeforeUnmount(() => {
  destroyed = true
  editor.value?.dispose()
  originalModel?.dispose()
  modifiedModel?.dispose()
  originalModel = null
  modifiedModel = null
})

watch(
  () => props.original,
  (value) => {
    if (originalModel && originalModel.getValue() !== value) originalModel.setValue(value)
  }
)
watch(
  () => props.modified,
  (value) => {
    if (modifiedModel && modifiedModel.getValue() !== value) modifiedModel.setValue(value)
  }
)
watch(
  () => props.language,
  (language) => {
    const m = monaco.value
    if (!m) return
    if (originalModel) m.editor.setModelLanguage(originalModel, language)
    if (modifiedModel) m.editor.setModelLanguage(modifiedModel, language)
  }
)
watch(
  () => props.theme,
  () => {
    if (monaco.value) defineQuantsuiteTheme(monaco.value, container.value, props.tokens, props.theme)
  }
)
watch(
  () => props.sideBySide,
  (sideBySide) => editor.value?.updateOptions({ renderSideBySide: sideBySide })
)

watch(
  () => props.wordWrap,
  (wrap) => editor.value?.updateOptions({ wordWrap: wrap ? 'on' : 'off', diffWordWrap: wrap ? 'on' : 'off' })
)

/** The selected lines of the right side: for review comments. */
function selection(): { startLine: number; endLine: number; text: string } | null {
  const right = editor.value?.getModifiedEditor()
  const sel = right?.getSelection()
  const model = right?.getModel()
  if (!sel || !model) return null
  const startLine = sel.startLineNumber
  const endLine = sel.endColumn === 1 && sel.endLineNumber > startLine ? sel.endLineNumber - 1 : sel.endLineNumber
  const lines: string[] = []
  for (let l = startLine; l <= endLine; l++) lines.push(model.getLineContent(l))
  return { startLine, endLine, text: lines.join('\n') }
}

defineExpose({
  layout: () => editor.value?.layout(),
  selection,
})
</script>

<template>
  <div ref="container" class="q-diff-editor" />
</template>

<style scoped>
.q-diff-editor {
  width: 100%;
  height: 100%;
  min-height: 0;
  min-width: 0;
}
</style>
