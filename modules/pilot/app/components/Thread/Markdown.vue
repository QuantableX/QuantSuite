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
import { ensureKatex, forgetCode, hasMath, highlightCode, renderMarkdown, renderStreaming, type StreamCache } from '#pilot/utils/markdown'
import { registerMessage, unregisterMessage } from '#pilot/utils/markdownEvents'
import { useHost } from '#pilot/utils/host'
import { mermaidVersion, showingSource, svgOf } from '#pilot/utils/mermaid'
import { pilot } from '#pilot/utils/invoke'
import { openExternal } from '#pilot/utils/external'
import { parseFileRef } from '#pilot/utils/files'
import { usePilotStore } from '#pilot/stores/pilot'
import { useInspectorStore } from '#pilot/stores/inspector'

const props = withDefaults(defineProps<{ source: string; streaming?: boolean; cite?: { threadId: string; itemId: string } | null }>(), { streaming: false, cite: null })
const emit = defineEmits<{ notice: [text: string] }>()

const root = ref<HTMLElement | null>(null)
const html = ref('')
// Embedded in another module (pre-1.1.0): its thread, and its way to open files.
const host = useHost()
const currentThread = () => host?.threadId ?? props.cite?.threadId ?? usePilotStore().activeId
function openRef(threadId: string, path: string, line: number | null) {
  if (host?.openFile?.(path, line)) return
  useInspectorStore().openFile(threadId, path, line)
}
let frame = 0

/** A selection inside this message (parity 14): no repaint replaces it. */
function selectedHere(): boolean {
  const sel = window.getSelection()
  return !!sel && !sel.isCollapsed && !!root.value && root.value.contains(sel.anchorNode) && sel.toString().trim().length > 0
}
let held = false

/** Finished blocks of a streaming message, rendered once (parity 21). */
const stream: StreamCache = { prefix: '', html: '' }

function paint() {
  frame = 0
  if (selectedHere()) {
    held = true
    return
  }
  held = false
  if (root.value) forgetCode(root.value)
  if (props.streaming) html.value = renderStreaming(props.source, stream)
  else {
    stream.prefix = ''
    stream.html = ''
    html.value = renderMarkdown(props.source)
  }
  if (!props.streaming) void nextTick(() => root.value && highlightCode(root.value))
}

// A streaming message far off screen waits to be painted until it comes
// near (parity 21); the final paint always happens.
let offscreen = false
let dirty = false
let seen: IntersectionObserver | null = null
function schedule() {
  if (props.streaming && offscreen) {
    dirty = true
    return
  }
  if (frame) return
  frame = requestAnimationFrame(paint)
}
function watchVisibility() {
  if (seen || !root.value || typeof IntersectionObserver === 'undefined') return
  seen = new IntersectionObserver(
    ([e]) => {
      offscreen = !e?.isIntersecting
      if (!offscreen && dirty) {
        dirty = false
        schedule()
      }
    },
    { rootMargin: '1200px 0px' }
  )
  seen.observe(root.value)
}
watch(
  () => props.streaming,
  (s) => {
    if (s) watchVisibility()
    else if (!s && seen) {
      seen.disconnect()
      seen = null
      offscreen = false
    }
  },
  { flush: 'post' }
)

watch(() => props.source, schedule)

