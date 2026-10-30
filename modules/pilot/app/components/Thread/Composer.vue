<script setup lang="ts">
/**
 * The message box (docs/PLAN-QUANTPILOT.md, card 4; T3 Code's composer).
 *
 * - Markdown as you type: a coloured backdrop under a transparent field —
 *   only colours change, so the caret never drifts. What is typed is sent.
 * - Keys (utils/composer.ts): the send key follows the setting; while the
 *   agent works it queues or steers (setting), Ctrl flips it, Ctrl+Shift+
 *   Enter steers the oldest queued message, Ctrl+Alt+Enter sends and opens
 *   a fresh thread. Shift+Enter continues lists and quotes, Tab nests,
 *   Enter inside a code block stays in it. Edits go through the browser's
 *   own insertText, so Ctrl+Z keeps working.
 * - ArrowUp in an empty box recalls earlier prompts (ArrowDown forward),
 *   Alt+Up edits the newest queued message, Ctrl+S stashes or restores.
 * - A paste of 32 KiB or more becomes a file the agent reads on demand
 *   (Ctrl+Shift+V keeps it inline). 120,000 characters at most.
 * - Context (card 9): `@` files of the checkout and other threads, `/`
 *   commands (QuantPilot's own run here, the agent's are sent), `$`
 *   skills; chips sit in the text and delete like one character;
 *   attachments (paste, drop anywhere, the + menu) wait on a shelf and
 *   must finish saving before the message can go.
 * - Drafts, pasted files, attachments, chips and stashes live in this
 *   device's storage.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ArrowUp, Ban, CornerDownRight, FileText, Folder, ListPlus, Loader2, Paperclip, Play, Plus, Square, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { caretLine } from '#pilot/utils/caret'
import { pilot } from '#pilot/utils/invoke'
import {
  appCommand,
  chipNextTo,
  chipsIn,
  chipToken,
  commandRows,
  outsideChip,
  plainText,
  replaceTrigger,
  triggerAt,
  uniqueLabel,
  type Trigger,
} from '#pilot/utils/mentions'
import type { MenuRow } from '#pilot/components/Thread/MentionMenu.vue'
import type { ComposerAttachment, ContextChip, SessionMeta, ThreadSnapshot } from '#pilot/types'
import {
  enterAction,
  highlight,
  indent,
  LARGE_PASTE_BYTES,
  MAX_CHARS,
  newLine,
  sizeLabel,
  type Edit,
  type KeyAction,
} from '#pilot/utils/composer'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const store = usePilotStore()

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const running = computed(() => Boolean(view.value?.turnId))
const sendShortcut = computed(() => store.settings?.sendShortcut ?? 'enter')
const followUp = computed(() => store.settings?.followUp ?? 'queue')

// ── Device storage: drafts, pasted files, stashes ──
interface Pasted {
  path: string
  size: number
}
interface Stash {
  id: string
  text: string
  pasted: Pasted[]
  threadTitle: string
  at: number
}

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}
function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: the draft lives for this session only */
  }
}

const drafts = useState<Record<string, string>>('pilot-drafts', () => load('pilot.drafts.v1', {}))
/** Attachments waiting per thread (saved ones only are kept across restarts). */
const attachState = useState<Record<string, ComposerAttachment[]>>('pilot-attach', () => load('pilot.attach.v1', {}))
/** Per thread: chip label → what it stands for. */
const chipState = useState<Record<string, Record<string, ContextChip>>>('pilot-chips', () => load('pilot.chips.v1', {}))
const pastes = useState<Record<string, Pasted[]>>('pilot-pastes', () => load('pilot.pasted.v1', {}))
const stashes = useState<Stash[]>('pilot-stash', () => load('pilot.stash.v1', []))
let saveTimer: ReturnType<typeof setTimeout> | null = null
function persistSoon() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    save('pilot.drafts.v1', drafts.value)
    save('pilot.pasted.v1', pastes.value)
    persistContext()
  }, 300)
}

function persistContext() {
  const ready: Record<string, ComposerAttachment[]> = {}
  for (const [id, list] of Object.entries(attachState.value)) {
    const kept = list.filter((a) => !a.pending && !a.error)
    if (kept.length) ready[id] = kept
  }
  save('pilot.attach.v1', ready)
  save('pilot.chips.v1', chipState.value)
}

const attachments = computed<ComposerAttachment[]>({
  get: () => attachState.value[props.threadId] ?? [],
  set: (v) => {
    if (v.length) attachState.value[props.threadId] = v
    else delete attachState.value[props.threadId]
    persistSoon()
  },
})
const chipMap = computed(() => chipState.value[props.threadId] ?? {})

const text = computed({
  get: () => drafts.value[props.threadId] ?? '',
  set: (v: string) => {
    if (v) drafts.value[props.threadId] = v
    else delete drafts.value[props.threadId]
    persistSoon()
  },
})
const pasted = computed<Pasted[]>({
  get: () => pastes.value[props.threadId] ?? [],
  set: (v) => {
    if (v.length) pastes.value[props.threadId] = v
    else delete pastes.value[props.threadId]
    persistSoon()
  },
})

// ── The field ──
const box = ref<HTMLTextAreaElement | null>(null)
const mirror = ref<HTMLElement | null>(null)
const MAX_HEIGHT = 280
const hl = computed(() => highlight(text.value))

