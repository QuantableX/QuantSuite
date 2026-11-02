<script setup lang="ts">
/**
 * A sent message: its attachments (thumbnails, files, folders), the text
 * with its chips, and an "Attached context" fold with what each chip
 * carried (card 9).
 *
 * Parity 12: a thumbnail opens the original image full size (MediaViewer);
 * a file opens in the inspector (read-only outside the checkout), a folder
 * in Explorer; right-click (or Shift+F10) offers Copy path, Open in…, Show
 * in Explorer and Save a copy. An attachment whose file moved or was deleted
 * shows as unavailable instead of failing silently.
 */
import { computed, onMounted, ref } from 'vue'
import { ChevronRight, FileText, Folder } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore } from '#pilot/stores/inspector'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { highlightQuote, parseQuotePath } from '#pilot/utils/quote'
import { CHIP_OPEN, segments } from '#pilot/utils/mentions'
import type { ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const emit = defineEmits<{ notice: [text: string] }>()
const inspector = useInspectorStore()

interface Shown {
  kind: string
  name?: string
  path?: string
  thumb?: string | null
  chipKind?: string
  label?: string
  content?: string
  comment?: string | null
}

const text = computed(() => String(props.item.data.text ?? ''))
const shown = computed<Shown[]>(() => props.item.data.attachments ?? [])
const images = computed(() => shown.value.filter((a) => a.kind === 'image'))
const files = computed(() => shown.value.filter((a) => a.kind === 'file' || a.kind === 'folder'))
const chips = computed(() => shown.value.filter((a) => a.kind === 'chip'))
/** What the chips carried — a skill carries only its name. */
const context = computed(() => chips.value.filter((c) => c.content && c.chipKind !== 'skill'))
const hasChips = computed(() => text.value.includes(CHIP_OPEN))
const parts = computed(() => segments(text.value))
// ── Attachments as things to open (parity 12) ──
const missing = ref<Set<string>>(new Set())
onMounted(async () => {
  const paths = [...images.value, ...files.value].map((a) => a.path).filter((p): p is string => !!p)
  if (!paths.length) return
  const exists = (await pilot<boolean[]>('plugin:pilot|pilot_paths_exist', { paths }).catch(() => null)) ?? []
  missing.value = new Set(paths.filter((_, i) => exists[i] === false))
})
const gone = (a: Shown) => !!a.path && missing.value.has(a.path)
const viewing = ref<number | null>(null)
let opener: HTMLElement | null = null
function view(i: number, e: Event) {
  opener = e.currentTarget as HTMLElement
  viewing.value = i
}
function closeViewer() {
  viewing.value = null
  opener?.focus()
}
function openFile(f: Shown) {
  if (!f.path || gone(f)) return emit('notice', `${f.name ?? 'The file'} is no longer at ${f.path}.`)
  if (f.kind === 'folder') void reveal(f)
  else inspector.openFile(props.item.threadId, f.path, null)
}
async function reveal(a: Shown) {
  if (!a.path) return
  await pilot('plugin:pilot|pilot_open_in', { id: props.item.threadId, path: a.path, line: null, editor: 'explorer' }).catch((e) => emit('notice', String(e)))
}
async function openIn(a: Shown) {
  if (!a.path) return
  await pilot('plugin:pilot|pilot_open_in', { id: props.item.threadId, path: a.path, line: null, editor: 'default' }).catch(() =>
    pilot('plugin:pilot|pilot_open_in', { id: props.item.threadId, path: a.path, line: null, editor: 'explorer' }).catch((e) => emit('notice', String(e)))
  )
}
async function saveCopy(a: Shown) {
  if (!a.path) return
  try {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const target = await save({ defaultPath: a.path.split(/[\\/]/).pop(), title: 'Save a copy' })
    if (!target) return
    await pilot('plugin:pilot|pilot_file_save_as', { id: props.item.threadId, path: a.path, target, content: null })
    emit('notice', `Saved a copy to ${target}.`)
  } catch (e) {
    emit('notice', String(e))
  }
}
const menu = ref<{ a: Shown; x: number; y: number } | null>(null)
function openMenu(a: Shown, e: MouseEvent | KeyboardEvent) {
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
  menu.value = { a, x: 'clientX' in e ? e.clientX : r.left, y: 'clientY' in e ? e.clientY : r.bottom }
}
async function menuAction(what: string) {
  const a = menu.value?.a
  menu.value = null
  if (!a?.path) return
  if (what === 'copy') await navigator.clipboard.writeText(a.path).then(() => emit('notice', 'Path copied.'), () => emit('notice', 'The clipboard is not available.'))
  else if (what === 'open') await openIn(a)
  else if (what === 'reveal') await reveal(a)
  else if (what === 'save') await saveCopy(a)
}

// ── Quotes go back to their source (parity 14) ──
const chipOf = (label: string) => chips.value.find((x) => x.label === label)
const isQuote = (label: string) => chipOf(label)?.chipKind === 'quote'
async function openQuote(label: string) {
  const c = chipOf(label)
  const src = parseQuotePath(c?.path)
  if (!c || !src) return
  let turnId = ''
  try {
    const around = await pilot<{ id: string; turnId: string }[]>('plugin:pilot|pilot_history_around', { threadId: src.threadId, itemId: src.itemId })
    turnId = (around ?? []).find((x) => x.id === src.itemId)?.turnId ?? ''
  } catch {
    emit('notice', 'The quoted reply is no longer there (rewound, deleted or out of reach) — the quote keeps its text.')
    return
  }
  const store = usePilotStore()
  const threads = useThreadStore()
  if (store.activeId !== src.threadId) await store.openSession(src.threadId)
  threads.reveal = { threadId: src.threadId, itemId: src.itemId, nonce: Date.now() }
  let unfolded = false
  for (let i = 0; i < 40; i++) {
    await new Promise((r) => setTimeout(r, 150))
    const el = document.querySelector<HTMLElement>(`[data-item-id="${CSS.escape(src.itemId)}"]`)
    // A message folded into "Worked for …": open the fold once.
    if (!el && !unfolded && turnId) {
      const head = document.querySelector<HTMLElement>(`[data-turn="${CSS.escape(turnId)}"] .qp-act-head[aria-expanded="false"]`)
      if (head) {
        head.click()
        unfolded = true
      }
    }
    if (el) {
      if (!highlightQuote(el, c.content ?? '', src.occurrence)) emit('notice', 'The reply changed since it was quoted — the quote keeps the text as it was.')
      return
    }
  }
}

const chipTitle = (label: string) => {
  const c = chips.value.find((x) => x.label === label)
  return c ? `${c.chipKind ?? 'context'} · ${(c.content ?? '').slice(0, 300)}` : label
}
</script>

<template>
  <div class="qp-um">
    <div v-if="images.length" class="qp-um-images">
      <button
        v-for="(im, i) in images"
        :key="i"
        class="qp-um-thumb-btn"
        :class="{ gone: gone(im) }"
        :title="gone(im) ? `No longer at ${im.path}` : `${im.path} — open full size`"
        :aria-label="`Open image ${im.name ?? i + 1}`"
        @click="gone(im) ? emit('notice', `The image is no longer at ${im.path}.`) : view(i, $event)"
        @contextmenu.prevent="openMenu(im, $event)"
        @keydown.shift.f10.prevent="openMenu(im, $event)"
      >
        <img v-if="im.thumb" class="qp-um-thumb" :src="im.thumb" :alt="im.name" />
        <span v-else class="qp-um-count">{{ im.name ?? 'image' }}</span>
      </button>
    </div>
    <div v-if="files.length" class="qp-um-files">
      <button
        v-for="(f, i) in files"
        :key="i"
        class="qp-um-file"
        :class="{ gone: gone(f) }"
        :title="gone(f) ? `No longer at ${f.path}` : f.path"
        @click="openFile(f)"
        @contextmenu.prevent="openMenu(f, $event)"
        @keydown.shift.f10.prevent="openMenu(f, $event)"
      >
        <Folder v-if="f.kind === 'folder'" :size="12" />
        <FileText v-else :size="12" />
        {{ f.name }}<span v-if="gone(f)" class="qp-um-gone">unavailable</span>
      </button>
    </div>
    <PilotThreadMediaViewer v-if="viewing !== null" :thread-id="item.threadId" :images="images.filter((im) => im.path).map((im) => ({ path: im.path!, name: im.name }))" :start="viewing" @close="closeViewer" />
    <Teleport to="body">
      <div v-if="menu" class="qp-um-veil" @pointerdown="menu = null" />
      <div v-if="menu" class="qp-um-menu" role="menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @keydown.esc="menu = null">
        <button role="menuitem" @click="menuAction('copy')">Copy path</button>
        <button v-if="menu.a.kind !== 'folder'" role="menuitem" :disabled="gone(menu.a)" @click="menuAction('open')">Open in the default app</button>
        <button role="menuitem" :disabled="gone(menu.a)" @click="menuAction('reveal')">Show in Explorer</button>
        <button v-if="menu.a.kind !== 'folder'" role="menuitem" :disabled="gone(menu.a)" @click="menuAction('save')">Save a copy…</button>
      </div>
    </Teleport>
    <div v-if="hasChips" class="qp-um-text">
      <template v-for="(p, i) in parts" :key="i">
        <button v-if="p.chip && isQuote(p.text)" type="button" class="qp-um-chip qp-um-quote" :title="`Quoted reply — open it\n${(chipOf(p.text)?.content ?? '').slice(0, 300)}${chipOf(p.text)?.comment ? `\nComment: ${chipOf(p.text)?.comment}` : ''}`" @click="openQuote(p.text)">{{ p.text }}</button>
        <span v-else-if="p.chip" class="qp-um-chip" :title="chipTitle(p.text)">{{ p.text }}</span>
        <template v-else>{{ p.text }}</template>
      </template>
    </div>
    <PilotThreadMarkdown v-else-if="text" :source="text" @notice="(t: string) => emit('notice', t)" />
    <details v-if="context.length" class="qp-um-context">
      <summary><ChevronRight class="qp-um-caret" :size="12" /> Attached context · {{ context.length }}</summary>
      <div v-for="(c, i) in context" :key="i" class="qp-um-block">
        <div class="qp-um-block-head">{{ c.chipKind === 'quote' ? `Quote ${c.label}` : c.label }}</div>
        <div v-if="c.comment" class="qp-um-comment">Comment: {{ c.comment }}</div>
        <pre class="qp-um-pre">{{ c.content }}</pre>
      </div>
    </details>
  </div>
</template>

<style scoped>
.qp-um { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.qp-um-images { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.qp-um-file { font: inherit; font-size: 11.5px; border: 1px solid transparent; cursor: pointer; }
.qp-um-file:hover:not(.gone) { border-color: var(--qss-accent, #7aa2f7); }
.qp-um-quote { border: none; font: inherit; cursor: pointer; }
.qp-um-quote:hover { text-decoration: underline; }
.qp-um-comment { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); padding: 2px 0 4px; }
.qp-um-thumb-btn { padding: 0; border: none; background: transparent; cursor: zoom-in; border-radius: 6px; }
.qp-um-thumb-btn:focus-visible { outline: 2px solid var(--qss-accent, #7aa2f7); outline-offset: 2px; }
.qp-um-thumb-btn.gone, .qp-um-file.gone { opacity: 0.55; cursor: not-allowed; text-decoration: line-through; }
.qp-um-gone { margin-left: 4px; font-size: 10.5px; color: var(--qss-warning, #d29a3f); text-decoration: none; }
.qp-um-veil { position: fixed; inset: 0; z-index: 1150; }
.qp-um-menu { position: fixed; z-index: 1160; display: flex; flex-direction: column; min-width: 180px; padding: 4px; border-radius: 8px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); }
.qp-um-menu button { border: none; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; text-align: left; padding: 5px 8px; border-radius: 6px; cursor: pointer; }
.qp-um-menu button:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-um-menu button:disabled { opacity: 0.5; cursor: default; }
.qp-um-thumb { max-width: 160px; max-height: 120px; border-radius: 6px; border: 1px solid var(--qss-border-subtle, #35353d); object-fit: cover; }
.qp-um-count { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-um-files { display: flex; flex-wrap: wrap; gap: 4px; }
.qp-um-file,
.qp-um-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
  border-radius: 5px;
  background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 16%, transparent);
  color: var(--qss-text, #d4d4d8);
  font-size: 12px;
  line-height: 1.6;
}
.qp-um-text { font-size: 13.5px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-um-context { font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-um-context summary { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; list-style: none; }
.qp-um-context summary::-webkit-details-marker { display: none; }
.qp-um-context[open] .qp-um-caret { transform: rotate(90deg); }
.qp-um-block { margin-top: 6px; }
.qp-um-block-head { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); margin-bottom: 2px; }
.qp-um-pre {
  max-height: 200px;
  margin: 0;
  padding: 6px 8px;
  overflow: auto;
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
}
</style>
