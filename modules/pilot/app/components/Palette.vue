<script setup lang="ts">
/**
 * The command palette (card 17; T3 Code's), Ctrl+K inside QuantPilot, over
 * the main area: threads of every project by title, branch, pull request or
 * project (often and lately opened ones first, the pick for the same query
 * boosted for three weeks); "In messages" — every thread's messages and plans,
 * from three letters on, opening at the message; projects (a new thread
 * there); actions with their keys. `>` shows actions only. Up/Down and Enter
 * or Ctrl+1..9 pick; Escape closes and returns to the message box.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { CornerDownLeft, FolderGit2, MessageSquare, Plus, Search, SquareTerminal, Zap } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { useKeysStore } from '#pilot/stores/keys'
import { COMMANDS } from '#pilot/utils/keybindings'
import { hasCommand, runCommand } from '#pilot/utils/commands'
import { matchesWords, rankThreads, type PaletteThread } from '#pilot/utils/palette'
import { plainText } from '#pilot/utils/mentions'
import { pilot } from '#pilot/utils/invoke'

interface MessageHit {
  threadId: string
  itemId: string
  kind: string
  snippet: string
  createdAt: number
}

interface Row {
  key: string
  section: string
  icon: unknown
  title: string
  sub: string
  hint: string
  run: () => unknown
}

const store = usePilotStore()
const threads = useThreadStore()
const sidebar = useSidebarStore()
const git = useGitStore()
const keys = useKeysStore()

const input = ref<HTMLInputElement | null>(null)
const query = ref(keys.paletteSeed)
const active = ref(0)
/** "New thread in…": only projects. */
const projectMode = ref(false)
const hits = ref<MessageHit[]>([])
const searching = ref(false)

const actionsOnly = computed(() => query.value.startsWith('>'))
const text = computed(() => (actionsOnly.value ? query.value.slice(1) : query.value).trim())

const paletteThreads = computed<PaletteThread[]>(() =>
  store.sessions.map((s) => {
    const pr = (git.links[s.id] ?? []).filter((l) => l.kind === 'pr').at(-1)
    return { id: s.id, title: s.title, branch: s.branch ?? '', project: s.contextName ?? '', pr: pr ? `#${pr.number} ${pr.title}` : '', lastActivityAt: s.lastActivityAt ?? 0 }
  })
)
const titleOf = (id: string) => store.sessions.find((s) => s.id === id)?.title || 'New thread'

function openThread(id: string) {
  keys.notePick(text.value, id)
  close(false)
  void store.openSession(id)
}

const rows = computed<Row[]>(() => {
  const out: Row[] = []
  const q = text.value
  const projects = store.contexts.filter((c) => c.kind !== 'general' && (!q || matchesWords(q, c.name)))
  const projectRows = projects.map<Row>((c) => ({
    key: `p:${c.id}`,
    section: 'Projects',
    icon: FolderGit2,
    title: c.name,
    sub: 'New thread here',
    hint: '',
    run: () => {
      close(false)
      void sidebar.newThread({ contextId: c.id })
    },
  }))
  if (projectMode.value) return projectRows
  if (!actionsOnly.value) {
    const ranked = rankThreads(paletteThreads.value, q, keys.frecency, keys.picks, Date.now())
    for (const t of ranked) {
      out.push({
        key: `t:${t.id}`,
        section: q ? 'Threads' : 'Recent threads',
        icon: MessageSquare,
        title: t.title || 'New thread',
        sub: [t.project, t.branch, t.pr].filter(Boolean).join(' · '),
        hint: t.id === store.activeId ? 'open' : '',
        run: () => openThread(t.id),
      })
    }
    for (const h of hits.value) {
      out.push({
        key: `m:${h.itemId}`,
        section: 'In messages',
        icon: h.kind === 'user' ? CornerDownLeft : Search,
        title: plainText(h.snippet),
        sub: `${titleOf(h.threadId)} · ${h.kind === 'user' ? 'you' : h.kind === 'plan' ? 'plan' : 'answer'}`,
        hint: '',
        run: () => {
          keys.notePick(text.value, h.threadId)
          close(false)
          threads.reveal = { threadId: h.threadId, itemId: h.itemId, nonce: Date.now() }
          void store.openSession(h.threadId)
        },
      })
    }
    if (q) out.push(...projectRows.slice(0, 4))
  }
  const actions: Row[] = [
    {
      key: 'a:newIn',
      section: 'Actions',
      icon: Plus,
      title: 'New thread in…',
      sub: '',
      hint: '',
      run: () => {
        projectMode.value = true
        query.value = ''
        active.value = 0
        void nextTick(() => input.value?.focus())
      },
    },
    ...COMMANDS.filter((c) => c.palette && hasCommand(c.id)).map<Row>((c) => ({
      key: `a:${c.id}`,
      section: 'Actions',
      icon: c.group === 'Panels' ? SquareTerminal : Zap,
      title: c.title,
      sub: '',
      hint: keys.keyOf(c.id),
      run: () => {
        close(false)
        runCommand(c.id)
      },
    })),
  ].filter((a) => !q || matchesWords(q, a.title))
  out.push(...(q || actionsOnly.value ? actions : actions.slice(0, 8)))
  return out
})

