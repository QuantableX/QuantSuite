<script setup lang="ts">
/**
 * The conversation's scroll area.
 *
 * - Sticks to the bottom while new text streams in; scrolling up detaches,
 *   and "Jump to latest" brings it back.
 * - Older pages load when the top comes into reach, keeping the reading
 *   position.
 * - One elapsed timer, only while a turn runs and the window is visible.
 * - Find (Ctrl+F): every user message, answer, commentary and plan of the
 *   whole thread, searched in storage (parity 15); the current match's item
 *   is brought in (a bounded excerpt when old, its fold opened) and
 *   highlighted with the CSS Custom Highlight API; Enter / Shift+Enter move,
 *   Escape closes.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ArrowDown, ChevronDown, ChevronUp, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { groupTurns } from '#pilot/utils/timeline'
import { useCommands } from '#pilot/utils/commands'
import { usePilotStore } from '#pilot/stores/pilot'
import { pilot } from '#pilot/utils/invoke'
import type { ThreadItem } from '#pilot/types'
import { domMatches, locateMatch, matchIndex, needleOf, textHost, type FindHit, type FindResult } from '#pilot/utils/find'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ notice: [text: string] }>()

const threads = useThreadStore()
const view = computed(() => threads.view(props.threadId))
const historyItems = ref<ThreadItem[] | null>(null)
let revealGeneration = 0
const findOpen = ref(false)
const query = ref('')
const findInput = ref<HTMLInputElement | null>(null)
// Find state (parity 15) — declared before the reveal watcher runs.
const hits = ref<FindHit[]>([])
const total = computed(() => hits.value.reduce((n, h) => n + h.count, 0))
const truncated = ref(0)
const needle = computed(() => needleOf(query.value))
let findGen = 0
let navGen = 0
let anchor: { itemId: string; n: number } | null = null
let currentRange: Range | null = null
const unfoldedByFind = new Set<string>()
const current = ref(0)
const searching = ref(false)
const highlights = typeof CSS !== 'undefined' && 'highlights' in CSS ? (CSS as any).highlights : null
const blocks = computed(() => groupTurns(historyItems.value ?? view.value?.items ?? []))

async function latest() {
  revealGeneration++
  historyItems.value = null
  closeFind()
  await nextTick()
  scrollToBottom(true)
}

const scroller = ref<HTMLElement | null>(null)
const content = ref<HTMLElement | null>(null)
const atBottom = ref(true)
const BOTTOM_SLACK = 48

// ── Elapsed time ──
const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | null = null
const running = computed(() => Boolean(view.value?.turnId))
function syncTicker() {
  const want = running.value && document.visibilityState === 'visible'
  if (want && !ticker) ticker = setInterval(() => (now.value = Date.now()), 1000)
  if (!want && ticker) {
    clearInterval(ticker)
    ticker = null
  }
}
watch(running, syncTicker)

function elapsedOf(turnId: string, fallbackStart: number, fallbackEnd: number): number {
  const t = view.value?.turns[turnId]
  if (t?.status === 'started') return now.value - t.startedAt
  if (t?.durationMs != null) return t.durationMs
  return fallbackEnd - fallbackStart
}

// ── Scrolling ──
function onScroll() {
  const el = scroller.value
  if (!el) return
  const wasBottom = atBottom.value
  atBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_SLACK
  if (el.scrollTop < 240) void loadOlder()
  // Back at the latest: let go of older pages (parity 21), not while Find works with them.
  if (atBottom.value && !wasBottom && !historyItems.value && !findOpen.value) {
    if (threads.trimOlder(props.threadId)) void nextTick(() => scrollToBottom())
  }
}

function scrollToBottom(smooth = false) {
  const el = scroller.value
  if (!el) return
  el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
  atBottom.value = true
}

async function loadOlder() {
  const el = scroller.value
  if (!el || historyItems.value || !view.value?.hasMore || view.value.loadingOlder) return
  const before = el.scrollHeight
  await threads.loadOlder(props.threadId)
  await nextTick()
  el.scrollTop += el.scrollHeight - before
}

let resize: ResizeObserver | null = null
onMounted(() => {
  resize = new ResizeObserver(() => {
    if (atBottom.value && !historyItems.value) scrollToBottom()
  })
  if (content.value) resize.observe(content.value)
  document.addEventListener('visibilitychange', syncTicker)
  syncTicker()
  void nextTick(() => scrollToBottom())
})
onBeforeUnmount(() => {
  revealGeneration++
  resize?.disconnect()
  document.removeEventListener('visibilitychange', syncTicker)
  if (ticker) clearInterval(ticker)
  clearFind()
})
watch(
  () => props.threadId,
  () => { revealGeneration++; historyItems.value = null; closeFind(); void nextTick(() => scrollToBottom()) }
)
watch(
  () => view.value?.loaded,
  (loaded) => {
    if (loaded) void nextTick(() => scrollToBottom())
  }
)

// An old search hit opens a bounded excerpt; live conversation state keeps updating separately.
watch(
  () => [props.threadId, threads.reveal?.nonce, view.value?.loaded] as const,
  async () => {
    const r = threads.reveal
    if (!r || r.threadId !== props.threadId || !view.value?.loaded) return
    threads.reveal = null
    const generation = ++revealGeneration
    closeFind()
    historyItems.value = null
    if (!view.value.items.some((i) => i.id === r.itemId)) {
      try {
        const items = await pilot<ThreadItem[]>('plugin:pilot|pilot_history_around', { threadId: r.threadId, itemId: r.itemId })
        if (props.threadId !== r.threadId || generation !== revealGeneration) return
        historyItems.value = items ?? null
      } catch (e) { emit('notice', String(e)); return }
    }
    await nextTick()
    const item = (historyItems.value ?? view.value.items).find((i) => i.id === r.itemId)
    const root = content.value
    if (!item || !root) return
    // A message folded into "Worked for …": its fold opens (parity 15); the turn stands in otherwise.
    if (!itemEl(item.id)) {
      root.querySelector<HTMLElement>(`[data-turn="${CSS.escape(item.turnId)}"] .qp-act-head[aria-expanded="false"]`)?.click()
      await nextTick()
    }
    const el = itemEl(item.id) ?? root.querySelector(`[data-turn="${CSS.escape(item.turnId)}"]`)
    if (!(el instanceof HTMLElement)) return
    // A palette hit highlights the searched text in the message.
    if (r.query && highlights) {
      await settle()
      if (generation !== revealGeneration) return
      const first = domMatches(el, needleOf(r.query))[0]
      if (first) {
        highlights.set('qp-quote', new (window as any).Highlight(first))
        setTimeout(() => highlights.delete('qp-quote'), 6000)
      }
    }
    el.scrollIntoView({ block: el.offsetHeight > (scroller.value?.clientHeight ?? 0) ? 'start' : 'center' })
    el.classList.add('qp-flash')
    setTimeout(() => el.classList.remove('qp-flash'), 1600)
  },
  { immediate: true, flush: 'post' }
)

// ── Find (parity 15) ──
// The backend lists the matching items of the whole thread with their counts
// (pilot_thread_find); only the current match's item is brought into view —
// a bounded excerpt for an old one, its "Worked for …" fold opened — so a
// search never renders the whole transcript.


async function openFind() {
  findOpen.value = true
  await nextTick()
  findInput.value?.select()
  if (needle.value && !hits.value.length) void search(true)
}

function clearFind() {
  findGen++
  highlights?.delete('qp-find')
  highlights?.delete('qp-find-current')
  hits.value = []
  truncated.value = 0
  anchor = null
  currentRange = null
  searching.value = false
}

function closeFind(refocus = false) {
  findOpen.value = false
  clearFind()
  // A fold opened for the current match stays open: the user is reading it.
  unfoldedByFind.clear()
  if (refocus) scroller.value?.focus({ preventScroll: true })
}

function itemEl(id: string): HTMLElement | null {
  return content.value?.querySelector<HTMLElement>(`[data-item-id="${CSS.escape(id)}"]`) ?? null
}

/** Rendered text settles a frame after mounting (Markdown paints, code highlights). */
function settle(): Promise<void> {
  return new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 30))))
}

