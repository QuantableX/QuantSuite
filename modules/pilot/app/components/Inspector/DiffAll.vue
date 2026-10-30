<script setup lang="ts">
/**
 * The Diff tab's all-files view (card 24; T3 Code's diff panel): every file
 * of the scope in one scroll, each a unified diff loaded as it scrolls into
 * view; its header folds it. Ctrl+F finds across all of them — folded ones
 * and ones not loaded yet included: Enter / Shift+Enter step, Escape closes.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { ChevronRight, Copy, Maximize2, X } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { findMatches, markSegments, parsePatch, stepMatch, type PatchLine } from '#pilot/utils/diff'
import type { FileChange } from '#pilot/types'

const props = defineProps<{ threadId: string; from: string; to: string; files: FileChange[]; staged: string[] }>()
const emit = defineEmits<{ focus: [path: string] }>()

/** Rendered lines per file; the rest is a click away in the one-file view. */
const SHOWN_MAX = 4000

const patches = ref<Record<string, PatchLine[]>>({})
const folded = ref<Set<string>>(new Set())
const scroller = ref<HTMLElement | null>(null)
const error = ref('')
const pending = new Set<string>()

const order = computed(() => props.files.map((f) => f.path))
const stagedSet = computed(() => new Set(props.staged))
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) }
}

async function fetchPaths(paths: string[]) {
  const want = paths.filter((p) => !(p in patches.value) && !pending.has(p) && !props.files.find((f) => f.path === p)?.binary)
  if (!want.length) return
  want.forEach((p) => pending.add(p))
  const key = `${props.from}|${props.to}`
  try {
    for (let i = 0; i < want.length; i += 20) {
      const chunk = want.slice(i, i + 20)
      const got = (await pilot<{ path: string; patch: string }[]>('plugin:pilot|pilot_diff_patches', { id: props.threadId, from: props.from, to: props.to, paths: chunk })) ?? []
      if (key !== `${props.from}|${props.to}`) return
      patches.value = { ...patches.value, ...Object.fromEntries(got.map((g) => [g.path, parsePatch(g.patch)])) }
    }
  } catch (e) {
    error.value = String(e)
  } finally {
    want.forEach((p) => pending.delete(p))
  }
}

// Files load as they come into view (with some room ahead), a few at a time.
let observer: IntersectionObserver | null = null
let queue: string[] = []
let flushTimer: ReturnType<typeof setTimeout> | null = null
function observe() {
  observer?.disconnect()
  if (!scroller.value) return
  observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) if (e.isIntersecting) queue.push((e.target as HTMLElement).dataset.path ?? '')
      if (flushTimer) clearTimeout(flushTimer)
      flushTimer = setTimeout(() => {
        const paths = [...new Set(queue.filter(Boolean))]
        queue = []
        void fetchPaths(paths)
      }, 40)
    },
    { root: scroller.value, rootMargin: '600px 0px' }
  )
  scroller.value.querySelectorAll<HTMLElement>('[data-path]').forEach((el) => observer!.observe(el))
}
watch(
  () => [props.from, props.to, order.value.join('\n')],
  async () => {
    patches.value = {}
    folded.value = new Set()
    error.value = ''
    await nextTick()
    observe()
    if (query.value) void loadAllThenFind()
  },
  { immediate: true }
)
watch(scroller, () => observe())
onBeforeUnmount(() => {
  observer?.disconnect()
  if (flushTimer) clearTimeout(flushTimer)
})

