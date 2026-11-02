<script setup lang="ts">
/**
 * One rendered Markdown body (an agent reply, a user message, a plan).
 *
 * Re-rendered at most once per frame while text streams in, so a long reply
 * never parses per token. Code blocks are coloured once the text stops
 * streaming; KaTeX loads the first time a formula appears. Clicks are
 * delegated: Copy on a code block, external links to the system browser,
 * file references (`path:line` in backticks, or a link to a path) open in
 * the Files tab at the line (card 13).
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ensureKatex, hasMath, highlightCode, renderMarkdown } from '#pilot/utils/markdown'
import { mermaidVersion, showingSource, svgOf } from '#pilot/utils/mermaid'
import { pilot } from '#pilot/utils/invoke'
import { openExternal } from '#pilot/utils/external'
import { parseFileRef } from '#pilot/utils/files'
import { usePilotStore } from '#pilot/stores/pilot'
import { useInspectorStore } from '#pilot/stores/inspector'

const props = withDefaults(defineProps<{ source: string; streaming?: boolean }>(), { streaming: false })
const emit = defineEmits<{ notice: [text: string] }>()

const root = ref<HTMLElement | null>(null)
const html = ref('')
let frame = 0

function paint() {
  frame = 0
  html.value = renderMarkdown(props.source)
  if (!props.streaming) void nextTick(() => root.value && highlightCode(root.value))
}

function schedule() {
  if (frame) return
  frame = requestAnimationFrame(paint)
}

watch(() => props.source, schedule)
// A diagram finished drawing somewhere: this message may show it now (parity 13).
watch(mermaidVersion, () => {
  if (props.source.includes('mermaid')) schedule()
})
watch(
  () => props.streaming,
  (s) => {
    if (!s) paint()
  }
)

onMounted(() => {
  paint()
  if (hasMath(props.source)) void ensureKatex().then(paint)
})
watch(
  () => hasMath(props.source),
  (m) => {
    if (m) void ensureKatex().then(schedule)
  }
)
onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame)
})

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return false
  }
}


const pilotStore = usePilotStore()
const large = ref<{ hash: string; from: HTMLElement } | null>(null)
async function mermaidAction(btn: HTMLElement) {
  const hash = btn.dataset.mm ?? ''
  const act = btn.dataset.mmAct
  if (act === 'toggle') {
    if (showingSource.has(hash)) showingSource.delete(hash)
    else showingSource.add(hash)
    schedule()
  } else if (act === 'large') {
    large.value = { hash, from: btn }
  } else if (act === 'export') {
    const svg = svgOf(hash)
    if (!svg) return
    try {
      const { save } = await import('@tauri-apps/plugin-dialog')
      const target = await save({ defaultPath: 'diagram.svg', title: 'Export the diagram', filters: [{ name: 'SVG', extensions: ['svg'] }] })
      if (!target || !pilotStore.activeId) return
      await pilot('plugin:pilot|pilot_file_save_as', { id: pilotStore.activeId, path: '', target, content: svg })
      emit('notice', `Exported to ${target}`)
    } catch (err) {
      emit('notice', String(err))
    }
  }
}

async function onClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  const mm = target.closest<HTMLElement>('[data-mm-act]')
  if (mm) {
    e.preventDefault()
    void mermaidAction(mm)
    return
  }
  const copy = target.closest<HTMLElement>('[data-copy]')
  if (copy) {
    e.preventDefault()
    const code = copy.closest('.qp-md-code')?.querySelector('code')?.textContent ?? ''
    if (await copyText(code)) {
      copy.textContent = 'Copied'
      setTimeout(() => (copy.textContent = 'Copy'), 1200)
    }
    return
  }
  const threadId = usePilotStore().activeId
  const code = target.closest<HTMLElement>('[data-file]')
  if (code && threadId && !window.getSelection()?.toString()) {
    useInspectorStore().openFile(threadId, code.dataset.file ?? '', Number(code.dataset.line) || null)
    return
  }
  const link = target.closest<HTMLAnchorElement>('a[href]')
  if (!link) return
  e.preventDefault()
  const href = link.getAttribute('href') ?? ''
  if (/^(https?|mailto):/i.test(href)) {
    void openExternal(href)
    return
  }
  if (!href || href.startsWith('#')) return
  const ref = parseFileRef(decodeURI(href.replace(/^file:\/\/\/?/i, '')))
  if (ref && threadId) useInspectorStore().openFile(threadId, ref.path, ref.line)
  else if (await copyText(decodeURI(href))) emit('notice', `Copied ${decodeURI(href)}`)
}
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html — sanitised in renderMarkdown -->
  <div ref="root" class="qp-md" :class="{ streaming }" @click="onClick" v-html="html" />
  <PilotThreadMermaidView v-if="large" :hash="large.hash" @close="(large.from.focus(), (large = null))" />
</template>

<style scoped>
.qp-md {
  font-size: 13.5px;
  line-height: 1.65;
  color: var(--qss-text, #d4d4d8);
  user-select: text;
  overflow-wrap: anywhere;
  min-width: 0;
}
.qp-md :deep(> :first-child) { margin-top: 0; }
.qp-md :deep(> :last-child) { margin-bottom: 0; }
.qp-md :deep(p) { margin: 0 0 10px; }
.qp-md :deep(code.qp-md-file) { cursor: pointer; text-decoration: underline dotted; text-underline-offset: 3px; }
.qp-md :deep(code.qp-md-file:hover) { color: var(--qss-accent, #7aa2f7); }
.qp-md :deep(h1),
.qp-md :deep(h2),
.qp-md :deep(h3),
.qp-md :deep(h4) {
  margin: 16px 0 8px;
  font-weight: 600;
  line-height: 1.35;
  color: var(--qss-text, #d4d4d8);
}
.qp-md :deep(h1) { font-size: 18px; }
.qp-md :deep(h2) { font-size: 16px; }
.qp-md :deep(h3) { font-size: 14.5px; }
.qp-md :deep(h4) { font-size: 13.5px; }
.qp-md :deep(ul),
.qp-md :deep(ol) { margin: 0 0 10px; padding-left: 22px; }
.qp-md :deep(ul) { list-style: disc; }
.qp-md :deep(ul ul) { list-style: circle; }
.qp-md :deep(ol) { list-style: decimal; }
.qp-md :deep(li:has(> input[type='checkbox'])) { list-style: none; }
.qp-md :deep(li) { margin: 2px 0; }
.qp-md :deep(li::marker) { color: var(--qss-text-muted, #6e6e7a); }
.qp-md :deep(li > p) { margin: 0; }
.qp-md :deep(li input[type='checkbox']) { margin: 0 6px 0 -18px; vertical-align: -1px; }
.qp-md :deep(a) { color: var(--qss-accent, #7aa2f7); text-decoration: none; cursor: pointer; }
.qp-md :deep(a:hover) { text-decoration: underline; }
.qp-md :deep(strong) { font-weight: 600; color: var(--qss-text, #e4e4e7); }
.qp-md :deep(hr) { border: none; border-top: 1px solid var(--qss-border-subtle, #35353d); margin: 14px 0; }
.qp-md :deep(blockquote) {
  margin: 0 0 10px;
  padding: 2px 12px;
  border-left: 3px solid var(--qss-border, #47474f);
  color: var(--qss-text-secondary, #9a9aa5);
}
.qp-md :deep(:not(pre) > code) {
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--qss-bg-card, #292930);
  color: var(--qss-text, #e4e4e7);
}
.qp-md :deep(table) {
  border-collapse: collapse;
  margin: 0 0 10px;
  font-size: 12.5px;
  display: block;
  max-width: 100%;
  overflow-x: auto;
}
.qp-md :deep(th),
.qp-md :deep(td) {
  border: 1px solid var(--qss-border-subtle, #35353d);
  padding: 4px 10px;
  text-align: left;
}
.qp-md :deep(th) { background: var(--qss-bg-raised, #1f1f25); font-weight: 600; }
.qp-md :deep(.qp-md-code) {
  margin: 0 0 12px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 8px;
  background: var(--qss-bg-raised, #1f1f25);
  overflow: hidden;
}
/* Mermaid diagrams (parity 13). */
.qp-md :deep(.qp-md-mm[data-mm-state='ready'][data-mm-show='diagram'] .qp-md-mm-src) { display: none; }
.qp-md :deep(.qp-md-mm[data-mm-show='source'] .qp-md-mm-view) { display: none; }
.qp-md :deep(.qp-md-mm-view) { padding: 10px; overflow: auto; max-height: 520px; text-align: center; background: var(--qss-bg, #18181e); }
.qp-md :deep(.qp-md-mm-view svg) { max-width: none; }
.qp-md :deep(.qp-md-mm-act) { margin-left: 6px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11px; cursor: pointer; }
.qp-md :deep(.qp-md-mm-act:hover) { color: var(--qss-text, #d4d4d8); }
.qp-md :deep(.qp-md-mm-note) { margin-left: 8px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-md :deep(.qp-md-mm-error) { padding: 6px 10px; font-size: 12px; color: var(--qss-warning, #d29a3f); font-family: var(--qss-font-sans, system-ui); }
.qp-md :deep(.qp-md-code-head) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 6px 4px 12px;
  font-size: 11.5px;
  color: var(--qss-text-muted, #6e6e7a);
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-md :deep(.qp-md-copy) {
  border: none;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11.5px;
  padding: 2px 8px;
  border-radius: 5px;
  cursor: pointer;
}
.qp-md :deep(.qp-md-copy:hover) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-md :deep(pre) {
  margin: 0;
  padding: 10px 12px;
  overflow-x: auto;
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  line-height: 1.55;
  tab-size: 2;
}
.qp-md :deep(pre code) { font: inherit; background: none; padding: 0; white-space: pre; }
.qp-md :deep(.qp-md-math) { margin: 0 0 10px; overflow-x: auto; }
.qp-md :deep(.qp-md-alert) { border-left-color: var(--qss-accent, #7aa2f7); }
.qp-md :deep(.qp-md-alert.is-tip) { border-left-color: var(--qss-success, #34d399); }
.qp-md :deep(.qp-md-alert.is-important) { border-left-color: #a78bfa; }
.qp-md :deep(.qp-md-alert.is-warning) { border-left-color: var(--qss-warning, #d29a3f); }
.qp-md :deep(.qp-md-alert.is-caution) { border-left-color: var(--qss-error, #f87171); }
.qp-md :deep(.qp-md-alert-title) { margin: 0 0 2px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
</style>