/** Section headers before the first row of each. */
const sections = computed(() => rows.value.map((r, i) => (i === 0 || rows.value[i - 1].section !== r.section ? r.section : '')))

watch(rows, () => {
  if (active.value >= rows.value.length) active.value = Math.max(0, rows.value.length - 1)
})
watch(query, () => (active.value = 0))

// "In messages": from three letters, a moment after typing stops.
let timer: ReturnType<typeof setTimeout> | null = null
let ask = 0
watch(text, (q) => {
  if (timer) clearTimeout(timer)
  if (actionsOnly.value || projectMode.value || q.replace(/\s/g, '').length < 3) {
    hits.value = []
    searching.value = false
    return
  }
  searching.value = true
  timer = setTimeout(async () => {
    const mine = ++ask
    const found = (await pilot<MessageHit[]>('plugin:pilot|pilot_messages_search', { query: q, limit: 8 }).catch(() => [])) ?? []
    if (mine !== ask) return
    hits.value = found
    searching.value = false
  }, 180)
})

const list = ref<HTMLElement | null>(null)
function move(d: number) {
  const n = rows.value.length
  if (!n) return
  active.value = (active.value + d + n) % n
  void nextTick(() => list.value?.querySelector('.on')?.scrollIntoView({ block: 'nearest' }))
}

function pick(i: number) {
  rows.value[i]?.run()
}

/** Closing returns the keyboard to the open thread's message box. */
function close(toComposer = true) {
  keys.closePalette()
  if (toComposer) void nextTick(() => document.querySelector<HTMLTextAreaElement>('.qp-shell .qp-c-input')?.focus())
}

function onKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey
  if (e.key === 'ArrowDown') move(1)
  else if (e.key === 'ArrowUp') move(-1)
  else if (e.key === 'Enter' && !e.isComposing) pick(active.value)
  else if (e.key === 'Escape') {
    if (projectMode.value) projectMode.value = false
    else close()
  } else if (mod && /^Digit[1-9]$/.test(e.code)) pick(Number(e.code.slice(5)) - 1)
  else if (e.key === 'Backspace' && projectMode.value && !query.value) projectMode.value = false
  else return
  e.preventDefault()
  e.stopPropagation()
}

onMounted(() => {
  void nextTick(() => {
    input.value?.focus()
    const v = input.value
    if (v) v.setSelectionRange(v.value.length, v.value.length)
  })
})
onBeforeUnmount(() => {
  if (timer) clearTimeout(timer)
})
</script>