function toggleFold(path: string) {
  const next = new Set(folded.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  folded.value = next
  void nextTick(observe)
}

async function copyPath(path: string) {
  await navigator.clipboard.writeText(path).catch(() => undefined)
}

/** A placeholder as tall as the file will roughly be, so the scroll does not jump. */
const estimate = (f: FileChange) => Math.min(600, (f.added + f.removed + 6) * 18)

// ── Find across all files ──
const finding = ref(false)
const query = ref('')
const active = ref(-1)
const input = ref<HTMLInputElement | null>(null)
const matches = computed(() => findMatches(order.value.map((p) => patches.value[p] ?? []), query.value))
const marks = computed(() => {
  const m = new Map<string, { start: number; end: number; current: boolean }[]>()
  matches.value.forEach((x, i) => {
    const key = `${x.file}:${x.line}`
    const list = m.get(key) ?? []
    list.push({ start: x.start, end: x.end, current: i === active.value })
    m.set(key, list)
  })
  return m
})
const segs = (file: number, line: number, text: string) => markSegments(text, marks.value.get(`${file}:${line}`) ?? [])

let findTimer: ReturnType<typeof setTimeout> | null = null
async function loadAllThenFind() {
  await fetchPaths(order.value)
  active.value = matches.value.length ? 0 : -1
  reveal()
}
watch(query, () => {
  if (findTimer) clearTimeout(findTimer)
  findTimer = setTimeout(() => void loadAllThenFind(), 150)
})

function reveal() {
  const m = matches.value[active.value]
  if (!m) return
  const path = order.value[m.file]!
  if (folded.value.has(path)) toggleFold(path)
  void nextTick(() => {
    const section = scroller.value?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)
    section?.querySelector<HTMLElement>(`[data-line="${m.line}"]`)?.scrollIntoView({ block: 'center' })
  })
}
function step(dir: 1 | -1) {
  active.value = stepMatch(matches.value.length, active.value, dir)
  reveal()
}
function openFind() {
  finding.value = true
  void nextTick(() => {
    input.value?.focus()
    input.value?.select()
  })
}
function closeFind() {
  finding.value = false
  query.value = ''
  active.value = -1
  scroller.value?.focus()
}
function onKey(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    e.stopPropagation()
    openFind()
  }
}
function onFindKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    closeFind()
  } else if (e.key === 'Enter') {
    e.preventDefault()
    step(e.shiftKey ? -1 : 1)
  }
}
</script>

