<script setup lang="ts">
/**
 * The workbench page — tabs, the editor (or a version compare), the check
 * panel, a status line. One page: every script opens as a tab here, the way
 * a Pine editor holds its scripts (docs/PLAN-QUANTSCRIPT.md). The editor's
 * squiggles come from the lint the store runs while the user types.
 */
import type { EditorMarker } from '@quantsuite/ui'
import { useWorkbenchStore } from '#script/stores/workbench'
import { timeAgo } from '#script/utils/format'

definePageMeta({ layout: 'script' })

const wb = useWorkbenchStore()

const editorRef = ref<{ reveal: (line: number, column?: number) => void; focus: () => void } | null>(null)
const editorReady = ref(false)
const showLibrary = computed(() => wb.libraryOpen || !wb.active)

// Capture before Monaco consumes Ctrl+Enter as Insert Line Below.
function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && !showLibrary.value && !wb.newScriptOpen) {
    e.preventDefault()
    e.stopPropagation()
    void wb.check()
  }
}

/** The lint's markers in the editor's shape. */
const diagnostics = computed<EditorMarker[]>(() => {
  const o = wb.active
  if (!o?.lint) return []
  return o.lint.markers.map((m) => ({
    severity: m.severity,
    message: m.message,
    line: m.line,
    column: m.column,
    endLine: m.end_line ?? undefined,
    endColumn: m.end_column ?? undefined,
    source: m.source,
    path: o.path,
  }))
})
const errorCount = computed(() => diagnostics.value.filter((d) => d.severity === 'error').length)
const warningCount = computed(() => diagnostics.value.filter((d) => d.severity === 'warning').length)

function jumpToFirstProblem() {
  wb.showResults('problems')
  const first = diagnostics.value.find((d) => d.severity === 'error') ?? diagnostics.value[0]
  if (first && wb.active) wb.reveal(wb.active.file, first.line)
}

function onInput(text: string) {
  if (wb.active) wb.setContent(wb.active.file, text)
}

function onSave() {
  void wb.save()
}

function onCursor(pos: { line: number; column: number }) {
  wb.cursor = pos
}

function onReady() {
  wb.cursor = { line: 1, column: 1 }
  editorReady.value = true
  applyReveal()
}

/** The sidebar and the context panel ask for a line; the editor may still
 *  be mounting, so the request waits for `ready` and for the right tab. */
function applyReveal() {
  const req = wb.revealRequest
  if (!req || !editorReady.value || !wb.active || wb.active.file !== req.file || wb.active.compare || showLibrary.value) return
  nextTick(() => {
    if (wb.revealRequest !== req || wb.activeFile !== req.file || showLibrary.value) return
    editorRef.value?.reveal(req.line)
    wb.revealRequest = null
  })
}

watch(() => wb.revealRequest, applyReveal)
watch(
  () => [wb.activeFile, wb.active?.compare, showLibrary.value] as const,
  () => {
    editorReady.value = false
    applyReveal()
  },
)

async function restoreCompared() {
  const o = wb.active
  if (!o?.compare) return
  const v = o.compare.meta.version
  if (o.content !== o.saved && !confirm(`Restore v${v}? The unsaved changes in the editor are replaced.`)) return
  await wb.restore(o.file, v)
}
</script>