<template>
  <div class="qp-pal-back" @pointerdown.self="close()">
    <div class="qp-pal" role="dialog" aria-label="Command palette" aria-modal="true">
      <div class="qp-pal-field">
        <Search :size="14" />
        <span v-if="projectMode" class="qp-pal-chip">New thread in</span>
        <input
          ref="input"
          v-model="query"
          class="qp-pal-input"
          :placeholder="projectMode ? 'Project' : 'Threads, messages, projects — > for actions'"
          role="combobox"
          aria-controls="qp-pal-list"
          :aria-activedescendant="rows[active] ? `qp-pal-${active}` : undefined"
          spellcheck="false"
          @keydown="onKey"
        />
        <span v-if="searching" class="qp-pal-busy">searching…</span>
      </div>
      <div id="qp-pal-list" ref="list" class="qp-pal-list" role="listbox">
        <template v-for="(r, i) in rows" :key="r.key">
          <div v-if="sections[i]" class="qp-pal-section">{{ sections[i] }}</div>
          <div :id="`qp-pal-${i}`" class="qp-pal-row" :class="{ on: i === active }" role="option" :aria-selected="i === active" @pointermove="active = i" @click="pick(i)">
            <component :is="r.icon" :size="14" class="qp-pal-icon" />
            <span class="qp-pal-text">
              <span class="qp-pal-title">{{ r.title }}</span>
              <span v-if="r.sub" class="qp-pal-sub">{{ r.sub }}</span>
            </span>
            <span v-if="r.hint" class="qp-pal-hint">{{ r.hint }}</span>
            <span v-else-if="i < 9" class="qp-pal-num">Ctrl+{{ i + 1 }}</span>
          </div>
        </template>
        <div v-if="!rows.length" class="qp-pal-empty">{{ searching ? 'Searching messages…' : 'Nothing matches.' }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-pal-back {
  position: absolute;
  inset: 0;
  z-index: 40;
  display: flex;
  justify-content: center;
  align-items: flex-start;
  padding-top: 72px;
  background: color-mix(in srgb, var(--qss-bg, #18181e) 45%, transparent);
}
.qp-pal {
  width: min(640px, calc(100% - 48px));
  max-height: min(520px, calc(100% - 96px));
  display: flex;
  flex-direction: column;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
  overflow: hidden;
  animation: qp-pal-in var(--qp-motion, 170ms) cubic-bezier(0.2, 0.8, 0.2, 1);
}
@keyframes qp-pal-in { from { opacity: 0; transform: translateY(-6px) scale(0.985); } }
@media (prefers-reduced-motion: reduce) { .qp-pal { animation: qp-pal-fade 0.12s ease-out; } }
@keyframes qp-pal-fade { from { opacity: 0; } }
.qp-pal-field { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-chip { flex-shrink: 0; padding: 1px 7px; border-radius: 5px; background: var(--qss-bg-card, #292930); color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-pal-input { flex: 1; min-width: 0; padding: 0; border: none; outline: none; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 13.5px; }
/* The field sits in the palette's own frame: no second focus ring inside it. */
.qp-pal-field .qp-pal-input:focus,
.qp-pal-field .qp-pal-input:focus-visible { outline: none; box-shadow: none; border: none; }
.qp-pal-busy { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 6px 8px; }
.qp-pal-section { padding: 8px 8px 3px; font-size: 11.5px; font-weight: 600; color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-row { display: flex; align-items: center; gap: 9px; padding: 6px 8px; border-radius: 7px; cursor: pointer; color: var(--qss-text, #d4d4d8); }
.qp-pal-row.on { background: var(--qss-bg-hover, #313139); }
.qp-pal-icon { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-text { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; }
.qp-pal-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
.qp-pal-sub { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-hint,
.qp-pal-num { flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-pal-num { opacity: 0; }
.qp-pal-row.on .qp-pal-num,
.qp-pal-row:hover .qp-pal-num { opacity: 0.8; }
.qp-pal-empty { padding: 16px 10px; text-align: center; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
</style>
