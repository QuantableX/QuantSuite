<script setup lang="ts">
/**
 * What the agent waits on, right above the message box (docs/PLAN-QUANTPILOT.md,
 * card 6; PandaCode's approval bar, T3 Code's question panel).
 *
 * - Approvals: what is asked (command with its input, file edit, MCP tool,
 *   web fetch) and who asks (the agent or a subagent). Allow, Allow for this
 *   session, Deny; with several, Approve all. A focused request: Enter
 *   allows, Escape moves to Deny. Typing a reply in the box and sending it
 *   declines the first request with that text (Composer.vue).
 * - Questions: one at a time with its options (single or multi select),
 *   an "Other" answer, Back / Next / Submit, Dismiss. Digits pick options.
 *   A secret question is a masked field and its answer is never saved; a
 *   question without free text offers only its options. A question with
 *   free text also takes images and files (parity 17): picked, pasted or
 *   dropped, saved like the composer's, sent as references in the answer.
 * - MCP forms and web-page steps (Elicitation.vue, parity card 05).
 * - What is picked or typed is a draft per request (utils/requestDrafts):
 *   it survives streaming, switching threads and a refused submit, and with
 *   several requests waiting each keeps its own.
 * - A plan waiting for review is a card in the conversation; here it is a
 *   one-line pointer to it.
 */
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { Bot, ChevronRight, FileText, Globe, ImageIcon, ListChecks, Loader2, Paperclip, Pencil, Plug, SquareTerminal, Wrench, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import type { AnswerFile, ComposerAttachment, ThreadItem } from '#pilot/types'
import { pruneRequestDrafts, requestDraft, type RequestDraft } from '#pilot/utils/requestDrafts'
import { DOCUMENT, cancelDocument, docLabel, readDocument } from '#pilot/utils/documents'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ done: [] }>()
const threads = useThreadStore()
const store = usePilotStore()

const pending = computed(() => threads.pending(props.threadId))
const approvals = computed(() => pending.value.filter((i) => i.kind === 'approval'))
const elicitations = computed(() => pending.value.filter((i) => i.kind === 'question' && i.data.elicitation))
const asked = computed(() => pending.value.filter((i) => i.kind === 'question' && !i.data.elicitation))
/** Several questions waiting: one at a time, the user may switch between them. */
const questionIndex = ref(0)
const question = computed(() => asked.value[Math.min(questionIndex.value, asked.value.length - 1)] ?? null)
watch(
  () => pending.value.map((i) => String(i.data.requestId)),
  (ids) => pruneRequestDrafts(props.threadId, ids)
)
/** A Codex proposal waits on the newest turn while nothing runs — PlanCard's rule (card 24). */
const proposal = computed(() => {
  const v = threads.view(props.threadId)
  if (!v || v.turnId) return null
  const last = v.items.at(-1)?.turnId
  return [...v.items].reverse().find((i) => i.kind === 'plan' && i.data.status === 'proposed' && i.turnId === last) ?? null
})
const plan = computed(() => pending.value.find((i) => i.kind === 'plan') ?? proposal.value)
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const agent = computed(() => store.adapterLabel(thread.value?.provider || 'claude'))

const busy = ref<string | null>(null)
const expanded = reactive<Record<string, boolean>>({})
const rows = ref<HTMLElement[]>([])

const ICONS: Record<string, unknown> = { command: SquareTerminal, read: FileText, search: FileText, edit: Pencil, web: Globe, agent: Bot, todo: ListChecks, mcp: Plug }
function verb(category: string): string {
  switch (category) {
    case 'command':
      return 'run'
    case 'edit':
      return 'edit'
    case 'read':
    case 'search':
      return 'read'
    case 'web':
      return 'fetch'
    default:
      return 'use'
  }
}

async function answer(item: ThreadItem, a: Parameters<typeof threads.answer>[2]) {
  busy.value = item.id
  await threads.answer(props.threadId, String(item.data.requestId), a)
  busy.value = null
}

async function approveAll() {
  for (const a of [...approvals.value]) await answer(a, { decision: 'allow' })
  emit('done')
}

