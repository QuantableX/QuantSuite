<script setup lang="ts">
/**
 * One file in the Files tab (card 13): Monaco, read-only until Edit (Ctrl+S
 * saves; files outside the checkout stay read-only); Markdown, HTML, SVG
 * and CSV/TSV rendered with a Raw switch — HTML in a sandboxed frame
 * without scripts reaching the app; images and PDFs shown. Copy contents
 * and Save as on top.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { Copy, Download, Eye, Code2, Pencil, Save, X } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { languageOf, parseDelimited, renderedOf, extOf } from '#pilot/utils/files'
import { sizeLabel } from '#pilot/utils/composer'
import type { FileContent } from '#pilot/types'

const props = defineProps<{ threadId: string; path: string; line: number | null; nonce: number }>()

const file = ref<FileContent | null>(null)
const error = ref('')
const loading = ref(false)
const raw = ref(false)
const editing = ref(false)
const buffer = ref('')
const note = ref('')
const editor = ref<{ reveal: (line: number, column?: number) => void; layout: () => void } | null>(null)
let blobUrl: string | null = null

const rendered = computed(() => (file.value?.kind === 'text' ? renderedOf(props.path) : null))
const showRendered = computed(() => Boolean(rendered.value) && !raw.value && !editing.value)
const dirty = computed(() => editing.value && buffer.value !== (file.value?.text ?? ''))
const name = computed(() => props.path.split(/[\\/]/).pop() ?? props.path)
const table = computed(() => (rendered.value === 'csv' && file.value?.text ? parseDelimited(file.value.text, extOf(props.path) === 'tsv' ? '\t' : ',') : null))
const dataUrl = computed(() => (file.value?.base64 ? `data:${file.value.mime};base64,${file.value.base64}` : ''))
const svgUrl = computed(() => (rendered.value === 'svg' && file.value?.text ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(file.value.text)}` : ''))
const pdfUrl = ref('')

function say(text: string) {
  note.value = text
  setTimeout(() => note.value === text && (note.value = ''), 3500)
}

async function load() {
  loading.value = true
  error.value = ''
  editing.value = false
  try {
    const f = await pilot<FileContent>('plugin:pilot|pilot_file_read', { id: props.threadId, path: props.path })
    file.value = f
    buffer.value = f?.text ?? ''
    if (blobUrl) URL.revokeObjectURL(blobUrl)
    blobUrl = null
    pdfUrl.value = ''
    if (f?.kind === 'pdf' && f.base64) {
      const bytes = Uint8Array.from(atob(f.base64), (c) => c.charCodeAt(0))
      blobUrl = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }))
      pdfUrl.value = blobUrl
    }
    // A line to go to means the code, not the rendering.
    raw.value = props.line !== null
    await nextTick()
    if (props.line) setTimeout(() => editor.value?.reveal(props.line ?? 1), 60)
  } catch (e) {
    file.value = null
    error.value = String(e)
  } finally {
    loading.value = false
  }
}
watch(() => [props.threadId, props.path, props.nonce], () => void load(), { immediate: true })
onBeforeUnmount(() => blobUrl && URL.revokeObjectURL(blobUrl))

async function saveFile(text = buffer.value) {
  if (!file.value || file.value.readOnly) return
  try {
    await pilot('plugin:pilot|pilot_file_write', { id: props.threadId, path: file.value.path, content: text })
    file.value = { ...file.value, text }
    say('Saved')
  } catch (e) {
    say(String(e))
  }
}

function cancelEdit() {
  buffer.value = file.value?.text ?? ''
  editing.value = false
}

async function copyContents() {
  const text = editing.value ? buffer.value : file.value?.text
  if (!text) return
  await navigator.clipboard.writeText(text).catch(() => undefined)
  say('Copied')
}

async function saveAs() {
  if (!file.value) return
  try {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const target = await save({ defaultPath: name.value, title: 'Save a copy' })
    if (!target) return
    await pilot('plugin:pilot|pilot_file_save_as', { id: props.threadId, path: file.value.path, target, content: editing.value ? buffer.value : null })
    say('Saved a copy')
  } catch (e) {
    say(String(e))
  }
}
</script>

<template>
  <div class="qp-fv">
    <div class="qp-fv-bar">
      <span class="qp-fv-path" :title="file?.path ?? path">{{ file?.rel ?? path }}</span>
      <span v-if="file?.readOnly && file.kind === 'text'" class="qp-fv-tag" :title="file.truncated ? 'Only the first 4 MB are shown' : 'Outside the thread\'s folder'">read-only</span>
      <span v-if="dirty" class="qp-fv-dot" title="Unsaved changes">●</span>
      <span v-if="note" class="qp-fv-note">{{ note }}</span>
      <span class="qp-fv-tools">
        <button v-if="rendered && !editing" class="qp-fv-icon" :title="raw ? 'Rendered' : 'Raw'" @click="raw = !raw">
          <component :is="raw ? Eye : Code2" :size="13" />
        </button>
        <template v-if="file?.kind === 'text' && !file.readOnly">
          <button v-if="!editing" class="qp-fv-icon" title="Edit" @click="editing = true"><Pencil :size="13" /></button>
          <template v-else>
            <button class="qp-fv-icon" :disabled="!dirty" title="Save (Ctrl+S)" @click="saveFile()"><Save :size="13" /></button>
            <button class="qp-fv-icon" title="Stop editing (changes are dropped)" @click="cancelEdit"><X :size="13" /></button>
          </template>
        </template>
        <button v-if="file?.kind === 'text'" class="qp-fv-icon" title="Copy contents" @click="copyContents"><Copy :size="13" /></button>
        <button v-if="file" class="qp-fv-icon" title="Save as…" @click="saveAs"><Download :size="13" /></button>
        <PilotInspectorOpenIn v-if="file" :thread-id="threadId" :path="file.path" :line="line" />
      </span>
    </div>

    <div v-if="error" class="qp-fv-msg err">{{ error }}</div>
    <div v-else-if="loading && !file" class="qp-fv-msg">Opening…</div>
    <template v-else-if="file">
      <div v-if="file.kind === 'image'" class="qp-fv-media"><img :src="dataUrl" :alt="name" /></div>
      <iframe v-else-if="file.kind === 'pdf'" class="qp-fv-frame" :src="pdfUrl" :title="name" />
      <div v-else-if="file.kind === 'binary'" class="qp-fv-msg">A binary file ({{ sizeLabel(file.size) }}) — no preview.</div>
      <template v-else-if="showRendered">
        <div v-if="rendered === 'markdown'" class="qp-fv-md"><QMarkdownPreview :source="file.text ?? ''" /></div>
        <iframe v-else-if="rendered === 'html'" class="qp-fv-frame white" sandbox="allow-scripts" :srcdoc="file.text ?? ''" :title="name" />
        <div v-else-if="rendered === 'svg'" class="qp-fv-media"><img :src="svgUrl" :alt="name" /></div>
        <div v-else-if="table" class="qp-fv-table">
          <table>
            <thead v-if="table.rows.length">
              <tr><th v-for="(c, i) in table.rows[0]" :key="i">{{ c }}</th></tr>
            </thead>
            <tbody>
              <tr v-for="(r, i) in table.rows.slice(1)" :key="i"><td v-for="(c, j) in r" :key="j">{{ c }}</td></tr>
            </tbody>
          </table>
          <div v-if="table.more" class="qp-fv-msg">The first 2,000 rows — switch to Raw for the rest.</div>
        </div>
      </template>
      <QCodeEditor
        v-else
        ref="editor"
        v-model="buffer"
        class="qp-fv-code"
        :path="file.path"
        :language="languageOf(file.path)"
        :read-only="!editing"
        :minimap="false"
        @save="saveFile"
      />
    </template>
  </div>
</template>

<style scoped>
.qp-fv { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
.qp-fv-bar { display: flex; align-items: center; gap: 6px; padding: 5px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); font-size: 12px; min-width: 0; }
.qp-fv-path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-secondary, #9a9aa5); }
.qp-fv-tag { flex-shrink: 0; padding: 0 6px; border-radius: 999px; border: 1px solid var(--qss-border-subtle, #35353d); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-fv-dot { color: var(--qss-warning, #d29a3f); font-size: 10px; }
.qp-fv-note { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-fv-tools { margin-left: auto; display: inline-flex; gap: 2px; flex-shrink: 0; }
.qp-fv-icon { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-fv-icon:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-fv-icon:disabled { opacity: 0.4; cursor: default; }
.qp-fv-msg { padding: 14px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-fv-msg.err { color: var(--qss-warning, #d29a3f); white-space: pre-wrap; }
.qp-fv-code { flex: 1; min-height: 0; }
.qp-fv-media { flex: 1; min-height: 0; overflow: auto; display: flex; align-items: flex-start; justify-content: center; padding: 16px; background: repeating-conic-gradient(#2a2a30 0 25%, #222228 0 50%) 0 0 / 16px 16px; }
.qp-fv-media img { max-width: 100%; height: auto; image-rendering: auto; }
.qp-fv-frame { flex: 1; min-height: 0; width: 100%; border: none; background: var(--qss-bg, #18181e); }
.qp-fv-frame.white { background: #fff; }
.qp-fv-md { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px; }
.qp-fv-table { flex: 1; min-height: 0; overflow: auto; }
.qp-fv-table table { border-collapse: collapse; font-size: 12px; font-family: var(--qss-font-mono, ui-monospace, monospace); }
.qp-fv-table th,
.qp-fv-table td { padding: 3px 8px; border: 1px solid var(--qss-border-subtle, #35353d); white-space: nowrap; text-align: left; color: var(--qss-text, #d4d4d8); }
.qp-fv-table th { position: sticky; top: 0; background: var(--qss-bg-card, #292930); font-weight: 600; }
</style>
