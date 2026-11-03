<script setup lang="ts">
/**
 * A QuantPilot chat on the canvas (pre-1.1.0). The window keeps only the
 * thread id in the canvas layout; the thread is a normal QuantPilot thread —
 * in its sidebar, in sync with it, and left alone when the window closes.
 * Without a thread the window shows a start screen in QuantPilot's own look
 * (its empty-thread hero): Bob, a new thread in the canvas's workspace with
 * the agent it gets, and the threads to continue, this workspace's first.
 * File references in the chat open as File windows beside it instead of
 * leaving the canvas.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { ChevronDown, Plus, Search } from 'lucide-vue-next'
import { v4 as uuidv4 } from 'uuid'
import type { CanvasWindow, FilePreviewKind } from '../../../shared/types'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useMascotStore } from '#pilot/stores/mascot'
import { timeAgo } from '#pilot/utils/format'
import type { AgentSetup } from '#pilot/utils/agentMenu'
import type { SessionMeta } from '#pilot/types'

const props = defineProps<{ window: CanvasWindow }>()
const canvasStore = useCanvasStore()
const workspaces = useWorkspacesStore()
const pilot = usePilotStore()
const threads = useThreadStore()
const mascot = useMascotStore()

const workspaceId = computed(() => workspaces.contentWorkspaceId!)
const threadId = computed(() => props.window.chatConfig?.threadId ?? null)
/** The canvas's folder as a QuantPilot project; General for the general canvas. */
const context = computed(() => (workspaces.contentWorkspaceId === workspaces.GENERAL_CONTENT ? 'general' : workspaces.contentWorkspaceId ?? 'general'))
const busy = ref(false)
const error = ref('')
const filter = ref('')

onMounted(() => {
  if (!pilot.loaded) void pilot.load()
})
// The agent and model a new thread gets here, for the button.
watch(context, (c) => void pilot.loadSetup(c), { immediate: true })

const thread = computed(() => (threadId.value ? pilot.sessions.find((s) => s.id === threadId.value) ?? null : null))
const missing = computed(() => Boolean(threadId.value) && pilot.loaded && !thread.value)
// A thread made since the list was read (a crew worker, round 2 card 09): read it again once before calling it gone.
let refreshed = false
watch(missing, (m) => {
  if (m && !refreshed) {
    refreshed = true
    void pilot.refreshSessions()
  }
}, { immediate: true })
// The window is titled after its thread.
watch(
  () => thread.value?.title,
  (t) => {
    const title = t?.trim() ? `Chat · ${t.trim()}` : 'Chat'
    if (props.window.title !== title) canvasStore.updateWindow(workspaceId.value, props.window.id, { title })
  },
  { immediate: true }
)

// ── The start screen ──
const placeName = computed(() => (context.value === 'general' ? 'General' : pilot.contexts.find((c) => c.id === context.value)?.name ?? 'this workspace'))
const agent = computed(() => pilot.newThreadAgent(context.value))
const agentModel = computed(() => {
  const a = agent.value
  const m = a ? pilot.newThreadSetup(context.value, a).model : ''
  if (!a || !m) return ''
  return threads.catalogs[a]?.models.find((x) => x.id === m)?.displayName ?? m.replace(/^claude-/, '')
})