/**
 * Highlight every match in the mounted items. A mounted item's own count wins
 * over the stored text's (a link target, for one, is not shown), and the
 * current match keeps its place.
 */
function paintMatches() {
  const index = new Map(hits.value.map((h, i) => [h.itemId, i]))
  const found = new Map<number, Range[]>()
  for (const el of content.value?.querySelectorAll<HTMLElement>('[data-item-id]') ?? []) {
    const i = index.get(el.dataset.itemId ?? '')
    if (i === undefined || !textHost(el)?.textContent) continue
    found.set(i, domMatches(el, needle.value))
  }
  if ([...found].some(([i, r]) => hits.value[i]!.count !== r.length)) {
    hits.value = hits.value.map((h, i) => (found.has(i) ? { ...h, count: found.get(i)!.length } : h))
  }
  if (anchor) {
    const h = hits.value.findIndex((x) => x.itemId === anchor!.itemId)
    if (h >= 0 && hits.value[h]!.count) current.value = matchIndex(hits.value, h, Math.min(anchor.n, hits.value[h]!.count - 1))
  }
  current.value = Math.min(current.value, Math.max(0, total.value - 1))
  const loc = locateMatch(hits.value, current.value)
  currentRange = (loc && found.get(loc.hit)?.[loc.n]) || null
  if (!highlights) return
  const H = (window as any).Highlight
  highlights.set('qp-find', new H(...[...found.values()].flat()))
  if (currentRange) highlights.set('qp-find-current', new H(currentRange))
  else highlights.delete('qp-find-current')
}

