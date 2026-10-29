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
 * - Drafts, pasted files and stashes live in this device's storage.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ArrowUp, CornerDownRight, FileText, ListPlus, Play, Square, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { caretLine } from '#pilot/utils/caret'
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
const pastes = useState<Record<string, Pasted[]>>('pilot-pastes', () => load('pilot.pasted.v1', {}))
const stashes = useState<Stash[]>('pilot-stash', () => load('pilot.stash.v1', []))
let saveTimer: ReturnType<typeof setTimeout> | null = null
function persistSoon() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    save('pilot.drafts.v1', drafts.value)
    save('pilot.pasted.v1', pastes.value)
  }, 300)
}

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
watch(
  () => props.threadId,
  () => {
    recall.value = null
    void nextTick(() => {
      grow()
      box.value?.focus()
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

const hasContent = computed(() => text.value.trim().length > 0 || pasted.value.length > 0)

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
  if (!hasContent.value || tooLong.value) return
  const message = compose()
  busy.value = true
  let ok = false
  if (action === 'queue') ok = await threads.queueAdd(id, message)
  else if (action === 'steer') ok = await threads.steer(id, message)
  else ok = await threads.send(id, message)
  busy.value = false
  if (!ok) return
  text.value = ''
  pasted.value = []
  recall.value = null
  if (action === 'sendNewThread' && thread.value) {
    await store.createThread({ contextId: thread.value.contextId, mode: thread.value.mode })
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
  if (e.key === 'Enter') {
    if (editing.value && !e.shiftKey) {
      e.preventDefault()
      void saveEdit()
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
}

async function onPaste(e: ClipboardEvent) {
  const pastedText = e.clipboardData?.getData('text/plain') ?? ''
  if (!pastedText || keepPasteInline) return
  const bytes = new TextEncoder().encode(pastedText).length
  if (bytes < LARGE_PASTE_BYTES && text.value.length + pastedText.length <= MAX_CHARS) return
  e.preventDefault()
  const path = await threads.savePastedText(props.threadId, pastedText)
  if (path) pasted.value = [...pasted.value, { path, size: bytes }]
}

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
  void nextTick(() => {
    grow()
    box.value?.focus()
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onModifier)
  window.removeEventListener('keyup', onModifier)
  window.removeEventListener('blur', onBlurWindow)
  if (saveTimer) {
    clearTimeout(saveTimer)
    save('pilot.drafts.v1', drafts.value)
    save('pilot.pasted.v1', pastes.value)
  }
})

const primary = computed(() => {
  if (editing.value) return { kind: 'save', title: 'Save the queued message (Enter)', disabled: false }
  if (running.value && !hasContent.value) return { kind: 'stop', title: 'Interrupt the agent', disabled: false }
  if (!running.value && !hasContent.value && view.value?.queueHeld && view.value.queue.length) {
    return { kind: 'resume', title: 'Resume the queue', disabled: false }
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
  if (running.value) return followUp.value === 'steer' && view.value?.canSteer ? 'Steer the running turn…' : 'Queue a follow-up…'
  if (view.value?.queueHeld && view.value.queue.length) return 'The queue is on hold — Resume, or write a new message'
  return 'Ask anything, or describe a task'
})

function focus() {
  box.value?.focus()
}
defineExpose({ focus })
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

    <div class="qp-composer-box" :class="{ editing: !!editing, over: tooLong }" @click="focus">
      <div v-if="editing" class="qp-c-editbar">
        <span>Editing a queued message</span>
        <button class="qp-link" @click.stop="cancelEdit">Cancel</button>
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
          @paste="onPaste"
          @scroll="syncScroll"
        />
      </div>
      <div class="qp-composer-row">
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
          <Play v-else-if="primary.kind === 'resume'" :size="13" fill="currentColor" />
          <ListPlus v-else-if="primary.kind === 'queue'" :size="15" />
          <CornerDownRight v-else-if="primary.kind === 'steer'" :size="15" />
          <ArrowUp v-else :size="15" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-composer { position: relative; flex-shrink: 0; padding: 4px 24px 14px; }
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