/** Chat threads a window can show (side chats stay with their main thread). */
const chats = computed(() => pilot.sessions.filter((s) => s.kind !== 'terminal' && !s.parentId))
const matches = computed(() => {
  const q = filter.value.trim().toLowerCase()
  return chats.value
    .filter((s) => !q || (s.title || 'New thread').toLowerCase().includes(q) || s.contextName.toLowerCase().includes(q))
    .sort((a, b) => (b.lastActivityAt ?? 0) - (a.lastActivityAt ?? 0))
})
const here = computed(() => matches.value.filter((s) => s.contextId === context.value).slice(0, 30))
const elsewhere = computed(() => matches.value.filter((s) => s.contextId !== context.value).slice(0, 30))
const groups = computed(() =>
  [
    { id: 'here', label: context.value === 'general' ? 'General' : 'This workspace', rows: here.value, offset: 0 },
    { id: 'elsewhere', label: 'Elsewhere', rows: elsewhere.value, offset: here.value.length },
  ].filter((g) => g.rows.length),
)
const flat = computed<SessionMeta[]>(() => [...here.value, ...elsewhere.value])
/** The row Enter opens: the first match, moved with the arrow keys (marked while the search has focus). */
const active = ref(0)
const searching = ref(false)
watch(filter, () => (active.value = 0))
const listEl = ref<HTMLElement | null>(null)
function onSearchKey(e: KeyboardEvent) {
  const n = flat.value.length
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (!n) return
    active.value = (active.value + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
    listEl.value?.querySelector<HTMLElement>(`[data-row="${active.value}"]`)?.scrollIntoView({ block: 'nearest' })
  } else if (e.key === 'Enter') {
    const s = flat.value[active.value]
    if (s) attach(s.id)
  } else if (e.key === 'Escape' && filter.value) {
    e.stopPropagation()
    filter.value = ''
  }
}

function attach(id: string | null) {
  canvasStore.updateWindow(workspaceId.value, props.window.id, { chatConfig: { threadId: id } })
}