<template>
  <div class="qsc-page" @keydown.capture="onKeydown">
    <ScriptEditorTabs />
    <ScriptEditorCommandBar v-if="!showLibrary" />

    <div class="qsc-stage">
      <ScriptEditorLibrary v-if="showLibrary" />
      <template v-else-if="wb.active">
        <div v-if="wb.active.compare" class="qsc-compare">
          <div class="qsc-compare-bar">
            <span class="qsc-compare-left">
              <span class="mono">v{{ wb.active.compare.meta.version }}</span>
              <span class="muted">{{ timeAgo(wb.active.compare.meta.created_at) }} · {{ wb.active.compare.meta.author }}</span>
              <span class="qsc-compare-msg muted">{{ wb.active.compare.meta.message }}</span>
            </span>
            <span class="muted">vs. the editor{{ wb.active.content !== wb.active.saved ? ' (unsaved)' : '' }}</span>
            <span class="qsc-compare-actions">
              <button
                v-if="wb.active.compare.meta.version !== wb.active.version?.version"
                class="qsc-btn is-sm"
                :disabled="wb.active.saving"
                @click="restoreCompared"
              >
                Restore v{{ wb.active.compare.meta.version }}
              </button>
              <button class="qsc-btn is-sm is-ghost" @click="wb.closeCompare(wb.active.file)">Back to the editor</button>
            </span>
          </div>
          <div class="qsc-editor">
            <QDiffEditor :original="wb.active.compare.content" :modified="wb.active.content" language="python" tokens="qss" />
          </div>
        </div>
        <div v-else class="qsc-editor">
          <QCodeEditor
            ref="editorRef"
            :key="wb.active.file"
            :model-value="wb.active.content"
            :path="wb.active.path"
            language="python"
            :read-only="!wb.active.editable"
            :minimap="wb.minimap"
            :word-wrap="wb.wordWrap"
            :diagnostics="diagnostics"
            tokens="qss"
            @update:model-value="onInput"
            @save="onSave"
            @cursor="onCursor"
            @ready="onReady"
          />
        </div>
      </template>

    </div>

    <ScriptEditorProblems v-if="!showLibrary && wb.active?.editable" />

    <footer v-if="!showLibrary" class="qsc-status">
      <template v-if="wb.active">
        <span class="mono">Ln {{ wb.cursor.line }}, Col {{ wb.cursor.column }}</span>
        <span class="qsc-status-sep" />
        <span>Python</span>
        <span class="qsc-status-sep" />
        <button
          v-if="wb.active.editable"
          class="qsc-link qsc-status-problems"
          :class="{ 'is-error': errorCount > 0, 'is-warn': !errorCount && warningCount > 0 }"
          :title="wb.active.linting ? 'Linting…' : diagnostics.length ? 'Jump to the first problem' : 'No problems'"
          @click="jumpToFirstProblem"
        >
          <template v-if="errorCount">{{ errorCount }} error{{ errorCount === 1 ? '' : 's' }}</template>
          <template v-else-if="warningCount">{{ warningCount }} warning{{ warningCount === 1 ? '' : 's' }}</template>
          <template v-else>{{ wb.active.lintError ? 'Lint unavailable' : !wb.python?.ok ? 'Python unavailable' : wb.active.linting || !wb.active.lint ? 'Lint pending…' : 'No problems' }}</template>
        </button>
        <span class="qsc-status-spacer" />
        <button class="qsc-link" :aria-pressed="wb.wordWrap" @click="wb.wordWrap = !wb.wordWrap">Wrap {{ wb.wordWrap ? 'on' : 'off' }}</button>
        <button class="qsc-link" :aria-pressed="wb.minimap" @click="wb.minimap = !wb.minimap">Minimap {{ wb.minimap ? 'on' : 'off' }}</button>
      </template>
      <template v-else>
        <span class="muted">No script open</span>
      </template>
    </footer>
  </div>
</template>

<style scoped>
.qsc-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.qsc-stage {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  position: relative;
}

.qsc-editor {
  flex: 1;
  min-height: 0;
  position: relative;
}

.qsc-compare {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.qsc-compare-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 36px;
  flex-wrap: wrap;
  padding: 6px 12px;
  border-bottom: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
  font-size: 12px;
  flex-shrink: 0;
}
.qsc-compare-bar > * {
  white-space: nowrap;
}
.qsc-compare-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  overflow: hidden;
}
.qsc-compare-left > * {
  flex-shrink: 0;
}
.qsc-compare-left .mono {
  font-weight: 600;
}
/* The message is what gives way in a narrow stage; the version and the
   actions never do. */
.qsc-compare-msg {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.qsc-compare-actions {
  margin-left: auto;
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
@container (max-width: 620px) {
  .qsc-compare-left { flex-basis: 100%; }
  .qsc-compare-actions { flex-wrap: wrap; }
}

.qsc-status {
  overflow-x: auto;
  white-space: nowrap;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 26px;
  padding: 0 12px;
  border-top: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
  color: var(--qss-text-secondary);
  font-size: 11px;
  flex-shrink: 0;
}
.qsc-status-sep {
  width: 1px;
  height: 12px;
  background: var(--qss-border);
}
.qsc-status > * { flex-shrink: 0; }
.qsc-status-spacer {
  flex: 1;
}
.qsc-status-dirty {
  color: var(--qss-warning);
}
.qsc-status-problems {
  font-size: 11px;
  color: var(--qss-text-secondary);
  text-decoration: none;
}
.qsc-status-problems.is-error {
  color: var(--qss-error);
}
.qsc-status-problems.is-warn {
  color: var(--qss-warning);
}
</style>