// ── Cite in composer (parity 14) ──
const QUOTE_MAX = 16 * 1024
const cite = ref<{ x: number; y: number; text: string; occurrence: number } | null>(null)
const citeComment = ref('')
const citing = ref(false)
function onSelectionChange() {
  if (held && !selectedHere()) schedule()
  if (!props.cite) return
  if (!selectedHere()) {
    if (!citing.value) cite.value = null
    return
  }
  const sel = window.getSelection()!
  const text = sel.toString().trim()
  const r = sel.getRangeAt(0).getBoundingClientRect()
  // Which occurrence of the text in the message, for highlighting the right one later.
  const before = document.createRange()
  before.setStart(root.value!, 0)
  before.setEnd(sel.getRangeAt(0).startContainer, sel.getRangeAt(0).startOffset)
  // Compared without whitespace, like findQuote: a selection's text has line
  // breaks between blocks that the DOM text has not.
  const prefix = before.toString().replace(/\s+/g, '')
  const needle = text.replace(/\s+/g, '')
  let occurrence = 0
  for (let i = prefix.indexOf(needle); i >= 0; i = prefix.indexOf(needle, i + 1)) occurrence++
  cite.value = { x: Math.min(r.right, window.innerWidth - 180), y: r.bottom + 6, text, occurrence }
}
function startCite() {
  if (!cite.value) return
  if (cite.value.text.length > QUOTE_MAX) {
    emit('notice', `The selection is ${Math.round(cite.value.text.length / 1024)} KB; a quote can be at most ${QUOTE_MAX / 1024} KB. Select less.`)
    return
  }
  citing.value = true
}
function addCite() {
  const c = cite.value
  if (!c || !props.cite) return
  const words = c.text.replace(/\s+/g, ' ')
  const label = `“${words.length > 40 ? words.slice(0, 40) + '…' : words}”`
  const chip = { kind: 'quote', label, content: c.text, path: `pilot-item:${props.cite.threadId}:${props.cite.itemId}:${c.occurrence}`, comment: citeComment.value.trim() || null }
  window.dispatchEvent(new CustomEvent('qp-add-context', { detail: { threadId: props.cite.threadId, chip } }))
  citing.value = false
  cite.value = null
  citeComment.value = ''
  window.getSelection()?.removeAllRanges()
}
function onCiteKey(e: KeyboardEvent) {
  // Ctrl+Shift+Q cites the selection from the keyboard.
  if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'q' && cite.value && !citing.value) {
    e.preventDefault()
    startCite()
  }
}
// One shared pair of document listeners for all messages (parity 21).
onMounted(() => root.value && registerMessage(root.value, { selection: onSelectionChange, key: onCiteKey }))
onBeforeUnmount(() => {
  seen?.disconnect()
  if (!root.value) return
  unregisterMessage(root.value)
  forgetCode(root.value)
})
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
  if (props.streaming) watchVisibility()
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
      const owner = currentThread()
      if (!target || !owner) return
      await pilot('plugin:pilot|pilot_file_save_as', { id: owner, path: '', target, content: svg })
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
  const threadId = currentThread()
  const code = target.closest<HTMLElement>('[data-file]')
  if (code && threadId && !window.getSelection()?.toString()) {
    openRef(threadId, code.dataset.file ?? '', Number(code.dataset.line) || null)
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
  if (ref && threadId) openRef(threadId, ref.path, ref.line)
  else if (await copyText(decodeURI(href))) emit('notice', `Copied ${decodeURI(href)}`)
}

