<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { useAppStore } from '#algo/stores/app'

const props = defineProps<{
  strategyId: string
  content: string
}>()

const emit = defineEmits<{
  'update:content': [value: string]
}>()

const app = useAppStore()
const containerRef = ref<HTMLDivElement | null>(null)

let editor: import('monaco-editor').editor.IStandaloneCodeEditor | null = null
let monaco: typeof import('monaco-editor') | null = null
let isUpdatingFromProp = false

const DARK_THEME = 'quantalgo-dark'
const LIGHT_THEME = 'quantalgo-light'

function defineThemes(m: typeof import('monaco-editor')) {
  m.editor.defineTheme(DARK_THEME, {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: '', foreground: 'd4d4d8', background: '1f1f25' },
      { token: 'comment', foreground: '6e6e7a', fontStyle: 'italic' },
      { token: 'keyword', foreground: 'a0a0a8' },
      { token: 'string', foreground: '2ed573' },
      { token: 'number', foreground: 'ffa502' },
      { token: 'type', foreground: 'b8b8c0' },
    ],
    colors: {
      'editor.background': '#1f1f25',
      'editor.foreground': '#d4d4d8',
      'editor.selectionBackground': '#313139',
      'editor.lineHighlightBackground': '#292930',
      'editorLineNumber.foreground': '#6e6e7a',
      'editorLineNumber.activeForeground': '#9a9aa5',
      'editorCursor.foreground': '#d4d4d8',
      'editorIndentGuide.background': '#292930',
      'editorIndentGuide.activeBackground': '#47474f',
      'editor.selectionHighlightBackground': '#313139',
      'editorWidget.background': '#292930',
      'editorWidget.border': '#47474f',
      'input.background': '#18181e',
      'input.border': '#47474f',
      'input.foreground': '#d4d4d8',
      'scrollbarSlider.background': '#47474f80',
      'scrollbarSlider.hoverBackground': '#5a5a6480',
      'scrollbarSlider.activeBackground': '#5a5a64',
    },
  })

  m.editor.defineTheme(LIGHT_THEME, {
    base: 'vs',
    inherit: true,
    rules: [
      { token: '', foreground: '24242c', background: 'e0e0e5' },
      { token: 'comment', foreground: '6a6a78', fontStyle: 'italic' },
      { token: 'keyword', foreground: '4a4a52' },
      { token: 'string', foreground: '1a8a4a' },
      { token: 'number', foreground: 'b07000' },
      { token: 'type', foreground: '3a3a42' },
    ],
    colors: {
      'editor.background': '#e0e0e5',
      'editor.foreground': '#24242c',
      'editor.selectionBackground': '#c8c8d0',
      'editor.lineHighlightBackground': '#e6e6ec',
      'editorLineNumber.foreground': '#6a6a78',
      'editorLineNumber.activeForeground': '#484856',
      'editorCursor.foreground': '#24242c',
      'editorIndentGuide.background': '#c8c8d0',
      'editorIndentGuide.activeBackground': '#a5a5af',
      'editor.selectionHighlightBackground': '#c8c8d0',
      'editorWidget.background': '#e6e6ec',
      'editorWidget.border': '#a5a5af',
      'input.background': '#d8d8de',
      'input.border': '#a5a5af',
      'input.foreground': '#24242c',
      'scrollbarSlider.background': '#a5a5af80',
      'scrollbarSlider.hoverBackground': '#8a8a9480',
      'scrollbarSlider.activeBackground': '#8a8a94',
    },
  })
}

function currentThemeName(): string {
  return app.settings.theme === 'light' ? LIGHT_THEME : DARK_THEME
}

onMounted(async () => {
  if (!containerRef.value) return

  monaco = await import('monaco-editor')
  defineThemes(monaco)

  editor = monaco.editor.create(containerRef.value, {
    value: props.content,
    language: 'python',
    theme: currentThemeName(),
    fontSize: 14,
    fontFamily: "ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, monospace",
    minimap: { enabled: true, maxColumn: 60, scale: 1 },
    lineNumbers: 'on',
    wordWrap: 'off',
    scrollBeyondLastLine: false,
    automaticLayout: true,
    tabSize: 4,
    insertSpaces: true,
    renderLineHighlight: 'line',
    smoothScrolling: true,
    cursorSmoothCaretAnimation: 'on',
    padding: { top: 12, bottom: 12 },
    bracketPairColorization: { enabled: true },
    guides: { indentation: true, bracketPairs: true },
  })

  editor.onDidChangeModelContent(() => {
    if (isUpdatingFromProp) return
    const value = editor?.getValue() ?? ''
    emit('update:content', value)
  })
})

// Sync prop changes into the editor
watch(
  () => props.content,
  (newVal) => {
    if (!editor) return
    const current = editor.getValue()
    if (current !== newVal) {
      isUpdatingFromProp = true
      editor.setValue(newVal)
      isUpdatingFromProp = false
    }
  },
)

// Switch editor theme when app theme changes
watch(
  () => app.settings.theme,
  () => {
    if (monaco) {
      monaco.editor.setTheme(currentThemeName())
    }
  },
)

onBeforeUnmount(() => {
  if (editor) {
    editor.dispose()
    editor = null
  }
})
</script>

<template>
  <div ref="containerRef" class="strategy-editor" />
</template>

<style scoped>
.strategy-editor {
  width: 100%;
  height: 100%;
  min-height: 400px;
}
</style>