/** ▾ beside "New thread": QuantPilot's agent menu (round 2, card 02). */
const newMore = ref<HTMLElement | null>(null)
const menuOpen = ref(false)
async function newThread(setup?: AgentSetup) {
  menuOpen.value = false
  busy.value = true
  error.value = ''
  try {
    // Through QuantPilot's store: the one resolver's setup and its notice (round 2, card 03).
    const t = await pilot.createThread({ contextId: context.value, ...(setup ?? {}), remember: Boolean(setup) }, false)
    if (!t) throw new Error(pilot.lastError || 'The thread could not be created.')
    attach(t.id)
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

// ── Opening a file from the chat: a File window beside this one ──
const IMAGE = /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/i
const MARKDOWN = /\.(md|markdown|mdx)$/i
const CODE: Record<string, string> = { ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript', vue: 'html', rs: 'rust', py: 'python', json: 'json', css: 'css', html: 'html', toml: 'ini', yaml: 'yaml', yml: 'yaml', sh: 'shell', ps1: 'powershell', go: 'go', java: 'java', c: 'c', cpp: 'cpp', cs: 'csharp', sql: 'sql' }
function previewOf(path: string): { kind: FilePreviewKind; language?: string } {
  if (IMAGE.test(path)) return { kind: 'image' }
  if (MARKDOWN.test(path)) return { kind: 'markdown', language: 'markdown' }
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  return CODE[ext] ? { kind: 'code', language: CODE[ext] } : { kind: 'text' }
}
function openFile(path: string, _line: number | null): boolean {
  const cwd = thread.value?.cwd
  const absolute = /^([a-zA-Z]:[\\/]|[\\/]{2}|\/)/.test(path) || !cwd ? path : `${cwd.replace(/[\\/]+$/, '')}/${path}`
  const p = previewOf(absolute)
  const pos = props.window.position
  canvasStore.addWindow(workspaceId.value, {
    id: uuidv4(),
    type: 'file',
    title: absolute.split(/[/\\]/).pop() ?? absolute,
    position: { x: pos.x + pos.width + 24, y: pos.y, width: 520, height: Math.max(320, Math.min(pos.height, 560)) },
    status: 'idle',
    minimized: false,
    zIndex: 0,
    fileConfig: { filePath: absolute, previewKind: p.kind, language: p.language },
  })
  return true
}
const host = { kind: 'canvas', openFile, openLabel: 'Open in a File window' }

/** Keys meant for the chat stay there: Space does not arm canvas panning, Ctrl+V does not paste a window. */
function keepKeys(e: KeyboardEvent) {
  if (e.code === 'Space' || (e.ctrlKey && e.key.toLowerCase() === 'v')) e.stopPropagation()
}
</script>

<template>
  <div class="canvas-chat" @keydown="keepKeys">
    <PilotEmbedChat v-if="threadId && !missing" :thread-id="threadId" :host="host" />
    <div v-else class="canvas-chat__start">
      <div class="canvas-chat__col">
        <header class="canvas-chat__hero">
          <div v-if="mascot.prefs.on" class="canvas-chat__face"><PilotFace state="idle" :agent="agent" /></div>
          <h2 class="canvas-chat__heading">Chat in {{ placeName }}</h2>
          <p class="canvas-chat__sub">A QuantPilot thread on the canvas — it is in QuantPilot's sidebar too.</p>
        </header>
        <p v-if="missing" class="canvas-chat__gone" role="status">This window's thread no longer exists. Start a new one or continue another.</p>
        <div class="canvas-chat__split">
          <button class="canvas-chat__new" :disabled="busy" @click="newThread()">
            <PilotAgentIcon v-if="agent" :agent="agent" :size="16" decorative />
            <Plus v-else :size="15" />
            <span class="canvas-chat__new-text">
              <span class="canvas-chat__new-main">{{ busy ? 'Starting…' : 'New thread' }}</span>
              <span v-if="agent" class="canvas-chat__new-meta">{{ pilot.adapterLabel(agent) }}{{ agentModel ? ` · ${agentModel}` : '' }}</span>
            </span>
          </button>
          <button ref="newMore" class="canvas-chat__more" :disabled="busy" title="With another agent or model" aria-label="With another agent or model" aria-haspopup="menu" :aria-expanded="menuOpen" @click.stop="menuOpen = !menuOpen"><ChevronDown :size="15" /></button>
        </div>
        <PilotAgentMenu v-if="menuOpen" :anchor="newMore" title="New thread with" :project="context" @pick="newThread" @close="menuOpen = false" />
        <p v-if="error" class="canvas-chat__err" role="alert">{{ error }}</p>

        <template v-if="chats.length">
          <div class="canvas-chat__divider"><span>or continue a thread</span></div>
          <label class="canvas-chat__search">
            <Search :size="13" />
            <input v-model="filter" placeholder="Search threads" aria-label="Search threads" spellcheck="false" @keydown="onSearchKey" @focus="searching = true" @blur="searching = false" />
          </label>
          <div ref="listEl" class="canvas-chat__list" role="listbox" aria-label="Threads">
            <template v-for="g in groups" :key="g.id">
              <div class="canvas-chat__group">{{ g.label }}</div>
              <button
                v-for="(s, i) in g.rows"
                :key="s.id"
                class="canvas-chat__row"
                :class="{ active: searching && g.offset + i === active }"
                role="option"
                :aria-selected="g.offset + i === active"
                :data-row="g.offset + i"
                @pointerenter="active = g.offset + i"
                @click="attach(s.id)"
              >
                <PilotAgentIcon :agent="s.provider || null" :size="13" decorative />
                <span class="canvas-chat__row-title">{{ s.title || 'New thread' }}</span>
                <span v-if="s.contextId !== context" class="canvas-chat__row-ctx">{{ s.contextName }}</span>
                <span class="canvas-chat__row-ago">{{ timeAgo(s.lastActivityAt) }}</span>
              </button>
            </template>
            <p v-if="!flat.length" class="canvas-chat__none">No thread matches “{{ filter.trim() }}”.</p>
          </div>
        </template>
        <p v-else-if="pilot.loaded" class="canvas-chat__none">No threads yet — the first one starts here.</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.canvas-chat { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; background: var(--qss-bg, #18181e); }
/* The start screen: a centred column that scrolls inside the window when it is small. */
.canvas-chat__start { flex: 1; min-height: 0; display: flex; overflow-y: auto; container-type: size; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.canvas-chat__col { margin: auto; width: min(440px, 100%); display: flex; flex-direction: column; gap: 12px; padding: 28px 20px; box-sizing: border-box; }
.canvas-chat__hero { display: flex; flex-direction: column; align-items: center; gap: 6px; text-align: center; }
.canvas-chat__face { width: 84px; margin-bottom: 2px; }
.canvas-chat__heading { margin: 0; font-size: 17px; font-weight: 600; color: var(--qss-text, #d4d4d8); overflow-wrap: anywhere; }
.canvas-chat__sub { margin: 0; max-width: 340px; font-size: 12px; line-height: 1.5; color: var(--qss-text-muted, #6e6e7a); }
.canvas-chat__gone { margin: 0; padding: 7px 10px; border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, transparent); border-radius: 8px; background: color-mix(in srgb, var(--qss-warning, #d29a3f) 8%, transparent); color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; line-height: 1.45; }
/* One primary button with the agent menu attached. */
.canvas-chat__split { display: flex; align-self: stretch; }
.canvas-chat__new,
.canvas-chat__more { border: 1px solid var(--qss-text, #d4d4d8); background: var(--qss-text, #d4d4d8); color: var(--qss-bg, #18181e); font: inherit; cursor: pointer; }
.canvas-chat__new { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-radius: 9px 0 0 9px; text-align: left; }
.canvas-chat__more { display: inline-flex; align-items: center; justify-content: center; width: 38px; flex-shrink: 0; border-left-color: color-mix(in srgb, var(--qss-bg, #18181e) 25%, var(--qss-text, #d4d4d8)); border-radius: 0 9px 9px 0; }
.canvas-chat__new:hover:not(:disabled),
.canvas-chat__more:hover:not(:disabled) { background: #fff; }
.canvas-chat__new:disabled,
.canvas-chat__more:disabled { opacity: 0.55; cursor: default; }
.canvas-chat__new-text { min-width: 0; display: flex; flex-direction: column; line-height: 1.25; }
.canvas-chat__new-main { font-weight: 600; font-size: 13px; }
.canvas-chat__new-meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; opacity: 0.7; }
.canvas-chat__err { margin: 0; color: var(--qss-error, #f87171); font-size: 12px; }
/* Threads to continue. */
.canvas-chat__divider { display: flex; align-items: center; gap: 10px; margin-top: 6px; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.canvas-chat__divider::before,
.canvas-chat__divider::after { content: ''; flex: 1; height: 1px; background: var(--qss-border-subtle, #35353d); }
.canvas-chat__search { display: flex; align-items: center; gap: 7px; height: 32px; padding: 0 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); color: var(--qss-text-muted, #6e6e7a); }
.canvas-chat__search:focus-within { border-color: var(--qss-accent, #a0a0a8); }
.canvas-chat__search input { flex: 1; min-width: 0; height: 100%; padding: 0; border: none; outline: none; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; }
.canvas-chat__search input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.canvas-chat__list { display: flex; flex-direction: column; gap: 1px; }
.canvas-chat__group { padding: 8px 8px 4px; font-size: 11.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.canvas-chat__row { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border: none; border-radius: 7px; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.canvas-chat__row:hover,
.canvas-chat__row.active { background: var(--qss-bg-hover, #313139); }
.canvas-chat__row-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.canvas-chat__row-ctx { max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex-shrink: 1; color: var(--qss-text-secondary, #9a9aa5); font-size: 11.5px; }
.canvas-chat__row-ago { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 11px; font-variant-numeric: tabular-nums; }
.canvas-chat__none { margin: 0; padding: 6px 8px; color: var(--qss-text-muted, #6e6e7a); font-size: 12px; text-align: center; }
/* A short window keeps the button and the threads; Bob and the line step aside. */
@container (max-height: 420px) {
  .canvas-chat__face,
  .canvas-chat__sub { display: none; }
  .canvas-chat__col { padding-top: 16px; padding-bottom: 16px; }
}
</style>