// ── The menu of a file reference or link (parity 20) ──
const linkMenu = ref<{ x: number; y: number; file: { path: string; line: number | null } | null; href: string | null; from: HTMLElement } | null>(null)
function onContextMenu(e: MouseEvent) {
  const target = e.target as HTMLElement
  const code = target.closest<HTMLElement>('[data-file]')
  const link = code ? null : target.closest<HTMLAnchorElement>('a[href]')
  if (!code && !link) return
  e.preventDefault()
  // From the keyboard (Shift+F10, Menu key) the menu sits under the element.
  const r = (code ?? link)!.getBoundingClientRect()
  const keyboard = e.clientX === 0 && e.clientY === 0
  const at = keyboard ? { x: r.left, y: r.bottom + 4 } : { x: e.clientX, y: e.clientY }
  if (code) {
    linkMenu.value = { ...at, file: { path: code.dataset.file ?? '', line: Number(code.dataset.line) || null }, href: null, from: code }
    return
  }
  const href = link!.getAttribute('href') ?? ''
  const ref = /^(https?|mailto):/i.test(href) ? null : parseFileRef(decodeURI(href.replace(/^file:\/\/\/?/i, '')))
  linkMenu.value = ref ? { ...at, file: { path: ref.path, line: ref.line }, href: null, from: link! } : { ...at, file: null, href, from: link! }
}
function onKeydown(e: KeyboardEvent) {
  const code = (e.target as HTMLElement).closest?.<HTMLElement>('[data-file]')
  const threadId = currentThread()
  if (e.key === 'Enter' && code && threadId) {
    e.preventDefault()
    openRef(threadId, code.dataset.file ?? '', Number(code.dataset.line) || null)
  }
}
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html — sanitised in renderMarkdown -->
  <div ref="root" class="qp-md" :class="{ streaming }" @click="onClick" @contextmenu="onContextMenu" @keydown="onKeydown" v-html="html" />
  <Teleport to="body">
    <Transition name="qp-lm">
      <PilotThreadLinkMenu
        v-if="linkMenu"
        :x="linkMenu.x"
        :y="linkMenu.y"
        :thread-id="currentThread()"
        :host-open="host?.openFile ?? null"
        :host-label="host?.openLabel ?? ''"
        :file="linkMenu.file"
        :href="linkMenu.href"
        :from="linkMenu.from"
        @close="linkMenu = null"
        @notice="(t: string) => emit('notice', t)"
      />
    </Transition>
    <div v-if="cite && !citing" class="qp-cite-btn-wrap" :style="{ left: `${cite.x}px`, top: `${cite.y}px` }">
      <button class="qp-cite-btn" title="Quote the selection in the message box (Ctrl+Shift+Q)" @mousedown.prevent @click="startCite">Cite in composer</button>
    </div>
    <div v-if="cite && citing" class="qp-cite-pop" role="dialog" aria-label="Cite in composer" :style="{ left: `${Math.min(cite.x, 9999)}px`, top: `${cite.y}px` }">
      <blockquote class="qp-cite-quote">{{ cite.text.length > 400 ? cite.text.slice(0, 400) + '…' : cite.text }}</blockquote>
      <input v-model="citeComment" class="qp-cite-comment" maxlength="2000" placeholder="Comment on it (optional)" aria-label="Comment" @keydown.enter.prevent="addCite" @keydown.esc="(citing = false), (cite = null)" />
      <div class="qp-cite-actions">
        <button class="qp-btn qp-btn-sm" @click="(citing = false), (cite = null)">Cancel</button>
        <button class="qp-btn qp-btn-sm qp-btn-primary" @click="addCite">Add to message</button>
      </div>
    </div>
  </Teleport>
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
.qp-md :deep(code.qp-md-file:focus-visible) { outline: 1px solid var(--qss-accent, #7aa2f7); outline-offset: 1px; }
/* The link menu (parity 20): in fast, out faster — and a leaving menu takes no input. */
:global(.qp-lm-enter-active) { transition: opacity 120ms ease-out, transform 120ms ease-out; }
:global(.qp-lm-leave-active) { transition: opacity 90ms ease-in; pointer-events: none; }
:global(.qp-lm-enter-from) { opacity: 0; transform: translateY(-3px); }
:global(.qp-lm-leave-to) { opacity: 0; }
@media (prefers-reduced-motion: reduce) {
  :global(.qp-lm-enter-active), :global(.qp-lm-leave-active) { transition: none; }
}
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
/* Cite in composer (parity 14) — teleported, so not :deep. */
:global(.qp-cite-btn-wrap) { position: fixed; z-index: 1100; transform: translateX(-100%); }
:global(.qp-cite-btn) { padding: 3px 9px; border-radius: 7px; border: 1px solid var(--qss-accent, #7aa2f7); background: var(--qss-bg-raised, #1f1f25); color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12px; cursor: pointer; box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35); }
:global(.qp-cite-pop) { position: fixed; z-index: 1100; width: 360px; transform: translateX(-100%); display: flex; flex-direction: column; gap: 6px; padding: 8px; border-radius: 10px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); font-size: 12.5px; }
:global(.qp-cite-quote) { margin: 0; padding: 4px 8px; border-left: 3px solid var(--qss-accent, #7aa2f7); max-height: 120px; overflow: auto; white-space: pre-wrap; color: var(--qss-text-secondary, #9a9aa5); }
:global(.qp-cite-comment) { height: 28px; padding: 0 8px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font: inherit; }
:global(.qp-cite-actions) { display: flex; justify-content: flex-end; gap: 6px; }
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