function onRowKey(e: KeyboardEvent, item: ThreadItem, i: number) {
  if (e.target !== e.currentTarget) return
  if (e.key === 'Enter') {
    e.preventDefault()
    void answer(item, { decision: 'allow' })
  } else if (e.key === 'Escape') {
    e.preventDefault()
    rows.value[i]?.querySelector<HTMLButtonElement>('.qp-ap-deny')?.focus()
  }
}

/** A new request with an empty box: take focus so Enter answers it. */
watch(
  () => approvals.value.length,
  (n, old) => {
    if (n > (old ?? 0)) {
      const active = document.activeElement as HTMLTextAreaElement | null
      const boxIsEmpty = active?.classList.contains('qp-c-input') && !active.value
      if (boxIsEmpty || active === document.body) void nextTick(() => rows.value[0]?.focus())
    }
  }
)

// ── Questions ──
interface Q {
  question: string
  header?: string
  options?: { label: string; description?: string }[]
  multiSelect?: boolean
  /** Masked input; the answer is never saved (Codex `isSecret`). */
  isSecret?: boolean
  /** `false`: only the offered options (Codex `isOther`). */
  isOther?: boolean
}
const questions = computed<Q[]>(() => question.value?.data.questions ?? [])
const draft = computed(() => (question.value ? requestDraft(props.threadId, String(question.value.data.requestId)) : null))
const picked = computed<Record<string, string[]>>(() => draft.value?.picked ?? {})
const other = computed<Record<string, string>>(() => draft.value?.other ?? {})
const step = computed({
  get: () => draft.value?.step ?? 0,
  set: (v: number) => {
    if (draft.value) draft.value.step = v
  },
})
const current = computed(() => questions.value[step.value] ?? null)
const freeText = (q: Q) => q.isOther !== false || !q.options?.length

function toggle(q: Q, label: string) {
  const cur = picked.value[q.question] ?? []
  picked.value[q.question] = q.multiSelect ? (cur.includes(label) ? cur.filter((l) => l !== label) : [...cur, label]) : [label]
  if (!q.multiSelect) other.value[q.question] = ''
}
function answered(q: Q): boolean {
  return (picked.value[q.question]?.length ?? 0) > 0 || Boolean(other.value[q.question]?.trim()) || ready(q).length > 0
}
const allAnswered = computed(() => questions.value.length > 0 && questions.value.every(answered))