function revealCurrent() {
  const el = scroller.value
  if (!currentRange || !el) return
  const r = currentRange.getBoundingClientRect()
  const box = el.getBoundingClientRect()
  if (r.top < box.top + 40 || r.bottom > box.bottom - 40) {
    el.scrollTop += r.top - box.top - box.height / 3
  }
}

/** Mount a hit's item: the live view, or a bounded excerpt around it; open its fold. */
async function bringIn(hit: FindHit, gen: number): Promise<boolean> {
  const live = view.value?.items ?? []
  if (!(historyItems.value ?? live).some((i) => i.id === hit.itemId)) {
    if (live.some((i) => i.id === hit.itemId)) {
      revealGeneration++
      historyItems.value = null
    } else {
      try {
        const items = await pilot<ThreadItem[]>('plugin:pilot|pilot_history_around', { threadId: props.threadId, itemId: hit.itemId })
        if (gen !== navGen) return false
        revealGeneration++
        historyItems.value = items ?? null
      } catch (e) {
        emit('notice', String(e))
        return false
      }
    }
    await nextTick()
  }
  if (!itemEl(hit.itemId)) {
    const head = content.value?.querySelector<HTMLElement>(`[data-turn="${CSS.escape(hit.turnId)}"] .qp-act-head[aria-expanded="false"]`)
    if (head) {
      head.click()
      unfoldedByFind.add(hit.turnId)
      await nextTick()
    }
  }
  // Folds Find opened for earlier matches close again.
  for (const t of [...unfoldedByFind]) {
    if (t === hit.turnId) continue
    content.value?.querySelector<HTMLElement>(`[data-turn="${CSS.escape(t)}"] .qp-act-head[aria-expanded="true"]`)?.click()
    unfoldedByFind.delete(t)
  }
  return true
}

/** Go to match `index` of the thread: mount it, highlight it, scroll to it. */
async function goTo(index: number, tries = 0) {
  const gen = ++navGen
  const fg = findGen
  const loc = locateMatch(hits.value, index)
  if (!loc) return
  const hit = hits.value[loc.hit]!
  current.value = index
  anchor = { itemId: hit.itemId, n: loc.n }
  if (!(await bringIn(hit, gen))) return
  await settle()
  if (gen !== navGen || fg !== findGen) return
  paintMatches()
  // The item shows no match after all (only in a link target, say): the next one.
  if (!currentRange && total.value && tries < 3 && hits.value[loc.hit]?.count === 0) return goTo(Math.min(index, total.value - 1), tries + 1)
  revealCurrent()
  // Code highlighting may replace text nodes a moment later.
  setTimeout(() => gen === navGen && fg === findGen && paintMatches(), 400)
}

/** Search the thread; `fresh` (a new query) goes to the newest match, a refresh keeps the current one. */
async function search(fresh: boolean) {
  const q = query.value.trim()
  if (!q || !needle.value) {
    clearFind()
    return
  }
  const gen = ++findGen
  if (fresh) searching.value = true
  let res: FindResult | null
  try {
    res = await pilot<FindResult>('plugin:pilot|pilot_thread_find', { threadId: props.threadId, query: q })
  } catch (e) {
    if (gen === findGen) {
      searching.value = false
      emit('notice', String(e))
    }
    return
  }
  if (gen !== findGen || !res) return
  searching.value = false
  hits.value = res.hits
  truncated.value = res.truncated ? res.total - res.hits.reduce((n, h) => n + h.count, 0) : 0
  if (fresh) {
    anchor = null
    if (total.value) await goTo(total.value - 1)
    else paintMatches()
  } else {
    // New text, a rewind or a deletion: the current match stays where it is, no scrolling.
    await settle()
    if (gen === findGen) paintMatches()
  }
}

let findTimer: ReturnType<typeof setTimeout> | null = null
watch(query, () => {
  // Old highlights go at once; the search follows the typing.
  findGen++
  highlights?.delete('qp-find')
  highlights?.delete('qp-find-current')
  if (findTimer) clearTimeout(findTimer)
  findTimer = setTimeout(() => void search(true), 150)
})

// Streaming text, new messages, rewinds: refresh at most every 700 ms.
let refreshTimer: ReturnType<typeof setTimeout> | null = null
watch(
  () => {
    const v = view.value
    const last = v?.items[v.items.length - 1]
    return `${v?.items.length}:${last?.id}:${last?.updatedAt}:${historyItems.value?.length ?? 0}`
  },
  () => {
    if (!findOpen.value || !needle.value || refreshTimer) return
    refreshTimer = setTimeout(() => {
      refreshTimer = null
      if (findOpen.value && needle.value) void search(false)
    }, 700)
  }
)