function grow() {
  const el = box.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`
  syncScroll()
}
function syncScroll() {
  if (mirror.value && box.value) mirror.value.scrollTop = box.value.scrollTop
}
watch(text, () => void nextTick(grow))
/** Opening a thread focuses the box — unless the keyboard is browsing the sidebar (T3 Code: F2 and the menu still act on the row). */
function focusOnOpen() {
  if (document.activeElement?.closest('.qp-side-list')) return
  box.value?.focus()
}
watch(
  () => props.threadId,
  () => {
    recall.value = null
    void nextTick(() => {
      grow()
      focusOnOpen()
    })
  }
)

/**
 * Replace the changed middle of the text through the browser's own
 * insertText, so native undo still works; fall back to setting the value.
 */
function applyEdit(e: Edit) {
  const el = box.value
  if (!el) return
  const old = el.value
  let p = 0
  while (p < old.length && p < e.text.length && old[p] === e.text[p]) p++
  let s = 0
  while (s < old.length - p && s < e.text.length - p && old[old.length - 1 - s] === e.text[e.text.length - 1 - s]) s++
  el.setSelectionRange(p, old.length - s)
  const ok = document.execCommand('insertText', false, e.text.slice(p, e.text.length - s))
  if (!ok) el.value = e.text
  text.value = el.value
  el.setSelectionRange(e.start, e.end)
}

const tooLong = computed(() => text.value.length > MAX_CHARS)
const counter = computed(() =>
  text.value.length > MAX_CHARS * 0.85 ? `${text.value.length.toLocaleString('en-US')} / ${MAX_CHARS.toLocaleString('en-US')}` : ''
)

// ── Sending ──
const busy = ref(false)
const modHeld = ref(false)

function compose(): string {
  const refs = pasted.value.map(
    (p) => `[Pasted text (${sizeLabel(p.size)}) saved to ${p.path} — read it with your file tools.]`
  )
  return [text.value.trimEnd(), ...refs].filter(Boolean).join('\n\n')
}

const hasContent = computed(() => text.value.trim().length > 0 || pasted.value.length > 0 || attachments.value.length > 0)
const uploading = computed(() => attachments.value.some((a) => a.pending))

/** The chips still in the text and the attachments that saved — what goes with the message. */
function extras() {
  // Plain copies: what goes over IPC is data, not reactive state.
  const chips = chipsIn(text.value)
    .map((label) => chipMap.value[label])
    .filter((c): c is ContextChip => Boolean(c))
    .map((c) => ({ kind: c.kind, label: c.label, content: c.content, path: c.path ?? null }))
  const files = attachments.value.filter((a) => !a.pending && !a.error).map((a) => ({ kind: a.kind, name: a.name, path: a.path, thumb: a.thumb ?? null }))
  return { chips, attachments: files }
}

/**
 * One message to several models: a thread and a worktree each, all started
 * in the background while this draft stays open (card 10).
 */
async function fanOut(picks: { provider: string; model: string }[], message: string, more: ReturnType<typeof extras>): Promise<boolean> {
  const t = thread.value
  const info = useGitStore().info[props.threadId]
  if (!t || !info?.repo) {
    showNote('Several models need a git project: each one works in its own worktree.')
    return false
  }
  for (const p of picks) {
    const meta = await store.createThread(
      {
        contextId: t.contextId,
        mode: t.mode,
        provider: p.provider,
        model: p.model,
        workspaceMode: 'worktree',
        baseBranch: info.baseBranch || info.repo.branch,
      },
      false
    )
    if (meta) await threads.send(meta.id, message, more)
  }
  delete threads.fanout[props.threadId]
  showNote(`Started ${picks.length} threads, one worktree each.`)
  return true
}

/** QuantPilot's own `/` commands run here instead of being sent. */
async function runAppCommand(name: string, arg: string): Promise<boolean> {
  const id = props.threadId
  switch (name) {
    case 'new':
      await useSidebarStore().newThread({ contextId: thread.value?.contextId })
      return true
    case 'model':
    case 'permissions':
      window.dispatchEvent(new CustomEvent('qp-open-control', { detail: { threadId: id, which: name === 'model' ? 'model' : 'mode' } }))
      return true
    case 'plan':
      return store.update(id, { mode: 'plan' })
    case 'rename':
      if (!arg) {
        showNote('Add the new title: /rename <title>')
        return false
      }
      return store.update(id, { title: arg })
    case 'copy': {
      const last = [...(view.value?.items ?? [])].reverse().find((i) => i.kind === 'assistant' && String(i.data.text ?? '').trim())
      if (!last) {
        showNote('Nothing to copy yet.')
        return false
      }
      await navigator.clipboard.writeText(String(last.data.text)).catch(() => undefined)
      showNote('Copied the latest answer.')
      return true
    }
    case 'pwd':
      showNote(thread.value?.cwd ?? '')
      return true
    case 'fork': {
      const last = [...(view.value?.items ?? [])].reverse().find((i) => i.kind === 'user' || i.kind === 'assistant')
      if (!last) {
        showNote('Nothing to fork yet.')
        return false
      }
      return Boolean(await threads.fork(id, last.id, false))
    }
    case 'restart':
      if (await threads.restart(id)) {
        showNote('The agent starts fresh on the next message; the conversation continues.')
        return true
      }
      return false
    case 'tui':
      await threads.openTui(id)
      return true
  }
  return false
}
const note = ref('')
let noteTimer: ReturnType<typeof setTimeout> | null = null
function showNote(text: string) {
  note.value = text
  if (noteTimer) clearTimeout(noteTimer)
  noteTimer = setTimeout(() => (note.value = ''), 5000)
}
/** The first approval waiting — a sent reply declines it. */
const pendingApproval = computed(() => threads.pending(props.threadId).find((i) => i.kind === 'approval') ?? null)

/** What the button (and the send key) does right now. */
const buttonAction = computed<KeyAction>(() =>
  enterAction({
    shift: false,
    mod: modHeld.value,
    alt: false,
    shortcut: 'enter',
    followUp: followUp.value,
    running: running.value,
    canSteer: Boolean(view.value?.canSteer),
    text: 'x',
    caret: 1,
  })
)

async function perform(action: KeyAction) {
  if (busy.value) return
  const id = props.threadId
  if (action === 'steerQueued') {
    const first = view.value?.queue[0]
    if (first) await threads.queueSendNow(id, first.id)
    return
  }
  if (!hasContent.value || tooLong.value || uploading.value) return
  const command = !pasted.value.length && !attachments.value.length ? appCommand(text.value) : null
  if (command) {
    if (await runAppCommand(command.name, command.arg)) {
      text.value = ''
      menu.value = null
    }
    return
  }
  const message = compose()
  const more = extras()
  const fan = threads.fanout[id]
  if (fan && fan.length > 1 && !running.value) {
    busy.value = true
    const done = await fanOut(fan, message, more)
    busy.value = false
    if (done) {
      text.value = ''
      pasted.value = []
      attachments.value = []
      delete chipState.value[id]
      persistSoon()
    }
    return
  }
  busy.value = true
  let ok = false
  if (pendingApproval.value && action !== 'sendNewThread') {
    // A reply while a request waits declines it, with the reply as the reason (T3 Code).
    await threads.answer(id, String(pendingApproval.value.data.requestId), { decision: 'deny', message: plainText(message) })
    ok = !view.value?.error
  } else if (action === 'queue') ok = await threads.queueAdd(id, message, more)
  else if (action === 'steer') ok = await threads.steer(id, message, more)
  else if (view.value?.compactFirst) {
    // Compact first, the message follows from the queue (card 15).
    ok = (await threads.send(id, '/compact')) && (await threads.queueAdd(id, message, more))
    if (ok && view.value) view.value.compactFirst = null
  } else ok = await threads.send(id, message, more)
  busy.value = false
  if (!ok) return
  text.value = ''
  pasted.value = []
  attachments.value = []
  delete chipState.value[id]
  persistSoon()
  menu.value = null
  recall.value = null
  if (action === 'sendNewThread' && thread.value) {
    // The next draft keeps the project, mode and workspace (T3 Code's background start).
    const t = thread.value
    await store.createThread({ contextId: t.contextId, mode: t.mode, provider: t.provider, workspaceMode: t.workspaceMode, baseBranch: t.baseBranch || undefined })
  }
  void nextTick(() => box.value?.focus())
}

function clickPrimary() {
  if (running.value && !hasContent.value) {
    void threads.interrupt(props.threadId)
    return
  }
  if (!running.value && !hasContent.value && view.value?.queueHeld && view.value.queue.length) {
    void threads.queueResume(props.threadId)
    return
  }
  if (primary.value.kind === 'continue') {
    void threads.resumeTurn(props.threadId)
    return
  }
  void perform(running.value ? buttonAction.value : 'send')
}

// ── Prompt recall ──
const recall = ref<{ index: number; text: string } | null>(null)
const history = computed(() =>
  (view.value?.items ?? [])
    .filter((i) => i.kind === 'user' && String(i.data.text ?? '').trim())
    .map((i) => String(i.data.text))
)

function recallStep(dir: 1 | -1): boolean {
  const list = history.value
  if (!list.length) return false
  const next = (recall.value?.index ?? -1) + dir
  if (next < 0) {
    recall.value = null
    text.value = ''
    return true
  }
  if (next >= list.length) return true
  const t = list[list.length - 1 - next]
  recall.value = { index: next, text: t }
  text.value = t
  void nextTick(() => {
    const el = box.value
    if (!el) return
    const pos = dir > 0 ? 0 : el.value.length
    el.setSelectionRange(pos, pos)
  })
  return true
}

// ── Editing a queued message ──
const editing = ref<{ itemId: string; previous: string } | null>(null)

watch(
  () => view.value?.editingQueued,
  (itemId) => {
    if (!itemId || editing.value?.itemId === itemId) return
    const item = view.value?.queue.find((q) => q.id === itemId)
    if (!item) return
    editing.value = { itemId, previous: text.value }
    text.value = item.text
    void nextTick(() => box.value?.focus())
  }
)
watch(
  () => view.value?.queue,
  (queue) => {
    const e = editing.value
    if (!e || queue?.some((q) => q.id === e.itemId)) return
    // It was sent or removed meanwhile: keep the edit only if nothing was waiting in the box.
    if (e.previous) text.value = e.previous
    editing.value = null
    threads.editQueued(props.threadId, null)
  }
)

async function saveEdit() {
  const e = editing.value
  if (!e) return
  if (!text.value.trim()) {
    await threads.queueRemove(props.threadId, e.itemId)
  } else if (!(await threads.queueUpdate(props.threadId, e.itemId, text.value))) {
    return
  }
  text.value = e.previous
  editing.value = null
  threads.editQueued(props.threadId, null)
}
function cancelEdit() {
  const e = editing.value
  if (!e) return
  text.value = e.previous
  editing.value = null
  threads.editQueued(props.threadId, null)
}

// ── Stash ──
const stashOpen = ref(false)
function persistStash() {
  save('pilot.stash.v1', stashes.value)
}
function stash() {
  if (!hasContent.value) {
    if (stashes.value.length === 1) restore(stashes.value[0].id)
    else if (stashes.value.length > 1) stashOpen.value = !stashOpen.value
    return
  }
  stashes.value = [
    { id: crypto.randomUUID(), text: text.value, pasted: pasted.value, threadTitle: thread.value?.title || 'New thread', at: Date.now() },
    ...stashes.value,
  ].slice(0, 20)
  persistStash()
  text.value = ''
  pasted.value = []
}
function restore(id: string) {
  const s = stashes.value.find((x) => x.id === id)
  if (!s) return
  if (hasContent.value) return
  text.value = s.text
  pasted.value = s.pasted
  stashes.value = stashes.value.filter((x) => x.id !== id)
  persistStash()
  stashOpen.value = false
  void nextTick(() => box.value?.focus())
}
function dropStash(id: string) {
  stashes.value = stashes.value.filter((x) => x.id !== id)
  persistStash()
}

// ── Keys ──
let keepPasteInline = false
function onKey(e: KeyboardEvent) {
  const el = box.value
  if (!el || e.isComposing) return
  const mod = e.ctrlKey || e.metaKey
  if (menu.value && menuKey(e)) return
  if ((e.key === 'Backspace' || e.key === 'Delete') && !mod && el.selectionStart === el.selectionEnd) {
    const range = chipNextTo(el.value, el.selectionStart, e.key === 'Backspace' ? -1 : 1)
    if (range) {
      e.preventDefault()
      el.setSelectionRange(range[0], range[1])
      document.execCommand('delete')
      onInput()
      return
    }
  }
  if (e.key === 'Enter') {
    if (editing.value && !e.shiftKey) {
      e.preventDefault()
      void saveEdit()
      return
    }
    // A new thread: Ctrl+Enter starts it in the background and opens the next draft (T3 Code).
    if (mod && !e.shiftKey && !e.altKey && sendShortcut.value === 'enter' && !running.value && !thread.value?.providerSessionId) {
      e.preventDefault()
      void perform('sendNewThread')
      return
    }
    const action = enterAction({
      shift: e.shiftKey,
      mod,
      alt: e.altKey,
      shortcut: sendShortcut.value,
      followUp: followUp.value,
      running: running.value,
      canSteer: Boolean(view.value?.canSteer),
      text: el.value,
      caret: el.selectionStart,
    })
    e.preventDefault()
    if (action === 'newline') applyEdit(newLine(el.value, el.selectionStart, el.selectionEnd))
    else if (action !== 'none') void perform(action)
    return
  }
  if (e.key === 'Tab' && !mod && !e.altKey) {
    const edit = indent(el.value, el.selectionStart, el.selectionEnd, e.shiftKey ? -1 : 1)
    if (edit) {
      e.preventDefault()
      applyEdit(edit)
    }
    return
  }
  if (e.key === 'ArrowUp' && e.altKey) {
    const last = view.value?.queue[view.value.queue.length - 1]
    if (last && el.selectionStart === 0) {
      e.preventDefault()
      threads.editQueued(props.threadId, last.id)
    }
    return
  }
  if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !mod && !e.shiftKey && !e.altKey && !editing.value) {
    const recalled = recall.value && el.value === recall.value.text
    const empty = !el.value && !pasted.value.length
    if (e.key === 'ArrowUp' && (empty || (recalled && caretLine(el).first))) {
      if (recallStep(1)) e.preventDefault()
    } else if (e.key === 'ArrowDown' && recalled && caretLine(el).last) {
      if (recallStep(-1)) e.preventDefault()
    }
    return
  }
  if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 's') {
    e.preventDefault()
    stash()
    return
  }
  if (mod && e.shiftKey && e.key.toLowerCase() === 'v') {
    keepPasteInline = true
    setTimeout(() => (keepPasteInline = false), 600)
    return
  }
  if (e.key === 'Escape') {
    if (editing.value) {
      e.preventDefault()
      cancelEdit()
    } else if (stashOpen.value) {
      stashOpen.value = false
    }
  }
}

function onInput() {
  const el = box.value
  if (!el) return
  text.value = el.value
  // Typing over a recalled prompt makes it a normal draft.
  if (recall.value && el.value !== recall.value.text) recall.value = null
  updateMenu()
}

/** A click or arrow into a chip lands beside it; the menu follows the caret. */
function onCaret(e: Event) {
  const el = box.value
  if (!el) return
  if (el.selectionStart === el.selectionEnd) {
    const back = e instanceof KeyboardEvent && (e.key === 'ArrowLeft' || e.key === 'Home')
    const at = outsideChip(el.value, el.selectionStart, back ? -1 : 1)
    if (at !== el.selectionStart) el.setSelectionRange(at, at)
  }
  if (e instanceof KeyboardEvent && ['ArrowUp', 'ArrowDown', 'Enter', 'Tab', 'Escape'].includes(e.key) && menu.value) return
  updateMenu()
}

// ── The @ / / / $ menu ──
interface Pick {
  row: MenuRow
  run: () => void | Promise<void>
}
const menu = ref<{ trigger: Trigger; picks: Pick[]; active: number; loading: boolean } | null>(null)
const menuRows = computed(() => menu.value?.picks.map((p) => p.row) ?? [])
const MENU_TITLES = { '@': 'Files, folders and threads', '/': 'Commands', $: 'Skills' } as const
let menuSeq = 0
let menuTimer: ReturnType<typeof setTimeout> | null = null

function updateMenu() {
  const el = box.value
  const t = el && el.selectionStart === el.selectionEnd ? triggerAt(el.value, el.selectionStart) : null
  if (!t) {
    menu.value = null
    return
  }
  const same = menu.value && menu.value.trigger.kind === t.kind && menu.value.trigger.start === t.start
  menu.value = { trigger: t, picks: same ? menu.value!.picks : [], active: same ? menu.value!.active : 0, loading: true }
  if (menuTimer) clearTimeout(menuTimer)
  const seq = ++menuSeq
  menuTimer = setTimeout(async () => {
    const picks = await picksFor(t)
    // A newer keystroke asked again: this answer is stale.
    if (seq !== menuSeq || !menu.value) return
    menu.value = { trigger: t, picks, active: Math.min(menu.value.active, Math.max(0, picks.length - 1)), loading: false }
  }, t.kind === '@' ? 90 : 0)
}

function basename(p: string) {
  const parts = p.replace(/\/$/, '').split('/')
  return parts[parts.length - 1] + (p.endsWith('/') ? '/' : '')
}

async function picksFor(t: Trigger): Promise<Pick[]> {
  const id = props.threadId
  const q = t.query.toLowerCase()
  if (t.kind === '@') {
    const hits = await threads.searchFiles(id, t.query)
    const others: Pick[] = store.sessions
      .filter((s) => s.id !== id && s.kind === 'chat' && s.title && (!q || s.title.toLowerCase().includes(q)))
      .slice(0, 6)
      .map((s) => ({
        row: { key: `t:${s.id}`, label: s.title, detail: `Thread · ${s.contextName}`, icon: 'thread', badge: 'Thread' },
        run: () => insertThread(t, s),
      }))
    // 40 rows at most, and matching threads always get theirs.
    const files: Pick[] = hits.slice(0, 40 - others.length).map((h) => ({
      row: { key: `f:${h.path}`, label: basename(h.path), detail: h.path, icon: h.dir ? 'folder' : 'file' },
      run: () => insertText(t, h.insert),
    }))
    return [...files, ...others]
  }
  if (t.kind === '/') {
    const provider = thread.value?.provider || 'claude'
    const catalog = await threads.catalog(provider)
    return commandRows(catalog?.commands ?? [], t.query).map((c) => ({
      row: {
        key: `c:${c.source}:${c.name}`,
        label: `/${c.name}${c.argumentHint ? ' ' + c.argumentHint : ''}`,
        detail: c.description,
        icon: 'command',
        badge: c.source === 'app' ? 'QuantPilot' : store.adapterLabel(provider),
      },
      run: () => insertText(t, `/${c.name}`),
    }))
  }
  const skills = await threads.skills(id)
  return skills
    .filter((k) => !q || k.name.toLowerCase().includes(q) || k.description.toLowerCase().includes(q))
    .slice(0, 40)
    .map((k) => ({
      row: { key: `s:${k.name}`, label: `$${k.name}`, detail: k.description, icon: 'skill', badge: k.source },
      run: () => insertChip(t, { kind: 'skill', label: `$${k.name}`, content: k.name, path: k.path || null }),
    }))
}

function menuKey(e: KeyboardEvent): boolean {
  const m = menu.value
  if (!m) return false
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    menu.value = null
    return true
  }
  if (!m.picks.length) return false
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    const n = m.picks.length
    m.active = (m.active + (e.key === 'ArrowDown' ? 1 : -1) + n) % n
    return true
  }
  if ((e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.metaKey) || e.key === 'Tab') {
    e.preventDefault()
    void pickMenu(m.active)
    return true
  }
  return false
}

async function pickMenu(i: number) {
  const pick = menu.value?.picks[i]
  menu.value = null
  if (pick) await pick.run()
  void nextTick(() => box.value?.focus())
}

/** Replace the trigger with text through the browser's own editing, so Ctrl+Z undoes it. */
function insertText(t: Trigger, insert: string) {
  const el = box.value
  if (!el) return
  const caret = el.selectionStart
  const next = replaceTrigger(el.value, t, caret, insert)
  el.setSelectionRange(t.start, caret)
  document.execCommand('insertText', false, next.text.slice(t.start, next.caret))
  onInput()
  menu.value = null
}

function insertChip(t: Trigger | null, chip: ContextChip) {
  const el = box.value
  if (!el) return
  const label = uniqueLabel(chip.label, el.value)
  chipState.value[props.threadId] = { ...chipMap.value, [label]: { ...chip, label } }
  persistSoon()
  if (t) insertText(t, chipToken(label))
  else {
    document.execCommand('insertText', false, chipToken(label) + ' ')
    onInput()
  }
}

/** Another thread as context: its recent conversation, so the agent can read it now. */
async function insertThread(t: Trigger, other: SessionMeta) {
  let transcript = ''
  try {
    const snap = await pilot<ThreadSnapshot>('plugin:pilot|pilot_thread_snapshot', { id: other.id, limit: 60 })
    const lines = (snap?.items ?? [])
      .filter((i) => (i.kind === 'user' || i.kind === 'assistant') && !i.parent && String(i.data.text ?? '').trim())
      .map((i) => `${i.kind === 'user' ? 'User' : 'Agent'}: ${plainText(String(i.data.text)).trim()}`)
    transcript = lines.join('\n\n')
    if (transcript.length > 12_000) transcript = '…' + transcript.slice(-12_000)
  } catch {
    transcript = ''
  }
  insertChip(t, {
    kind: 'thread',
    label: other.title,
    content: `QuantPilot thread "${other.title}" (${other.contextName}), recent conversation:\n\n${transcript || '(no messages yet)'}`,
    path: null,
  })
}

/** Another card (terminal excerpt, diff comment, element) adds context at the caret. */
function addContext(chip: ContextChip) {
  box.value?.focus()
  insertChip(null, chip)
}

/** "Edit from here" put a sent message back: its text, chips and attachments (card 11). */
function onComposerSet(e: Event) {
  const d = (e as CustomEvent<{ threadId: string; text: string; attachments?: Record<string, any>[] }>).detail
  if (d?.threadId !== props.threadId) return
  const kept = Array.isArray(d.attachments) ? d.attachments : []
  const chips: Record<string, ContextChip> = {}
  for (const a of kept.filter((x) => x.kind === 'chip' && x.label)) {
    chips[a.label] = { kind: String(a.chipKind ?? 'file'), label: String(a.label), content: String(a.content ?? ''), path: a.path ?? null }
  }
  chipState.value[props.threadId] = chips
  attachments.value = kept
    .filter((x) => x.kind === 'image' || x.kind === 'file' || x.kind === 'folder')
    .map((x) => ({ key: crypto.randomUUID(), kind: x.kind, name: String(x.name ?? ''), path: String(x.path ?? ''), size: 0, thumb: x.thumb ?? null }))
  text.value = d.text
  void nextTick(() => {
    grow()
    const el = box.value
    el?.focus()
    el?.setSelectionRange(el.value.length, el.value.length)
  })
}
/** A saved attachment from elsewhere — the browser's screenshot (card 14), at most four per message. */
function onAddAttachment(e: Event) {
  const d = (e as CustomEvent<{ threadId: string; attachment: Record<string, any> }>).detail
  if (d?.threadId !== props.threadId || !d.attachment?.path) return
  const shots = attachments.value.filter((a) => a.name.startsWith('screenshot')).length
  if (shots >= 4) {
    showNote('A message can carry four browser screenshots.')
    return
  }
  const a = d.attachment
  attachments.value = [...attachments.value, { key: crypto.randomUUID(), kind: 'image', name: String(a.name), path: String(a.path), size: Number(a.size) || 0, thumb: a.thumb ?? null }]
}
function onAddContext(e: Event) {
  const d = (e as CustomEvent<{ threadId: string; chip: ContextChip }>).detail
  if (d?.threadId === props.threadId && d.chip) addContext(d.chip)
}

// ── Attachments ──
const IMAGE_TYPES = /^image\/(png|jpeg|webp)$/
const MAX_ATTACHMENTS = 100

function readData(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result))
    r.onerror = () => reject(r.error)
    r.readAsDataURL(file)
  })
}

function patchAttachment(key: string, patch: Partial<ComposerAttachment>) {
  attachments.value = attachments.value.map((a) => (a.key === key ? { ...a, ...patch } : a))
}

async function addFiles(files: File[]) {
  const room = MAX_ATTACHMENTS - attachments.value.length
  if (files.length > room) showNote(`A message can carry up to ${MAX_ATTACHMENTS} attachments.`)
  for (const f of files.slice(0, Math.max(0, room))) {
    const key = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const image = IMAGE_TYPES.test(f.type)
    const limit = image ? 10 : 50
    if (f.size > limit * 1024 * 1024) {
      attachments.value = [...attachments.value, { key, kind: image ? 'image' : 'file', name: f.name, path: '', size: f.size, error: `Larger than ${limit} MiB` }]
      continue
    }
    attachments.value = [...attachments.value, { key, kind: image ? 'image' : 'file', name: f.name, path: '', size: f.size, pending: true }]
    try {
      const data = await readData(f)
      const saved = image ? await threads.saveImage(props.threadId, f.name, data) : await threads.saveFile(props.threadId, f.name, { data })
      if (!saved) throw new Error('not saved')
      patchAttachment(key, { kind: saved.kind, name: saved.name, path: saved.path, size: saved.size, thumb: saved.thumb, pending: false })
    } catch (e) {
      patchAttachment(key, { pending: false, error: e instanceof Error ? e.message : String(e) })
    }
  }
  void nextTick(() => box.value?.focus())
}

/** The + menu: files and images, or a folder, through the system picker. */
const plusOpen = ref(false)
async function pickFromDisk(directory: boolean) {
  plusOpen.value = false
  let picked: string | string[] | null = null
  try {
    const dialog = await import('@tauri-apps/plugin-dialog')
    picked = await dialog.open({ multiple: !directory, directory, title: directory ? 'Attach a folder' : 'Attach images and files' })
  } catch {
    showNote('The file picker is not available here.')
    return
  }
  const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked]
  for (const path of paths) {
    const name = path.split(/[\\/]/).pop() || path
    const key = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    attachments.value = [...attachments.value, { key, kind: directory ? 'folder' : 'file', name, path, size: 0, pending: true }]
    try {
      const saved = await threads.saveFile(props.threadId, name, { path })
      if (!saved) throw new Error('not saved')
      patchAttachment(key, { kind: saved.kind, name: saved.name, path: saved.path, size: saved.size, thumb: saved.thumb, pending: false })
    } catch (e) {
      patchAttachment(key, { pending: false, error: e instanceof Error ? e.message : String(e) })
    }
  }
}

function removeAttachment(key: string) {
  attachments.value = attachments.value.filter((a) => a.key !== key)
}

function onDrop(e: DragEvent) {
  threads.fileDrag = false
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (!files.length) return
  e.preventDefault()
  e.stopPropagation()
  void addFiles(files)
}

defineExpose({ focus, addContext })

async function onPaste(e: ClipboardEvent) {
  const images = Array.from(e.clipboardData?.files ?? []).filter((f) => IMAGE_TYPES.test(f.type))
  if (images.length) {
    e.preventDefault()
    void addFiles(images)
    return
  }
  const pastedText = e.clipboardData?.getData('text/plain') ?? ''
  if (!pastedText || keepPasteInline) return
  const bytes = new TextEncoder().encode(pastedText).length
  if (bytes < LARGE_PASTE_BYTES && text.value.length + pastedText.length <= MAX_CHARS) return
  e.preventDefault()
  const path = await threads.savePastedText(props.threadId, pastedText)
  if (path) pasted.value = [...pasted.value, { path, size: bytes }]
}

// ── Files dropped on a sidebar row or anywhere on the window ──
watch(
  () => threads.incoming[props.threadId]?.length ?? 0,
  (n) => {
    if (n) void addFiles(threads.takeIncoming(props.threadId))
  },
  { immediate: true }
)

function removePasted(path: string) {
  pasted.value = pasted.value.filter((p) => p.path !== path)
}

// ── Modifier state for the button's icon ──
function onModifier(e: KeyboardEvent) {
  modHeld.value = e.ctrlKey || e.metaKey
}
function onBlurWindow() {
  modHeld.value = false
}
onMounted(() => {
  window.addEventListener('keydown', onModifier)
  window.addEventListener('keyup', onModifier)
  window.addEventListener('blur', onBlurWindow)
  window.addEventListener('qp-composer-set', onComposerSet)
  window.addEventListener('qp-add-context', onAddContext)
  window.addEventListener('qp-add-attachment', onAddAttachment)
  void nextTick(() => {
    grow()
    focusOnOpen()
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onModifier)
  window.removeEventListener('keyup', onModifier)
  window.removeEventListener('blur', onBlurWindow)
  window.removeEventListener('qp-composer-set', onComposerSet)
  window.removeEventListener('qp-add-context', onAddContext)
  window.removeEventListener('qp-add-attachment', onAddAttachment)
  if (saveTimer) {
    clearTimeout(saveTimer)
    save('pilot.drafts.v1', drafts.value)
    save('pilot.pasted.v1', pastes.value)
  }
})

const primary = computed(() => {
  if (editing.value) return { kind: 'save', title: 'Save the queued message (Enter)', disabled: false }
  if (pendingApproval.value && hasContent.value) return { kind: 'decline', title: 'Decline the request with this reply', disabled: busy.value || tooLong.value }
  if (running.value && !hasContent.value) return { kind: 'stop', title: 'Interrupt the agent', disabled: false }
  if (!running.value && !hasContent.value && view.value?.queueHeld && view.value.queue.length) {
    return { kind: 'resume', title: 'Resume the queue', disabled: false }
  }
  if (view.value?.life.tui) return { kind: 'send', title: 'The conversation is open in the terminal', disabled: true }
  if (!running.value && !hasContent.value && (view.value?.state === 'interrupted' || view.value?.state === 'limited')) {
    return { kind: 'continue', title: 'Resume the interrupted turn', disabled: busy.value }
  }
  if (running.value) {
    const steer = buttonAction.value === 'steer'
    return {
      kind: steer ? 'steer' : 'queue',
      title: steer ? 'Send into the running turn (Ctrl flips to queue)' : 'Queue for after this turn (Ctrl flips to steer)',
      disabled: busy.value || tooLong.value,
    }
  }
  return { kind: 'send', title: 'Send', disabled: !hasContent.value || busy.value || tooLong.value }
})

const placeholder = computed(() => {
  if (editing.value) return 'Edit the queued message — Enter saves, Escape cancels'
  if (pendingApproval.value) return 'Write why to decline the request above, or answer it there'
  if (running.value) return followUp.value === 'steer' && view.value?.canSteer ? 'Steer the running turn…' : 'Queue a follow-up…'
  if (view.value?.queueHeld && view.value.queue.length) return 'The queue is on hold — Resume, or write a new message'
  if (view.value?.life.tui) return 'Open in the terminal — close it there to continue here'
  if (view.value?.state === 'interrupted' || view.value?.state === 'limited') return 'Resume, or write a new message'
  return 'Ask anything, or describe a task'
})

function focus() {
  box.value?.focus()
}

</script>

<template>
  <div class="qp-composer">
    <div v-if="stashOpen && stashes.length" class="qp-stash" role="menu">
      <div class="qp-stash-head">Stashed prompts</div>
      <div v-for="s in stashes" :key="s.id" class="qp-stash-row">
        <button class="qp-stash-pick" role="menuitem" :disabled="hasContent" @click="restore(s.id)">
          <span class="qp-stash-text">{{ s.text || `${s.pasted.length} pasted file(s)` }}</span>
          <span class="qp-stash-sub">{{ s.threadTitle }}</span>
        </button>
        <button class="qp-stash-x" title="Delete this stash" @click="dropStash(s.id)"><X :size="12" /></button>
      </div>
    </div>

    <PilotThreadMentionMenu
      v-if="menu"
      :rows="menuRows"
      :active="menu.active"
      :loading="menu.loading"
      :title="MENU_TITLES[menu.trigger.kind]"
      :empty="menu.trigger.kind === '@' ? 'No file or thread matches' : menu.trigger.kind === '$' ? 'No skills found' : 'No command matches'"
      @hover="(i: number) => menu && (menu.active = i)"
      @pick="pickMenu"
    />
    <div v-if="note" class="qp-c-note" role="status">{{ note }}</div>

    <PilotThreadContextBar :thread-id="threadId" class="qp-c-context" />
    <div
      class="qp-composer-box"
      :class="{ editing: !!editing, over: tooLong, 'file-over': threads.fileDrag }"
      @click="focus"
      @dragover.prevent
      @drop="onDrop"
    >
      <div v-if="editing" class="qp-c-editbar">
        <span>Editing a queued message</span>
        <button class="qp-link" @click.stop="cancelEdit">Cancel</button>
      </div>
      <div v-if="attachments.length" class="qp-c-shelf">
        <div
          v-for="a in attachments"
          :key="a.key"
          class="qp-c-att"
          :class="{ image: a.kind === 'image' && a.thumb, failed: !!a.error }"
          :title="a.error ?? a.path"
        >
          <img v-if="a.kind === 'image' && a.thumb" :src="a.thumb" :alt="a.name" class="qp-c-att-thumb" />
          <template v-else>
            <Loader2 v-if="a.pending" class="qp-c-spin" :size="12" />
            <Folder v-else-if="a.kind === 'folder'" :size="12" />
            <FileText v-else :size="12" />
            <span class="qp-c-att-name">{{ a.name }}</span>
            <span v-if="a.size && !a.error" class="qp-c-att-size">{{ sizeLabel(a.size) }}</span>
            <span v-if="a.error" class="qp-c-att-err">{{ a.error }}</span>
          </template>
          <button class="qp-c-att-x" :title="`Remove ${a.name}`" @click.stop="removeAttachment(a.key)"><X :size="11" /></button>
        </div>
      </div>
      <div v-if="pasted.length" class="qp-c-chips">
        <span v-for="p in pasted" :key="p.path" class="qp-c-chip" :title="p.path">
          <FileText :size="12" />
          Pasted text · {{ sizeLabel(p.size) }}
          <button class="qp-c-chip-x" title="Remove" @click.stop="removePasted(p.path)"><X :size="11" /></button>
        </span>
      </div>
      <div class="qp-c-field">
        <!-- eslint-disable-next-line vue/no-v-html — escaped in highlight() -->
        <div ref="mirror" class="qp-c-mirror" aria-hidden="true" v-html="hl" />
        <textarea
          ref="box"
          :value="text"
          class="qp-c-input"
          rows="1"
          spellcheck="true"
          :placeholder="placeholder"
          @input="onInput"
          @keydown="onKey"
          @keyup="onCaret"
          @click="onCaret"
          @paste="onPaste"
          @blur="menu = null"
          @scroll="syncScroll"
        />
      </div>
      <div class="qp-composer-row">
        <span class="qp-c-plus-anchor">
          <button class="qp-c-plus" title="Attach images, files or a folder — or paste and drop them" @click.stop="plusOpen = !plusOpen">
            <Plus :size="14" />
          </button>
          <div v-if="plusOpen" class="qp-c-plus-menu" role="menu" @click.stop>
            <button class="qp-c-plus-row" role="menuitem" @click="pickFromDisk(false)"><Paperclip :size="13" /> Images and files…</button>
            <button class="qp-c-plus-row" role="menuitem" @click="pickFromDisk(true)"><Folder :size="13" /> Folder…</button>
          </div>
        </span>
        <PilotThreadControls :thread-id="threadId" @done="focus" />
        <span v-if="counter" class="qp-c-count" :class="{ over: tooLong }">{{ counter }}</span>
        <button
          v-if="stashes.length && !editing"
          class="qp-c-stash"
          :title="hasContent ? 'Stash this prompt (Ctrl+S)' : 'Restore a stashed prompt (Ctrl+S)'"
          @click.stop="hasContent ? stash() : (stashOpen = !stashOpen)"
        >
          Stash · {{ stashes.length }}
        </button>
        <button
          class="qp-composer-send"
          :class="`is-${primary.kind}`"
          :disabled="primary.disabled"
          :title="primary.title"
          @click.stop="primary.kind === 'save' ? saveEdit() : clickPrimary()"
        >
          <Square v-if="primary.kind === 'stop'" :size="12" fill="currentColor" />
          <Ban v-else-if="primary.kind === 'decline'" :size="14" />
          <Play v-else-if="primary.kind === 'resume' || primary.kind === 'continue'" :size="13" fill="currentColor" />
          <ListPlus v-else-if="primary.kind === 'queue'" :size="15" />
          <CornerDownRight v-else-if="primary.kind === 'steer'" :size="15" />
          <ArrowUp v-else :size="15" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-composer-box.file-over { border-color: var(--qss-accent, #7aa2f7); box-shadow: 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-c-mirror :deep(.qp-hl-chip) {
  border-radius: 4px;
  background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 20%, transparent);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--qss-accent, #7aa2f7) 45%, transparent);
  color: var(--qss-text, #d4d4d8);
}
.qp-c-note { max-width: 780px; margin: 0 auto 6px; padding: 5px 10px; border-radius: 6px; background: var(--qss-bg-raised, #1f1f25); font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-c-shelf { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 10px 0; }
.qp-c-att {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 240px;
  height: 26px;
  padding: 0 22px 0 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  font-size: 12px;
  color: var(--qss-text, #d4d4d8);
}
.qp-c-att.image { height: 52px; padding: 0; overflow: hidden; }
.qp-c-att.failed { border-color: var(--qss-error, #f87171); }
.qp-c-att-thumb { height: 52px; max-width: 120px; object-fit: cover; display: block; }
.qp-c-att-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-c-att-size { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-c-att-err { font-size: 11px; color: var(--qss-error, #f87171); white-space: nowrap; }
.qp-c-att-x {
  position: absolute;
  top: 3px;
  right: 3px;
  display: inline-flex;
  padding: 2px;
  border: none;
  border-radius: 4px;
  background: color-mix(in srgb, var(--qss-bg, #18181e) 80%, transparent);
  color: var(--qss-text-secondary, #9a9aa5);
  cursor: pointer;
}
.qp-c-att-x:hover { color: var(--qss-text, #d4d4d8); }
.qp-c-spin { animation: qp-c-spin 900ms linear infinite; }
@keyframes qp-c-spin { to { transform: rotate(360deg); } }
.qp-c-plus-anchor { position: relative; display: inline-flex; }
.qp-c-plus {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  cursor: pointer;
}
.qp-c-plus:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-c-plus-menu {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  z-index: 30;
  min-width: 190px;
  padding: 4px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
}
.qp-c-plus-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-c-plus-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-composer { position: relative; flex-shrink: 0; padding: 4px 24px 14px; }
.qp-c-context { max-width: 780px; margin: 0 auto; }
.qp-composer-box {
  max-width: 780px;
  margin: 0 auto;
  padding: 10px 10px 8px 14px;
  border-radius: 14px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  cursor: text;
  transition: border-color 0.12s ease;
}
.qp-composer-box:focus-within { border-color: color-mix(in srgb, var(--qss-text, #d4d4d8) 45%, var(--qss-border, #47474f)); }
.qp-composer-box.editing { border-style: dashed; border-color: var(--qss-accent, #7aa2f7); }
.qp-composer-box.over { border-color: var(--qss-error, #f87171); }
.qp-c-editbar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px; font-size: 11.5px; color: var(--qss-accent, #7aa2f7); }
.qp-c-chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
.qp-c-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 24px;
  padding: 0 4px 0 8px;
  border-radius: 6px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  font-size: 11.5px;
  color: var(--qss-text-secondary, #9a9aa5);
}
.qp-c-chip-x { display: inline-flex; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; padding: 2px; border-radius: 4px; }
.qp-c-chip-x:hover { color: var(--qss-text, #d4d4d8); background: var(--qss-bg-hover, #313139); }

/* The field and its coloured backdrop share every metric. */
.qp-c-field { position: relative; }
.qp-c-mirror,
.qp-c-input {
  margin: 0;
  padding: 0;
  border: none;
  font-family: var(--qss-font-sans, system-ui, sans-serif);
  font-size: 13.5px;
  font-weight: 400;
  line-height: 1.55;
  letter-spacing: normal;
  white-space: pre-wrap;
  overflow-wrap: break-word;
  word-break: normal;
  tab-size: 2;
}
.qp-c-mirror {
  position: absolute;
  inset: 0;
  overflow: hidden;
  color: var(--qss-text, #d4d4d8);
  pointer-events: none;
}
.qp-c-input {
  position: relative;
  display: block;
  width: 100%;
  min-height: 22px;
  max-height: 280px;
  resize: none;
  outline: none;
  background: transparent;
  color: transparent;
  caret-color: var(--qss-text, #d4d4d8);
}
.qp-c-input::selection { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 35%, transparent); color: transparent; }
.qp-c-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
/* The box shows focus (border); the field itself draws no second ring. */
.qp-composer .qp-c-input:focus,
.qp-composer .qp-c-input:focus-visible { outline: none; box-shadow: none; }
.qp-c-mirror :deep(.qp-hl-mark) { color: var(--qss-text-muted, #6e6e7a); }
.qp-c-mirror :deep(.qp-hl-heading) { color: var(--qss-text, #f4f4f5); }
.qp-c-mirror :deep(.qp-hl-strong) { color: #f4f4f5; }
.qp-c-mirror :deep(.qp-hl-em) { color: var(--qss-text-secondary, #b4b4bd); }
.qp-c-mirror :deep(.qp-hl-quote) { color: var(--qss-text-secondary, #9a9aa5); }
.qp-c-mirror :deep(.qp-hl-code),
.qp-c-mirror :deep(.qp-hl-fence) { color: #e0af68; }
.qp-c-mirror :deep(.qp-hl-link) { color: var(--qss-accent, #7aa2f7); }
.qp-c-mirror :deep(.qp-hl-token) { color: var(--qss-accent, #7aa2f7); }

.qp-composer-row { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
.qp-c-count { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-c-count.over { color: var(--qss-error, #f87171); }
.qp-c-stash {
  border: 1px solid var(--qss-border, #47474f);
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 11px;
  height: 22px;
  padding: 0 8px;
  border-radius: 6px;
  cursor: pointer;
}
.qp-c-stash:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-composer-send {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: none;
  background: var(--qss-text, #d4d4d8);
  color: var(--qss-bg, #18181e);
  cursor: pointer;
}
.qp-composer-send:disabled { opacity: 0.3; cursor: default; }
.qp-composer-send:active:not(:disabled) { transform: scale(0.94); }
.qp-composer-send.is-stop { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); border: 1px solid var(--qss-border, #47474f); }
.qp-composer-send.is-decline { background: var(--qss-error, #f87171); color: var(--qss-bg, #18181e); }
.qp-composer-send.is-queue,
.qp-composer-send.is-steer { background: var(--qss-accent, #7aa2f7); color: var(--qss-bg, #18181e); }

.qp-stash {
  position: absolute;
  bottom: calc(100% - 6px);
  right: max(24px, calc((100% - 780px) / 2));
  z-index: 10;
  width: 320px;
  max-height: 280px;
  overflow-y: auto;
  padding: 4px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.4);
}
.qp-stash-head { padding: 6px 8px 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-stash-row { display: flex; align-items: center; gap: 2px; }
.qp-stash-pick {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  text-align: left;
  cursor: pointer;
}
.qp-stash-pick:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-stash-pick:disabled { opacity: 0.5; cursor: default; }
.qp-stash-text { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-stash-sub { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-stash-x { display: inline-flex; padding: 6px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-stash-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