// ── Files with an answer (parity 17) ──
const QFILES = 10
const canAttach = (q: Q) => freeText(q) && !q.isSecret
const filesOf = (q: Q): ComposerAttachment[] => draft.value?.files[q.question] ?? []
const ready = (q: Q) => filesOf(q).filter((f) => !f.pending && !f.error && f.path)
/** Still saving or failed: the answer waits until that is sorted out. */
const fileProblem = computed(() => {
  const all = questions.value.flatMap((q) => filesOf(q))
  if (all.some((f) => f.pending)) return 'Wait until the files are saved.'
  if (all.some((f) => f.error)) return 'Remove the files that could not be saved.'
  return ''
})
function readData(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result))
    r.onerror = () => reject(r.error)
    r.readAsDataURL(file)
  })
}
/** Patch one file of one question of one request, wherever the panel is now. */
function patchFile(d: RequestDraft, question: string, key: string, patch: Partial<ComposerAttachment>) {
  d.files[question] = (d.files[question] ?? []).map((f) => (f.key === key ? { ...f, ...patch } : f))
}
async function saveInto(d: RequestDraft, question: string, entry: ComposerAttachment, save: () => Promise<{ kind: string; name: string; path: string; size: number; thumb?: string | null } | null>) {
  d.files[question] = [...(d.files[question] ?? []), entry]
  try {
    const saved = await save()
    if (!saved) throw new Error('not saved')
    patchFile(d, question, entry.key, { kind: saved.kind as ComposerAttachment['kind'], name: saved.name, path: saved.path, size: saved.size, thumb: saved.thumb ?? null, pending: false })
    if (saved.kind === 'file' && DOCUMENT.test(saved.name)) {
      patchFile(d, question, entry.key, { docPending: true })
      const doc = await readDocument(saved.path, saved.name)
      patchFile(d, question, entry.key, { docPending: false, doc })
    }
  } catch (e) {
    patchFile(d, question, entry.key, { pending: false, error: e instanceof Error ? e.message : String(e) })
  }
}
const newKey = () => `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
function room(q: Q, n: number): number {
  const left = QFILES - filesOf(q).length
  if (n > left) fileNote.value = `An answer can carry up to ${QFILES} files.`
  return Math.max(0, left)
}
const fileNote = ref('')
async function addFiles(q: Q, files: File[]) {
  const d = draft.value
  if (!d || !canAttach(q)) return
  fileNote.value = ''
  for (const f of files.slice(0, room(q, files.length))) {
    const image = /^image\/(png|jpeg|webp)$/.test(f.type)
    const limit = image ? 10 : 50
    const entry: ComposerAttachment = { key: newKey(), kind: image ? 'image' : 'file', name: f.name || 'pasted.png', path: '', size: f.size, pending: true }
    if (f.size > limit * 1024 * 1024) {
      d.files[q.question] = [...(d.files[q.question] ?? []), { ...entry, pending: false, error: `Larger than ${limit} MiB` }]
      continue
    }
    void saveInto(d, q.question, entry, async () => {
      const data = await readData(f)
      return image ? threads.saveImage(props.threadId, entry.name, data) : threads.saveFile(props.threadId, entry.name, { data })
    })
  }
}
async function pickFiles(q: Q) {
  const d = draft.value
  if (!d || !canAttach(q)) return
  let picked: string | string[] | null = null
  try {
    const dialog = await import('@tauri-apps/plugin-dialog')
    picked = await dialog.open({ multiple: true, directory: false, title: 'Attach images and files to your answer' })
  } catch {
    fileNote.value = 'The file picker is not available here.'
    return
  }
  const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked]
  fileNote.value = ''
  for (const path of paths.slice(0, room(q, paths.length))) {
    const name = path.split(/[\\/]/).pop() || path
    void saveInto(d, q.question, { key: newKey(), kind: 'file', name, path: '', size: 0, pending: true }, () => threads.saveFile(props.threadId, name, { path }))
  }
}
function removeFile(q: Q, key: string) {
  const d = draft.value
  if (!d) return
  const gone = (d.files[q.question] ?? []).find((f) => f.key === key)
  if (gone?.docPending && gone.path) cancelDocument(gone.path)
  d.files[q.question] = (d.files[q.question] ?? []).filter((f) => f.key !== key)
}
function onPaste(e: ClipboardEvent) {
  const q = current.value
  const files = Array.from(e.clipboardData?.files ?? [])
  if (!q || !files.length || !canAttach(q)) return
  e.preventDefault()
  void addFiles(q, files)
}
function onDrop(e: DragEvent) {
  threads.fileDrag = false
  const q = current.value
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (!q || !files.length) return
  e.preventDefault()
  e.stopPropagation()
  if (!canAttach(q)) {
    fileNote.value = q.isSecret ? 'A secret answer cannot carry files.' : 'This question takes only its options.'
    return
  }
  void addFiles(q, files)
}
const sizeLabel = (n: number) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`)

async function submitQuestion() {
  if (!question.value || !allAnswered.value || fileProblem.value) return
  const answers: Record<string, string> = {}
  const files: Record<string, AnswerFile[]> = {}
  for (const q of questions.value) {
    const parts = [...(picked.value[q.question] ?? [])]
    // A secret is sent exactly as typed.
    const own = q.isSecret ? other.value[q.question] : other.value[q.question]?.trim()
    if (own) parts.push(own)
    answers[q.question] = parts.join(', ')
    const sent = canAttach(q) ? ready(q) : []
    if (sent.length) files[q.question] = sent.map((f) => ({ kind: f.kind === 'image' ? 'image' : 'file', name: f.name, path: f.path, thumb: f.thumb && f.thumb.length < 96 * 1024 ? f.thumb : null }))
  }
  const item = question.value
  busy.value = item.id
  const err = await threads.answer(props.threadId, String(item.data.requestId), { decision: 'answers', answers, files })
  busy.value = null
  if (!err) emit('done')
}

