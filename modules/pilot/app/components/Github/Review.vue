<script setup lang="ts">
/**
 * A human review of a pull request (parity 11) — separate from the agent's
 * "Review" thread. The PR at its current head: description, commits, files
 * (viewed marks, per head), each file's diff with the existing discussions at
 * their lines; click a line number (Shift for a range) to draft a comment.
 * Drafts are saved on this computer only and survive restarts; after a push
 * a draft follows its line when it is found once, otherwise it is marked
 * stale and never posted at a guessed line. Submit posts one review — Comment,
 * Approve or Request changes as GitHub allows you — exactly once; it never merges.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { ArrowLeft, Check, CircleDot, ExternalLink, MessageSquare, RefreshCw } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { openExternal } from '#pilot/utils/external'
import { parsePatch, type PatchLine } from '#pilot/utils/diff'

interface Draft {
  id: string
  path: string
  side: 'RIGHT' | 'LEFT'
  line: number | null
  startLine: number | null
  body: string
  commit: string
  lineText: string
  createdAt: number
  anchor: 'ok' | 'moved' | 'stale' | ''
}
interface FileDiff {
  path: string
  previous: string | null
  status: string
  additions: number
  deletions: number
  patch: string | null
}
interface Discussion {
  id: string
  path: string
  line: number | null
  side: string
  resolved: boolean
  outdated: boolean
  canResolve: boolean
  comments: { id: string; author: string; body: string; createdAt: string; mine: boolean }[]
}
interface Loaded {
  number: number
  title: string
  body: string
  url: string
  author: string
  state: string
  draft: boolean
  base: string
  head: string
  headSha: string
  commits: [string, string][]
  files: FileDiff[]
  truncatedFiles: boolean
  discussions: Discussion[]
  checks: { passed: number; failed: number; pending: number }
  viewer: string
  actions: string[]
  drafts: Draft[]
}

const props = defineProps<{ contextId: string; number: number }>()
const emit = defineEmits<{ close: [] }>()
const pr = ref<Loaded | null>(null)
const error = ref('')
const notice = ref('')
const loading = ref(false)
const selected = ref<string | null>(null)
const editing = ref<{ line: number; side: 'RIGHT' | 'LEFT'; start: number | null; text: string; body: string } | null>(null)
const rangeStart = ref<{ line: number; side: 'RIGHT' | 'LEFT' } | null>(null)
const summary = ref('')
const event = ref('comment')
const busy = ref(false)

const viewedKey = computed(() => `qp-review-viewed:${props.contextId}:${props.number}:${pr.value?.headSha ?? ''}`)
const viewed = ref<Set<string>>(new Set())
function loadViewed() {
  try {
    viewed.value = new Set(JSON.parse(localStorage.getItem(viewedKey.value) ?? '[]'))
  } catch {
    viewed.value = new Set()
  }
}
function toggleViewed(path: string) {
  const next = new Set(viewed.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  viewed.value = next
  localStorage.setItem(viewedKey.value, JSON.stringify([...next]))
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    pr.value = await pilot<Loaded>('plugin:pilot|pilot_review_load', { contextId: props.contextId, number: props.number })
    summary.value = pr.value?.drafts.find((d) => !d.path)?.body ?? ''
    if (!selected.value || !pr.value?.files.some((f) => f.path === selected.value)) selected.value = pr.value?.files[0]?.path ?? null
    if (pr.value && !pr.value.actions.includes(event.value)) event.value = 'comment'
    loadViewed()
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}
onMounted(() => void load())

const file = computed(() => pr.value?.files.find((f) => f.path === selected.value) ?? null)
const lines = computed<PatchLine[]>(() => (file.value?.patch ? parsePatch(file.value.patch) : []))
const drafts = computed(() => (pr.value?.drafts ?? []).filter((d) => d.path === selected.value))
const stale = computed(() => (pr.value?.drafts ?? []).filter((d) => d.path && d.anchor === 'stale'))
const inline = computed(() => (pr.value?.drafts ?? []).filter((d) => d.path))
const threadsHere = computed(() => (pr.value?.discussions ?? []).filter((t) => t.path === selected.value))
const sideOf = (l: PatchLine): 'RIGHT' | 'LEFT' => (l.kind === 'del' ? 'LEFT' : 'RIGHT')
const numOf = (l: PatchLine) => (l.kind === 'del' ? l.old : l.new)
const at =
  (l: PatchLine) =>
  <T extends { line: number | null; side: string }>(items: T[]): T[] =>
    items.filter((x) => x.line === numOf(l) && x.side === sideOf(l))

function startComment(l: PatchLine, e: MouseEvent) {
  const n = numOf(l)
  if (n == null || l.kind === 'hunk' || l.kind === 'meta') return
  const side = sideOf(l)
  if (e.shiftKey && rangeStart.value && rangeStart.value.side === side && rangeStart.value.line < n) {
    editing.value = { line: n, side, start: rangeStart.value.line, text: l.text, body: '' }
  } else {
    editing.value = { line: n, side, start: null, text: l.text, body: '' }
    rangeStart.value = { line: n, side }
  }
}

async function saveDraft(d: Partial<Draft>) {
  if (!pr.value) return
  try {
    await pilot('plugin:pilot|pilot_review_draft_save', {
      contextId: props.contextId,
      number: props.number,
      draft: { id: '', path: '', side: 'RIGHT', line: null, startLine: null, body: '', commit: pr.value.headSha, lineText: '', createdAt: 0, anchor: '', ...d },
    })
    editing.value = null
    await load()
  } catch (e) {
    error.value = String(e)
  }
}
function saveEditing() {
  const e = editing.value
  if (!e || !e.body.trim() || !selected.value) return
  void saveDraft({ path: selected.value, side: e.side, line: e.line, startLine: e.start, body: e.body, lineText: e.text })
}
async function removeDraft(id: string) {
  await pilot('plugin:pilot|pilot_review_draft_delete', { contextId: props.contextId, number: props.number, id }).catch((e) => (error.value = String(e)))
  await load()
}
let summaryTimer: ReturnType<typeof setTimeout> | undefined
watch(summary, (text) => {
  clearTimeout(summaryTimer)
  summaryTimer = setTimeout(() => {
    const existing = pr.value?.drafts.find((d) => !d.path)
    if (!pr.value || (existing?.body ?? '') === text) return
    void pilot('plugin:pilot|pilot_review_draft_save', {
      contextId: props.contextId,
      number: props.number,
      draft: { ...(existing ?? { id: '', path: '', side: 'RIGHT', line: null, startLine: null, commit: pr.value.headSha, lineText: '', createdAt: 0, anchor: '' }), body: text },
    }).then((d) => {
      if (pr.value && d) pr.value.drafts = [...pr.value.drafts.filter((x) => x.path || x.id !== (d as Draft).id), d as Draft]
    })
  }, 500)
})

async function submit() {
  busy.value = true
  error.value = ''
  try {
    const url = await pilot<string>('plugin:pilot|pilot_review_submit', { contextId: props.contextId, number: props.number, event: event.value })
    notice.value = `Review submitted${url ? ` — ${url}` : ''}.`
    summary.value = ''
    await load()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function resolve(t: Discussion) {
  await pilot('plugin:pilot|pilot_review_resolve', { threadId: t.id, resolved: !t.resolved }).catch((e) => (error.value = String(e)))
  await load()
}

const STATUS: Record<string, string> = { added: 'A', modified: 'M', removed: 'D', renamed: 'R', copied: 'C', changed: 'M' }
const canSubmit = computed(() => !busy.value && pr.value && (event.value === 'approve' || summary.value.trim() || inline.value.length) && !stale.value.length)
</script>

<template>
  <section class="qp-rv">
    <header class="qp-rv-bar">
      <button class="qp-rv-icon" aria-label="Back" @click="emit('close')"><ArrowLeft :size="14" /></button>
      <span class="qp-rv-title">#{{ number }} {{ pr?.title }}</span>
      <span v-if="pr" class="qp-rv-dim">{{ pr.author }} · {{ pr.head }} → {{ pr.base }} · {{ pr.headSha.slice(0, 7) }}</span>
      <span v-if="pr" class="qp-rv-checks"><Check :size="11" /> {{ pr.checks.passed }} <CircleDot :size="11" /> {{ pr.checks.pending }} ✕ {{ pr.checks.failed }}</span>
      <div class="qp-rv-spacer" />
      <button class="qp-rv-icon" title="Reload from GitHub" :disabled="loading" @click="load"><RefreshCw :size="13" /></button>
      <button v-if="pr" class="qp-rv-icon" title="Open on GitHub" @click="openExternal(pr.url)"><ExternalLink :size="13" /></button>
    </header>
    <div v-if="error" class="qp-rv-err" role="alert">{{ error }}</div>
    <div v-if="notice" class="qp-rv-ok" role="status">{{ notice }}</div>
    <div v-if="loading && !pr" class="qp-rv-dim qp-rv-pad">Loading the pull request…</div>
    <div v-if="pr" class="qp-rv-body">
      <aside class="qp-rv-files">
        <div class="qp-rv-sec">Files ({{ pr.files.length }}{{ pr.truncatedFiles ? '+' : '' }})</div>
        <button v-for="f in pr.files" :key="f.path" class="qp-rv-file" :class="{ on: f.path === selected, seen: viewed.has(f.path) }" :title="f.previous ? `${f.previous} → ${f.path}` : f.path" @click="selected = f.path">
          <input type="checkbox" :checked="viewed.has(f.path)" title="Viewed" @click.stop="toggleViewed(f.path)" />
          <span class="qp-rv-st" :data-st="STATUS[f.status] ?? 'M'">{{ STATUS[f.status] ?? 'M' }}</span>
          <span class="qp-rv-fname">{{ f.path }}</span>
          <span class="qp-rv-num"><span class="add">+{{ f.additions }}</span> <span class="del">−{{ f.deletions }}</span></span>
        </button>
        <div class="qp-rv-sec">Commits</div>
        <div v-for="c in pr.commits" :key="c[0]" class="qp-rv-commit"><code>{{ c[0] }}</code> {{ c[1] }}</div>
      </aside>
      <main class="qp-rv-diff">
        <div v-if="!file" class="qp-rv-dim qp-rv-pad">No files.</div>
        <template v-else>
          <div class="qp-rv-filehead">{{ file.previous ? `${file.previous} → ` : '' }}{{ file.path }}</div>
          <div v-if="!file.patch" class="qp-rv-dim qp-rv-pad">GitHub shows no diff for this file (binary, very large or only renamed).</div>
          <div v-for="(l, i) in lines" :key="i">
            <div class="qp-rv-line" :data-kind="l.kind">
              <button class="qp-rv-ln" :disabled="l.kind === 'hunk' || l.kind === 'meta'" title="Comment on this line (Shift: a range from the last one)" @click="startComment(l, $event)">{{ l.old ?? '' }}</button>
              <button class="qp-rv-ln" :disabled="l.kind === 'hunk' || l.kind === 'meta'" title="Comment on this line (Shift: a range from the last one)" @click="startComment(l, $event)">{{ l.new ?? '' }}</button>
              <pre class="qp-rv-code">{{ l.text }}</pre>
            </div>
            <div v-for="t in at(l)(threadsHere)" :key="t.id" class="qp-rv-thread" :class="{ resolved: t.resolved }">
              <div class="qp-rv-thread-head">
                <MessageSquare :size="11" /> {{ t.resolved ? 'Resolved' : 'Open' }}<template v-if="t.outdated"> · outdated</template>
                <button v-if="t.canResolve" class="qp-link" @click="resolve(t)">{{ t.resolved ? 'Reopen' : 'Resolve' }}</button>
              </div>
              <div v-for="c in t.comments" :key="c.id" class="qp-rv-comment"><b>{{ c.author }}</b> {{ c.body }}</div>
            </div>
            <div v-for="d in at(l)(drafts)" :key="d.id" class="qp-rv-draft" :data-anchor="d.anchor">
              <span class="qp-rv-tag">{{ d.anchor === 'stale' ? 'Stale draft — its line changed in a push; it will not be posted here' : 'Draft' }}{{ d.startLine ? ` · lines ${d.startLine}–${d.line}` : '' }}{{ d.anchor === 'moved' ? ' · moved with its line after a push' : '' }}</span>
              <div class="qp-rv-draft-body">{{ d.body }}</div>
              <button class="qp-link" @click="removeDraft(d.id)">Delete</button>
            </div>
            <div v-if="editing && editing.line === numOf(l) && editing.side === sideOf(l)" class="qp-rv-edit">
              <div class="qp-rv-dim">{{ editing.start ? `Lines ${editing.start}–${editing.line}` : `Line ${editing.line}` }} ({{ editing.side === 'LEFT' ? 'old' : 'new' }} side)</div>
              <textarea v-model="editing.body" class="qp-rv-text" rows="3" placeholder="Comment — saved as a draft, posted only when you submit" @keydown.ctrl.enter="saveEditing" />
              <div class="qp-rv-actions">
                <button class="qp-btn qp-btn-sm" @click="editing = null">Cancel</button>
                <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!editing.body.trim()" @click="saveEditing">Save draft</button>
              </div>
            </div>
          </div>
        </template>
      </main>
      <aside class="qp-rv-side">
        <div class="qp-rv-sec">Description</div>
        <div class="qp-rv-desc">{{ pr.body || 'No description.' }}</div>
        <div class="qp-rv-sec">Your review</div>
        <div v-if="stale.length" class="qp-rv-err">
          {{ stale.length }} draft(s) point at lines that changed in a push:
          <div v-for="d in stale" :key="d.id" class="qp-rv-stale">
            <span>{{ d.path }}:{{ d.line }} — {{ d.body.slice(0, 80) }}</span>
            <button class="qp-link" @click="removeDraft(d.id)">Delete</button>
          </div>
        </div>
        <div class="qp-rv-dim">{{ inline.length }} draft comment(s) on this computer — nothing is posted before Submit.</div>
        <textarea v-model="summary" class="qp-rv-text" rows="5" placeholder="Summary" />
        <div class="qp-rv-events" role="radiogroup">
          <label><input v-model="event" type="radio" value="comment" /> Comment</label>
          <label :class="{ off: !pr.actions.includes('approve') }"><input v-model="event" type="radio" value="approve" :disabled="!pr.actions.includes('approve')" /> Approve</label>
          <label :class="{ off: !pr.actions.includes('request_changes') }"><input v-model="event" type="radio" value="request_changes" :disabled="!pr.actions.includes('request_changes')" /> Request changes</label>
        </div>
        <div v-if="!pr.actions.includes('approve')" class="qp-rv-dim">GitHub allows only comments here (your own pull request, or not open).</div>
        <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="!canSubmit" @click="submit">{{ busy ? 'Submitting…' : 'Submit review' }}</button>
      </aside>
    </div>
  </section>
</template>

<style scoped>
.qp-rv { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-rv-bar { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-rv-title { font-weight: 600; font-size: 13px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-rv-icon { border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; display: inline-flex; }
.qp-rv-spacer { flex: 1; }
.qp-rv-checks { display: inline-flex; align-items: center; gap: 3px; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-rv-dim { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-rv-pad { padding: 20px; }
.qp-rv-err { color: var(--qss-error, #e06c75); font-size: 12px; padding: 4px 12px; }
.qp-rv-ok { color: var(--qss-success, #34d399); font-size: 12px; padding: 4px 12px; }
.qp-rv-body { flex: 1; min-height: 0; display: grid; grid-template-columns: 240px 1fr 280px; }
.qp-rv-files, .qp-rv-side { min-height: 0; overflow: auto; padding: 8px; display: flex; flex-direction: column; gap: 4px; border-right: 1px solid var(--qss-border-subtle, #35353d); }
.qp-rv-side { border-right: none; border-left: 1px solid var(--qss-border-subtle, #35353d); gap: 8px; }
.qp-rv-sec { margin-top: 6px; font-size: 11.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-rv-file { display: flex; align-items: center; gap: 5px; padding: 3px 4px; border: none; border-radius: 6px; background: transparent; color: inherit; font: inherit; font-size: 12px; text-align: left; cursor: pointer; }
.qp-rv-file.on { background: var(--qss-bg-hover, #313139); }
.qp-rv-file.seen .qp-rv-fname { color: var(--qss-text-muted, #6e6e7a); }
.qp-rv-fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
.qp-rv-st { width: 12px; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-rv-st[data-st='A'] { color: var(--qss-success, #34d399); }
.qp-rv-st[data-st='D'] { color: var(--qss-error, #f87171); }
.qp-rv-st[data-st='M'] { color: var(--qss-warning, #d29a3f); }
.qp-rv-st[data-st='R'] { color: var(--qss-accent, #7aa2f7); }
.qp-rv-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; }
.add { color: var(--qss-success, #34d399); }
.del { color: var(--qss-error, #f87171); }
.qp-rv-commit { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-rv-diff { min-height: 0; overflow: auto; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-rv-filehead { position: sticky; top: 0; padding: 6px 10px; background: var(--qss-bg-raised, #1f1f25); border-bottom: 1px solid var(--qss-border-subtle, #35353d); font-family: var(--qss-font-sans, system-ui); font-weight: 500; z-index: 1; }
.qp-rv-line { display: grid; grid-template-columns: 44px 44px 1fr; }
.qp-rv-line[data-kind='add'] { background: color-mix(in srgb, var(--qss-success, #34d399) 10%, transparent); }
.qp-rv-line[data-kind='del'] { background: color-mix(in srgb, var(--qss-error, #f87171) 10%, transparent); }
.qp-rv-line[data-kind='hunk'] { color: var(--qss-text-muted, #6e6e7a); background: var(--qss-bg-raised, #1f1f25); }
.qp-rv-ln { border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); font: inherit; font-size: 11px; text-align: right; padding: 0 6px; cursor: pointer; }
.qp-rv-ln:hover:not(:disabled) { color: var(--qss-accent, #7aa2f7); }
.qp-rv-code { margin: 0; white-space: pre-wrap; word-break: break-word; }
.qp-rv-thread, .qp-rv-draft, .qp-rv-edit { margin: 4px 10px 4px 88px; padding: 6px 8px; border-radius: 8px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); font-family: var(--qss-font-sans, system-ui); font-size: 12px; display: flex; flex-direction: column; gap: 4px; }
.qp-rv-thread.resolved { opacity: 0.7; }
.qp-rv-thread-head { display: flex; align-items: center; gap: 5px; color: var(--qss-text-secondary, #9a9aa5); font-size: 11.5px; }
.qp-rv-draft { border-color: color-mix(in srgb, var(--qss-accent, #7aa2f7) 60%, transparent); }
.qp-rv-draft[data-anchor='stale'] { border-color: color-mix(in srgb, var(--qss-error, #e06c75) 60%, transparent); opacity: 0.8; }
.qp-rv-draft[data-anchor='stale'] .qp-rv-tag { color: var(--qss-error, #e06c75); }
.qp-rv-draft[data-anchor='moved'] { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 60%, transparent); }
.qp-rv-tag { font-size: 11px; color: var(--qss-accent, #7aa2f7); }
.qp-rv-draft-body, .qp-rv-comment, .qp-rv-desc { white-space: pre-wrap; word-break: break-word; }
.qp-rv-text { width: 100%; box-sizing: border-box; padding: 6px 8px; border-radius: 7px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg, #18181e); color: inherit; font: inherit; font-family: var(--qss-font-sans, system-ui); resize: vertical; }
.qp-rv-actions { display: flex; justify-content: flex-end; gap: 6px; }
.qp-rv-events { display: flex; flex-direction: column; gap: 3px; }
.qp-rv-events .off { opacity: 0.5; }
.qp-rv-stale { display: flex; gap: 6px; align-items: baseline; }
.qp-rv-desc { max-height: 160px; overflow: auto; font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
</style>