function step(dir: 1 | -1) {
  const n = total.value
  if (!n) return
  void goTo((current.value + dir + n) % n)
}

function onFindKey(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    step(e.shiftKey ? -1 : 1)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    closeFind(true)
  }
}

// Find (card 17: a rebindable command); only the open thread's timeline answers.
const pilotStore = usePilotStore()
useCommands({
  'chat.find': () => (pilotStore.activeId === props.threadId ? openFind() : false),
})
</script>

<template>
  <div class="qp-tl">
    <div v-if="historyItems" class="qp-history-excerpt" role="status">
      <span>History excerpt · {{ historyItems.length }} nearby items</span>
      <button class="qp-history-return" @click="latest">Return to latest</button>
    </div>
    <div v-if="findOpen" class="qp-find" role="search">
      <input ref="findInput" v-model="query" class="qp-find-input" placeholder="Find in thread" aria-label="Find in thread" @keydown="onFindKey" />
      <span class="qp-find-count" role="status" :title="truncated ? `${truncated.toLocaleString('en-US')} older matches are not listed — narrow the search` : ''">{{ searching ? '…' : query.trim() ? `${total ? current + 1 : 0}/${total}${truncated ? '+' : ''}` : '' }}</span>
      <button class="qp-find-btn" title="Previous (Shift+Enter)" @click="step(-1)"><ChevronUp :size="14" /></button>
      <button class="qp-find-btn" title="Next (Enter)" @click="step(1)"><ChevronDown :size="14" /></button>
      <button class="qp-find-btn" title="Close (Escape)" @click="closeFind(true)"><X :size="14" /></button>
    </div>

    <div ref="scroller" class="qp-tl-scroll" tabindex="-1" @scroll.passive="onScroll">
      <div ref="content" class="qp-tl-content">
        <div v-if="view?.loadingOlder" class="qp-tl-older">Loading earlier messages…</div>
        <PilotThreadTurn
          v-for="b in blocks"
          :key="b.turnId"
          :block="b"
          :thread-id="threadId"
          :running="view?.turnId === b.turnId"
          :idle="!view?.turnId"
          :elapsed="elapsedOf(b.turnId, b.startedAt, b.updatedAt)"
          :cost="view?.turns[b.turnId]?.costUsd ?? null"
          @notice="(t: string) => emit('notice', t)"
        />
        <slot v-if="!historyItems" name="after" />
      </div>
    </div>

    <button v-if="!atBottom && !historyItems" class="qp-jump" @click="scrollToBottom(true)"><ArrowDown :size="13" /> Jump to latest</button>
  </div>
</template>

<style scoped>
.qp-tl { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-history-excerpt { display: flex; flex-shrink: 0; flex-wrap: wrap; justify-content: center; align-items: center; gap: 12px; padding: 8px 16px; font-size: 12px; color: var(--qss-text-secondary); border-bottom: 1px solid var(--qss-border); }
.qp-history-return { flex-shrink: 0; white-space: nowrap; padding: 4px 8px; border: 1px solid var(--qss-border); border-radius: 6px; color: var(--qss-text); background: transparent; cursor: pointer; }
.qp-history-return:hover { background: var(--qss-hover); }
.qp-tl-scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-anchor: none; outline: none; }
/* Settings → Appearance → Conversation text (card 19). */
.qp-tl-content { zoom: var(--qp-text-zoom, 1); }
.qp-tl-scroll:focus-visible { box-shadow: inset 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-tl-content :deep(.qp-flash) { animation: qp-flash 1.6s ease-out; border-radius: 10px; }
@keyframes qp-flash { 0%, 30% { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 18%, transparent); } 100% { background: transparent; } }
@media (prefers-reduced-motion: reduce) { .qp-tl-content :deep(.qp-flash) { animation: none; outline: 1px solid var(--qss-accent, #7aa2f7); } }
.qp-tl-content { max-width: 780px; margin: 0 auto; padding: 18px 24px 8px; }
.qp-tl-older { text-align: center; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); padding: 6px 0 12px; }
.qp-jump {
  position: absolute;
  left: 50%;
  bottom: 12px;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 12px;
  border-radius: 14px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
}
.qp-jump:hover { color: var(--qss-text, #d4d4d8); }
.qp-find {
  position: absolute;
  top: 8px;
  right: 16px;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
}
.qp-find-input {
  width: 200px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12px;
  outline: none;
}
.qp-find-input:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-find-count { min-width: 44px; text-align: center; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-find-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  cursor: pointer;
}
.qp-find-btn:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
