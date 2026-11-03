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
import { ChevronRight, ClipboardPaste, FileText, Film, Folder } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore } from '#pilot/stores/inspector'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { highlightQuote, parseQuotePath } from '#pilot/utils/quote'
import { CHIP_OPEN, segments } from '#pilot/utils/mentions'
import type { ThreadItem } from '#pilot/types'

const props = withDefaults(
  defineProps<{
    item: ThreadItem
    /** Round 2, card 12: `media` (images, videos — above the bubble), `body` (the rest), `all`. */
    part?: 'all' | 'media' | 'body'
  }>(),
  { part: 'all' },
)
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
const VIDEO = /\.(mp4|webm|mov|m4v|mkv|avi)$/i
const videos = computed(() => shown.value.filter((a) => a.kind === 'file' && VIDEO.test(a.name ?? a.path ?? '')))
const files = computed(() => shown.value.filter((a) => (a.kind === 'file' && !VIDEO.test(a.name ?? a.path ?? '')) || a.kind === 'folder'))
const media = computed(() => props.part !== 'body')
const body = computed(() => props.part !== 'media')
/** A message of only files shows them as cards (round 2, card 12). */
const cards = computed(() => !text.value.trim() && files.value.length > 0)
// Pasted text: a long message folds to its first lines (round 2, card 12).
const PASTE_LINES = 14
const PASTE_CHARS = 1500
const lineCount = computed(() => text.value.split('\n').length)
const pasted = computed(() => !hasChips.value && (lineCount.value > PASTE_LINES || text.value.length > PASTE_CHARS))
const expanded = ref(false)
const shownText = computed(() => (pasted.value && !expanded.value ? text.value.split('\n').slice(0, 8).join('\n').slice(0, 900) : text.value))
const folderOf = (p?: string) => (p ? p.replace(/[\\/][^\\/]*$/, '') : '')
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
    <div v-if="media && videos.length" class="qp-um-images">
      <button
        v-for="(v, i) in videos"
        :key="`v${i}`"
        class="qp-um-video"
        :class="{ gone: gone(v) }"
        :title="gone(v) ? `No longer at ${v.path}` : `${v.path} — open the video`"
        @click="openFile(v)"
        @contextmenu.prevent="openMenu(v, $event)"
      >
        <Film :size="18" />
        <span>{{ v.name }}</span>
      </button>
    </div>
    <div v-if="media && images.length" class="qp-um-images">
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
    <div v-if="body && files.length" class="qp-um-files" :class="{ cards }">
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
        <Folder v-if="f.kind === 'folder'" :size="cards ? 16 : 12" />
        <FileText v-else :size="cards ? 16 : 12" />
        <span v-if="cards" class="qp-um-card-text">
          <span class="qp-um-card-name">{{ f.name }}</span>
          <span class="qp-um-card-dir">{{ folderOf(f.path) }}</span>
        </span>
        <template v-else>{{ f.name }}</template><span v-if="gone(f)" class="qp-um-gone">unavailable</span>
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
    <div v-if="body && hasChips" class="qp-um-text">
      <template v-for="(p, i) in parts" :key="i">
        <button v-if="p.chip && isQuote(p.text)" type="button" class="qp-um-chip qp-um-quote" :title="`Quoted reply — open it\n${(chipOf(p.text)?.content ?? '').slice(0, 300)}${chipOf(p.text)?.comment ? `\nComment: ${chipOf(p.text)?.comment}` : ''}`" @click="openQuote(p.text)">{{ p.text }}</button>
        <span v-else-if="p.chip" class="qp-um-chip" :title="chipTitle(p.text)">{{ p.text }}</span>
        <template v-else>{{ p.text }}</template>
      </template>
    </div>
    <div v-else-if="body && pasted" class="qp-um-paste">
      <div class="qp-um-paste-head"><ClipboardPaste :size="12" /> Pasted text · {{ lineCount.toLocaleString('en-US') }} lines</div>
      <pre class="qp-um-paste-body" :class="{ open: expanded }">{{ shownText }}</pre>
      <button class="qp-um-paste-more" @click="expanded = !expanded">{{ expanded ? 'Show less' : 'Show all' }}</button>
    </div>
    <PilotThreadMarkdown v-else-if="body && text" :source="text" @notice="(t: string) => emit('notice', t)" />
    <details v-if="body && context.length" class="qp-um-context">
      <summary><ChevronRight class="qp-um-caret" :size="12" /> Attached context · {{ context.length }}</summary>
      <div v-for="(c, i) in context" :key="i" class="qp-um-block">
        <div class="qp-um-block-head">{{ c.chipKind === 'quote' ? `Quote ${c.label}` : c.chipKind === 'element' ? `Snapshot · ${c.label}` : c.label }}</div>
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
.qp-um-video { display: inline-flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; width: 120px; height: 80px; padding: 6px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11px; cursor: pointer; }
.qp-um-video > span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-um-video:hover { border-color: var(--qss-accent, #7aa2f7); }
.qp-um-files.cards { flex-direction: column; align-items: stretch; min-width: 0; max-width: 100%; }
.qp-um-files.cards .qp-um-file { min-width: 0; max-width: 100%; overflow: hidden; }
.qp-um-files.cards .qp-um-file { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 9px; background: var(--qss-bg-card, #292930); border-color: var(--qss-border-subtle, #35353d); text-align: left; }
.qp-um-card-text { display: flex; flex-direction: column; min-width: 0; }
.qp-um-card-name { font-size: 12.5px; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-um-card-dir { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-um-paste { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.qp-um-paste-head { display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-um-paste-body { margin: 0; max-height: 180px; overflow: hidden; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--qss-text-secondary, #9a9aa5); mask-image: linear-gradient(#000 70%, transparent); }
.qp-um-paste-body.open { max-height: none; mask-image: none; }
.qp-um-paste-more { align-self: flex-start; padding: 0; border: none; background: transparent; color: var(--qss-accent, #7aa2f7); font: inherit; font-size: 11.5px; cursor: pointer; }
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
