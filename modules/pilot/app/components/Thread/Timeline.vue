<script setup lang="ts">
/**
 * The conversation's scroll area.
 *
 * - Sticks to the bottom while new text streams in; scrolling up detaches,
 *   and "Jump to latest" brings it back.
 * - Older pages load when the top comes into reach, keeping the reading
 *   position.
 * - One elapsed timer, only while a turn runs and the window is visible.
 * - Find (Ctrl+F): every user message, answer and plan of the whole thread
 *   (older pages are loaded first), highlighted with the CSS Custom
 *   Highlight API; Enter / Shift+Enter move, Escape closes.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useShortcuts } from '@quantsuite/core'
import { ArrowDown, ChevronDown, ChevronUp, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { groupTurns } from '#pilot/utils/timeline'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ notice: [text: string] }>()

const threads = useThreadStore()
const view = computed(() => threads.view(props.threadId))
const blocks = computed(() => groupTurns(view.value?.items ?? []))

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
  atBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < BOTTOM_SLACK
  if (el.scrollTop < 240) void loadOlder()
}

function scrollToBottom(smooth = false) {
  const el = scroller.value
  if (!el) return
  el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
  atBottom.value = true
}

async function loadOlder() {
  const el = scroller.value
  if (!el || !view.value?.hasMore || view.value.loadingOlder) return
  const before = el.scrollHeight
  await threads.loadOlder(props.threadId)
  await nextTick()
  el.scrollTop += el.scrollHeight - before
}

let resize: ResizeObserver | null = null
onMounted(() => {
  resize = new ResizeObserver(() => {
    if (atBottom.value) scrollToBottom()
  })
  if (content.value) resize.observe(content.value)
  document.addEventListener('visibilitychange', syncTicker)
  syncTicker()
  void nextTick(() => scrollToBottom())
})
onBeforeUnmount(() => {
  resize?.disconnect()
  document.removeEventListener('visibilitychange', syncTicker)
  if (ticker) clearInterval(ticker)
  clearFind()
})
watch(
  () => props.threadId,
  () => void nextTick(() => scrollToBottom())
)
watch(
  () => view.value?.loaded,
  (loaded) => {
    if (loaded) void nextTick(() => scrollToBottom())
  }
)

// ── Find ──
const findOpen = ref(false)
const query = ref('')
const findInput = ref<HTMLInputElement | null>(null)
const matches = ref<Range[]>([])
const current = ref(0)
const searching = ref(false)
const highlights = typeof CSS !== 'undefined' && 'highlights' in CSS ? (CSS as any).highlights : null

async function openFind() {
  findOpen.value = true
  await nextTick()
  findInput.value?.select()
}

function clearFind() {
  highlights?.delete('qp-find')
  highlights?.delete('qp-find-current')
  matches.value = []
}

function closeFind() {
  findOpen.value = false
  clearFind()
}

async function loadEverything() {
  let guard = 0
  while (view.value?.hasMore && guard++ < 50) await threads.loadOlder(props.threadId)
}

function collect(q: string): Range[] {
  const root = content.value
  if (!root || !q) return []
  const needle = q.toLowerCase()
  const ranges: Range[] = []
  for (const host of root.querySelectorAll<HTMLElement>('[data-item-id] .qp-md')) {
    const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = (node.nodeValue ?? '').toLowerCase()
      let at = text.indexOf(needle)
      while (at >= 0) {
        const r = new Range()
        r.setStart(node, at)
        r.setEnd(node, at + needle.length)
        ranges.push(r)
        at = text.indexOf(needle, at + needle.length)
      }
    }
  }
  return ranges
}

function paintMatches() {
  if (!highlights) return
  const H = (window as any).Highlight
  highlights.set('qp-find', new H(...matches.value))
  const cur = matches.value[current.value]
  if (cur) highlights.set('qp-find-current', new H(cur))
  else highlights.delete('qp-find-current')
}

function reveal() {
  const cur = matches.value[current.value]
  const el = scroller.value
  if (!cur || !el) return
  const r = cur.getBoundingClientRect()
  const box = el.getBoundingClientRect()
  if (r.top < box.top + 40 || r.bottom > box.bottom - 40) {
    el.scrollTop += r.top - box.top - box.height / 3
  }
}

let findTimer: ReturnType<typeof setTimeout> | null = null
watch(query, (q) => {
  if (findTimer) clearTimeout(findTimer)
  findTimer = setTimeout(async () => {
    if (!q.trim()) {
      clearFind()
      return
    }
    searching.value = true
    await loadEverything()
    await nextTick()
    matches.value = collect(q.trim())
    current.value = Math.max(0, matches.value.length - 1)
    searching.value = false
    paintMatches()
    reveal()
  }, 150)
})

function step(dir: 1 | -1) {
  const n = matches.value.length
  if (!n) return
  current.value = (current.value + dir + n) % n
  paintMatches()
  reveal()
}

function onFindKey(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    step(e.shiftKey ? -1 : 1)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    closeFind()
  }
}

useShortcuts([
  {
    key: 'f',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      void openFind()
    },
  },
])
</script>

<template>
  <div class="qp-tl">
    <div v-if="findOpen" class="qp-find" role="search">
      <input ref="findInput" v-model="query" class="qp-find-input" placeholder="Find in thread" @keydown="onFindKey" />
      <span class="qp-find-count">{{ searching ? '…' : query ? `${matches.length ? current + 1 : 0}/${matches.length}` : '' }}</span>
      <button class="qp-find-btn" title="Previous (Shift+Enter)" @click="step(-1)"><ChevronUp :size="14" /></button>
      <button class="qp-find-btn" title="Next (Enter)" @click="step(1)"><ChevronDown :size="14" /></button>
      <button class="qp-find-btn" title="Close (Escape)" @click="closeFind"><X :size="14" /></button>
    </div>

    <div ref="scroller" class="qp-tl-scroll" @scroll.passive="onScroll">
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
        <slot name="after" />
      </div>
    </div>

    <button v-if="!atBottom" class="qp-jump" @click="scrollToBottom(true)"><ArrowDown :size="13" /> Jump to latest</button>
  </div>
</template>

<style scoped>
.qp-tl { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-tl-scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-anchor: none; }
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