function onQuestionKey(e: KeyboardEvent) {
  const q = current.value
  if (!q || (e.target as HTMLElement).tagName === 'INPUT') return
  if (/^[1-9]$/.test(e.key) && q.options?.[Number(e.key) - 1]) {
    e.preventDefault()
    toggle(q, q.options[Number(e.key) - 1].label)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    if (step.value < questions.value.length - 1) step.value++
    else submitQuestion()
  }
}

function showPlan() {
  if (!plan.value) return
  document.querySelector(`[data-item-id="${plan.value.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}
</script>

<template>
  <div v-if="pending.length || proposal" class="qp-ap" role="region" aria-label="Waiting for you">
    <div
      v-for="(a, i) in approvals"
      :key="a.id"
      ref="rows"
      class="qp-ap-row"
      tabindex="0"
      :aria-label="`${agent} wants to ${verb(a.data.category)} ${a.data.title}. Enter allows, Escape moves to Deny.`"
      @keydown="onRowKey($event, a, i)"
    >
      <div class="qp-ap-line">
        <component :is="ICONS[a.data.category] ?? Wrench" :size="14" class="qp-ap-icon" />
        <span class="qp-ap-who">{{ a.parent ? 'A subagent' : agent }} wants to {{ verb(a.data.category) }}</span>
        <code class="qp-ap-title" :title="a.data.title">{{ a.data.title || a.data.toolName }}</code>
        <button v-if="a.data.input && a.data.input !== a.data.title" class="qp-ap-more" :aria-expanded="!!expanded[a.id]" @click="expanded[a.id] = !expanded[a.id]">
          <ChevronRight :size="12" :class="{ open: expanded[a.id] }" /> Details
        </button>
      </div>
      <pre v-if="expanded[a.id]" class="qp-ap-pre">{{ a.data.input }}</pre>
      <div class="qp-ap-actions">
        <button class="qp-btn qp-btn-primary qp-btn-sm" :disabled="busy === a.id" @click="answer(a, { decision: 'allow' })">Allow</button>
        <button
          v-if="a.data.allowSession !== false"
          class="qp-btn qp-btn-sm"
          :disabled="busy === a.id"
          @click="answer(a, { decision: 'allowSession' })"
        >
          Allow for this session
        </button>
        <button class="qp-btn qp-btn-sm qp-ap-deny" :disabled="busy === a.id" @click="answer(a, { decision: 'deny' })">Deny</button>
        <span v-if="i === 0" class="qp-ap-hint">or write why in the box below to decline with feedback</span>
      </div>
    </div>
    <div v-if="approvals.length > 1" class="qp-ap-all">
      <button class="qp-btn qp-btn-sm" @click="approveAll">Approve all {{ approvals.length }}</button>
    </div>

    <PilotThreadElicitation
      v-for="(el, n) in elicitations"
      :key="el.id"
      :thread-id="threadId"
      :item="el"
      :position="elicitations.length > 1 ? `${n + 1} of ${elicitations.length}` : undefined"
      @done="emit('done')"
    />

    <div v-if="question && current" :key="question.id" class="qp-ap-q" tabindex="0" @keydown="onQuestionKey" @paste="onPaste" @dragover.prevent @drop="onDrop">
      <div class="qp-ap-q-head">
        <span class="qp-ap-q-tag">{{ current.header || 'Question' }}</span>
        <span v-if="questions.length > 1" class="qp-ap-q-step">{{ step + 1 }} of {{ questions.length }}</span>
        <span v-if="asked.length > 1" class="qp-ap-q-many">
          <button class="qp-link" :disabled="questionIndex === 0" @click="questionIndex--">Previous request</button>
          <span class="qp-ap-q-step">request {{ asked.indexOf(question) + 1 }} of {{ asked.length }}</span>
          <button class="qp-link" :disabled="questionIndex >= asked.length - 1" @click="questionIndex++">Next request</button>
        </span>
      </div>
      <div v-if="question.data.byMessage" class="qp-ap-q-note">The agent asked this for later — your answer goes out as your next message.</div>
      <div class="qp-ap-q-text">{{ current.question }}</div>
      <div class="qp-ap-q-options">
        <button
          v-for="(o, n) in current.options ?? []"
          :key="o.label"
          class="qp-ap-opt"
          :class="{ on: (picked[current.question] ?? []).includes(o.label) }"
          :aria-pressed="(picked[current.question] ?? []).includes(o.label)"
          @click="toggle(current, o.label)"
        >
          <span class="qp-ap-opt-n">{{ n + 1 }}</span>
          <span class="qp-ap-opt-text">
            <span>{{ o.label }}</span>
            <span v-if="o.description" class="qp-ap-opt-desc">{{ o.description }}</span>
          </span>
        </button>
      </div>
      <input
        v-if="freeText(current)"
        v-model="other[current.question]"
        class="qp-ap-other"
        :type="current.isSecret ? 'password' : 'text'"
        :autocomplete="current.isSecret ? 'off' : undefined"
        :placeholder="
          current.isSecret
            ? 'Your answer — hidden, sent to the agent only, never saved'
            : !current.options?.length
              ? 'Your answer'
              : current.multiSelect
                ? 'Something else (added to your picks)'
                : 'Other — your own answer'
        "
        @input="!current.multiSelect && other[current.question] && (picked[current.question] = [])"
        @keydown.enter.prevent="step < questions.length - 1 ? step++ : submitQuestion()"
      />
      <div v-if="canAttach(current) && filesOf(current).length" class="qp-ap-files">
        <div v-for="f in filesOf(current)" :key="f.key" class="qp-ap-file" :class="{ failed: !!f.error }" :title="f.error ?? f.path">
          <Loader2 v-if="f.pending" class="qp-ap-spin" :size="12" />
          <img v-else-if="f.kind === 'image' && f.thumb" :src="f.thumb" :alt="f.name" class="qp-ap-thumb" />
          <ImageIcon v-else-if="f.kind === 'image'" :size="12" />
          <FileText v-else :size="12" />
          <span class="qp-ap-file-name">{{ f.name }}</span>
          <span v-if="f.size && !f.error" class="qp-ap-file-size">{{ sizeLabel(f.size) }}</span>
          <span v-if="f.error" class="qp-ap-file-err">{{ f.error }}</span>
          <span v-else-if="f.docPending" class="qp-ap-file-doc">Reading text…</span>
          <span v-else-if="f.doc" class="qp-ap-file-doc" :class="{ warn: f.doc.status !== 'ok' }" :title="f.doc.detail ?? ''">{{ docLabel(f.doc) }}</span>
          <button class="qp-ap-file-x" :title="`Remove ${f.name}`" @click.stop="removeFile(current, f.key)"><X :size="11" /></button>
        </div>
      </div>
      <div v-if="fileNote || (fileProblem && step === questions.length - 1)" class="qp-ap-q-note" role="status">{{ fileNote || fileProblem }}</div>
      <div class="qp-ap-actions">
        <button v-if="canAttach(current)" class="qp-btn qp-btn-sm" title="Attach images or files to this answer — or paste or drop them here" @click="pickFiles(current)"><Paperclip :size="12" /> Attach</button>
        <button v-if="step > 0" class="qp-btn qp-btn-sm" @click="step--">Back</button>
        <button v-if="step < questions.length - 1" class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!answered(current)" @click="step++">Next</button>
        <button v-else class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!allAnswered || !!fileProblem || busy === question.id" @click="submitQuestion">Answer</button>
        <button class="qp-btn qp-btn-sm" @click="answer(question, { decision: 'dismiss' })">Dismiss</button>
      </div>
    </div>

    <button v-if="plan" class="qp-ap-plan" @click="showPlan">A plan is ready for your review — show it</button>
  </div>
</template>

<style scoped>
.qp-ap { max-width: 780px; width: calc(100% - 48px); margin: 0 auto 6px; display: flex; flex-direction: column; gap: 6px; }
.qp-ap-row,
.qp-ap-q {
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 6%, var(--qss-bg-raised, #1f1f25));
  display: flex;
  flex-direction: column;
  gap: 7px;
  outline: none;
}
.qp-ap-row:focus-visible,
.qp-ap-q:focus-visible { box-shadow: 0 0 0 1px var(--qss-warning, #d29a3f); }
.qp-ap-line { display: flex; align-items: center; gap: 7px; min-width: 0; font-size: 12.5px; }
.qp-ap-icon { flex-shrink: 0; color: var(--qss-warning, #d29a3f); }
.qp-ap-who { flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ap-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  color: var(--qss-text, #d4d4d8);
}
.qp-ap-more { flex-shrink: 0; margin-left: auto; display: inline-flex; align-items: center; gap: 3px; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; cursor: pointer; }
.qp-ap-more:hover { color: var(--qss-text, #d4d4d8); }
.qp-ap-more .open { transform: rotate(90deg); }
.qp-ap-pre {
  margin: 0;
  max-height: 180px;
  overflow: auto;
  padding: 7px 9px;
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.qp-ap-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-ap-hint { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-all { display: flex; justify-content: flex-end; }
.qp-ap-q-head { display: flex; align-items: center; justify-content: space-between; }
.qp-ap-q-tag { font-size: 11.5px; font-weight: 600; color: var(--qss-warning, #d29a3f); }
.qp-ap-q-note { margin-top: 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-q-many { display: inline-flex; align-items: center; gap: 8px; margin-left: auto; padding-left: 10px; font-size: 11.5px; }
.qp-ap-q-step { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-q-text { font-size: 13px; color: var(--qss-text, #d4d4d8); }
.qp-ap-q-options { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 6px; }
.qp-ap-opt {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 7px 9px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-ap-opt:hover { background: var(--qss-bg-hover, #313139); }
.qp-ap-opt.on { border-color: var(--qss-accent, #7aa2f7); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 14%, var(--qss-bg, #18181e)); }
.qp-ap-opt-n { flex-shrink: 0; width: 16px; height: 16px; border-radius: 4px; display: inline-flex; align-items: center; justify-content: center; background: var(--qss-bg-card, #292930); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-opt-text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.qp-ap-opt-desc { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-other {
  height: 28px;
  padding: 0 9px;
  border-radius: 7px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
}
.qp-ap-other:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-ap-plan {
  align-self: flex-start;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: transparent;
  color: var(--qss-warning, #d29a3f);
  font-size: 12px;
  padding: 4px 10px;
  border-radius: 14px;
  cursor: pointer;
}
.qp-ap-plan:hover { background: color-mix(in srgb, var(--qss-warning, #d29a3f) 10%, transparent); }
/* Files with an answer (parity 17). */
.qp-ap-files { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px; }
.qp-ap-file { display: inline-flex; align-items: center; gap: 5px; max-width: 340px; height: 26px; padding: 0 4px 0 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; font-size: 12px; }
.qp-ap-file.failed { border-color: var(--qss-error, #f87171); }
.qp-ap-thumb { height: 18px; width: 18px; object-fit: cover; border-radius: 3px; }
.qp-ap-file-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ap-file-size { flex-shrink: 0; white-space: nowrap; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ap-file-err { font-size: 11px; color: var(--qss-error, #f87171); white-space: nowrap; }
.qp-ap-file-doc { flex-shrink: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); white-space: nowrap; }
.qp-ap-file-doc.warn { color: var(--qss-warning, #e0af68); }
.qp-ap-file-x { display: inline-flex; padding: 2px; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; border-radius: 4px; }
.qp-ap-file-x:hover { color: var(--qss-text, #d4d4d8); }
.qp-ap-spin { animation: qp-ap-spin 1s linear infinite; }
@keyframes qp-ap-spin { to { transform: rotate(360deg); } }
</style>