<template>
  <div class="qp-dall" @keydown="onKey">
    <div v-if="finding" class="qp-dall-find">
      <input ref="input" v-model="query" class="qp-dall-find-input" placeholder="Find in all files" spellcheck="false" @keydown="onFindKey" />
      <span class="qp-dall-find-count">{{ query ? (matches.length ? `${active + 1} of ${matches.length}` : 'No results') : '' }}</span>
      <button class="qp-dall-icon" title="Close (Esc)" aria-label="Close find" @click="closeFind"><X :size="13" /></button>
    </div>
    <div v-if="error" class="qp-dall-error">{{ error }}</div>
    <div ref="scroller" class="qp-dall-scroll" tabindex="-1">
      <section v-for="(f, fi) in files" :key="f.path" class="qp-dall-file" :data-path="f.path">
        <header class="qp-dall-head" :title="f.path" @click="toggleFold(f.path)">
          <ChevronRight class="qp-dall-chev" :class="{ open: !folded.has(f.path) }" :size="12" />
          <span class="qp-dall-st" :data-st="f.status">{{ f.status }}</span>
          <span class="qp-dall-dir">{{ split(f.path).dir }}</span><span class="qp-dall-name">{{ split(f.path).name }}</span>
          <span v-if="stagedSet.has(f.path)" class="qp-dall-staged" title="Staged">●</span>
          <span class="qp-dall-num"><span class="add">+{{ f.added }}</span> <span class="del">−{{ f.removed }}</span></span>
          <button class="qp-dall-icon" title="Copy path" aria-label="Copy path" @click.stop="copyPath(f.path)"><Copy :size="12" /></button>
          <PilotInspectorOpenIn :thread-id="threadId" :path="f.path" @click.stop />
          <button class="qp-dall-icon" title="Open alone (split view, stage, comment)" aria-label="Open alone" @click.stop="emit('focus', f.path)"><Maximize2 :size="12" /></button>
        </header>
        <div v-if="!folded.has(f.path)" class="qp-dall-body">
          <div v-if="f.binary" class="qp-dall-note">A binary file — no text diff.</div>
          <div v-else-if="!patches[f.path]" class="qp-dall-skel" :style="{ height: `${estimate(f)}px` }" />
          <template v-else>
            <div v-for="(l, li) in patches[f.path]!.slice(0, SHOWN_MAX)" :key="li" class="qp-dall-ln" :class="l.kind" :data-line="li">
              <span class="no">{{ l.old ?? '' }}</span>
              <span class="no">{{ l.new ?? '' }}</span>
              <span class="code"><template v-for="(s, si) in segs(fi, li, l.text)" :key="si"><mark v-if="s.mark" :class="{ cur: s.current }">{{ s.text }}</mark><template v-else>{{ s.text }}</template></template></span>
            </div>
            <button v-if="patches[f.path]!.length > SHOWN_MAX" class="qp-dall-more" @click="emit('focus', f.path)">
              {{ patches[f.path]!.length - SHOWN_MAX }} more lines — open the file alone
            </button>
          </template>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qp-dall { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-dall-find { display: flex; align-items: center; gap: 6px; padding: 5px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dall-find-input {
  flex: 1;
  min-width: 0;
  padding: 3px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  outline: none;
}
.qp-dall-find-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-dall-find-count { flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-dall-error { margin: 8px; padding: 6px 8px; border-radius: 6px; background: color-mix(in srgb, var(--qss-warning, #d29a3f) 14%, transparent); color: var(--qss-warning, #d29a3f); }
.qp-dall-scroll { flex: 1; min-height: 0; overflow: auto; outline: none; }
.qp-dall-file { border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dall-head {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 4px 8px;
  background: var(--qss-bg-raised, #1f1f25);
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
  cursor: pointer;
  user-select: none;
}
.qp-dall-head:hover { background: var(--qss-bg-hover, #313139); }
.qp-dall-chev { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); transition: transform var(--qp-motion, 160ms) ease; }
.qp-dall-chev.open { transform: rotate(90deg); }
.qp-dall-st { width: 14px; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; text-align: center; }
.qp-dall-st[data-st='A'] { color: var(--qss-success, #34d399); }
.qp-dall-st[data-st='D'] { color: var(--qss-error, #f87171); }
.qp-dall-st[data-st='M'] { color: var(--qss-warning, #d29a3f); }
.qp-dall-dir { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-dall-name { flex-shrink: 0; white-space: nowrap; }
.qp-dall-staged { color: var(--qss-success, #34d399); font-size: 9px; }
.qp-dall-num { margin-left: auto; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-dall-num .add { color: var(--qss-success, #34d399); }
.qp-dall-num .del { color: var(--qss-error, #f87171); }
.qp-dall-icon { display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-dall-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-dall-body { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; line-height: 18px; }
.qp-dall-note { padding: 10px 12px; font-family: var(--qss-font-sans, system-ui, sans-serif); color: var(--qss-text-muted, #6e6e7a); }
.qp-dall-skel { background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--qss-bg-hover, #313139) 50%, transparent), transparent); background-size: 200% 100%; animation: qp-dall-skel 1.4s ease-in-out infinite; }
@keyframes qp-dall-skel { from { background-position: 100% 0; } to { background-position: -100% 0; } }
@media (prefers-reduced-motion: reduce) {
  .qp-dall-skel { animation: none; }
}
.qp-dall-ln { display: flex; min-width: max-content; white-space: pre; }
.qp-dall-ln .no { flex-shrink: 0; width: 44px; padding-right: 8px; text-align: right; color: var(--qss-text-muted, #6e6e7a); user-select: none; }
.qp-dall-ln .code { padding: 0 10px 0 4px; }
.qp-dall-ln.add { background: color-mix(in srgb, var(--qss-success, #34d399) 10%, transparent); }
.qp-dall-ln.del { background: color-mix(in srgb, var(--qss-error, #f87171) 10%, transparent); }
.qp-dall-ln.hunk { color: var(--qss-text-muted, #6e6e7a); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 6%, transparent); }
.qp-dall-ln.meta { color: var(--qss-text-muted, #6e6e7a); }
.qp-dall-ln mark { background: color-mix(in srgb, var(--qss-warning, #d29a3f) 35%, transparent); color: inherit; border-radius: 2px; }
.qp-dall-ln mark.cur { background: var(--qss-warning, #d29a3f); color: var(--qss-bg, #18181e); }
.qp-dall-more { display: block; margin: 6px 8px; padding: 2px 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: 11.5px var(--qss-font-sans, system-ui, sans-serif); cursor: pointer; }
</style>
