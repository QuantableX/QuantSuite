<script setup lang="ts">
/**
 * The Diff tab (card 11; T3 Code's diff panel): This turn (a turn's own
 * changes, between its checkpoints), Uncommitted (working tree against
 * HEAD) or Branch vs base. Files on top, Monaco's diff below — split or
 * unified, wrapped or not; Ctrl+F finds inside it. Per file: copy the path,
 * stage / unstage / discard (Uncommitted; discard asks first), and a review
 * comment on the selected lines that lands in the composer as a chip.
 */
import { computed, ref, watch } from 'vue'
import { Columns2, Copy, MessageSquarePlus, RefreshCw, Rows2, WrapText } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore, type DiffScope } from '#pilot/stores/inspector'
import type { DiffList, DiffSides, FileChange } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const inspector = useInspectorStore()

const scope = ref<DiffScope>(inspector.target.scope)
const turn = ref<string | null>(inspector.target.turn)
const list = ref<DiffList | null>(null)
const selected = ref<string | null>(inspector.target.path)
const sides = ref<DiffSides | null>(null)
const loading = ref(false)
const error = ref('')
const note = ref('')
const confirmDiscard = ref(false)
const commenting = ref<{ start: number; end: number; code: string } | null>(null)
const commentText = ref('')
const editor = ref<{ selection: () => { startLine: number; endLine: number; text: string } | null } | null>(null)

const SCOPES: { id: DiffScope; label: string }[] = [
  { id: 'turn', label: 'This turn' },
  { id: 'uncommitted', label: 'Uncommitted' },
  { id: 'branch', label: 'Branch vs base' },
]

async function load() {
  loading.value = true
  error.value = ''
  try {
    list.value = await pilot<DiffList>('plugin:pilot|pilot_diff_list', { id: props.threadId, scope: scope.value, turn: scope.value === 'turn' ? turn.value : null })
    const files = list.value?.files ?? []
    if (!selected.value || !files.some((f) => f.path === selected.value)) selected.value = files[0]?.path ?? null
    if (scope.value === 'turn' && !turn.value && list.value?.turns.length) turn.value = list.value.turns[0][0]
  } catch (e) {
    list.value = null
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

// The editor stays mounted from file to file: the old sides show until the new ones arrive.
async function loadSides() {
  const l = list.value
  if (!l || !selected.value) {
    sides.value = null
    return
  }
  try {
    sides.value = await pilot<DiffSides>('plugin:pilot|pilot_diff_file', { id: props.threadId, from: l.from, to: l.to, path: selected.value })
  } catch (e) {
    sides.value = null
    error.value = String(e)
  }
}

watch(() => [props.threadId, scope.value, turn.value], () => void load(), { immediate: true })
watch(() => [list.value, selected.value], () => void loadSides())
// The changes card or a key opened a target: follow it.
watch(
  () => inspector.target.nonce,
  () => {
    scope.value = inspector.target.scope
    turn.value = inspector.target.turn
    selected.value = inspector.target.path
    void load()
  }
)

const LANG: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', js: 'javascript', mjs: 'javascript', jsx: 'javascript', vue: 'html', html: 'html',
  css: 'css', scss: 'scss', json: 'json', md: 'markdown', rs: 'rust', py: 'python', toml: 'ini', yml: 'yaml', yaml: 'yaml',
  sh: 'shell', ps1: 'powershell', sql: 'sql', go: 'go', java: 'java', cs: 'csharp', cpp: 'cpp', c: 'c', h: 'cpp', xml: 'xml',
}
const language = computed(() => LANG[(selected.value ?? '').split('.').pop()?.toLowerCase() ?? ''] ?? 'plaintext')
const current = computed<FileChange | null>(() => list.value?.files.find((f) => f.path === selected.value) ?? null)
const totals = computed(() => (list.value?.files ?? []).reduce((t, f) => ({ a: t.a + f.added, r: t.r + f.removed }), { a: 0, r: 0 }))
const staged = computed(() => new Set(list.value?.staged ?? []))
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) }
}

function say(text: string) {
  note.value = text
  setTimeout(() => note.value === text && (note.value = ''), 4000)
}

async function act(action: 'stage' | 'unstage' | 'discard') {
  if (!selected.value) return
  confirmDiscard.value = false
  try {
    await pilot('plugin:pilot|pilot_diff_action', { id: props.threadId, path: selected.value, action })
    say(action === 'discard' ? `Discarded ${selected.value}` : action === 'stage' ? 'Staged' : 'Unstaged')
    await load()
  } catch (e) {
    error.value = String(e)
  }
}

async function copyPath() {
  if (!selected.value) return
  await navigator.clipboard.writeText(selected.value).catch(() => undefined)
  say('Path copied')
}

function startComment() {
  const sel = editor.value?.selection()
  if (!sel || !sel.text.trim()) {
    say('Select lines on the right side first.')
    return
  }
  commenting.value = { start: sel.startLine, end: sel.endLine, code: sel.text }
  commentText.value = ''
}

function addComment() {
  const c = commenting.value
  if (!c || !selected.value || !commentText.value.trim()) return
  const lines = c.start === c.end ? `${c.start}` : `${c.start}–${c.end}`
  const chip = {
    kind: 'comment',
    label: `${split(selected.value).name}:${lines}`,
    content: `Review comment: ${commentText.value.trim()}\n\n${selected.value} lines ${lines}:\n${c.code}`,
    path: selected.value,
  }
  window.dispatchEvent(new CustomEvent('qp-add-context', { detail: { threadId: props.threadId, chip } }))
  commenting.value = null
  say('Added to your message')
}
</script>

<template>
  <div class="qp-diff">
    <div class="qp-diff-bar">
      <span class="qp-diff-scopes" role="radiogroup" aria-label="What to compare">
        <button v-for="s in SCOPES" :key="s.id" class="qp-diff-scope" role="radio" :aria-checked="scope === s.id" :class="{ on: scope === s.id }" @click="scope = s.id">
          {{ s.label }}
        </button>
      </span>
      <select v-if="scope === 'turn' && list?.turns.length" v-model="turn" class="qp-diff-turn" aria-label="Turn">
        <option v-for="t in list.turns" :key="t[0]" :value="t[0]">{{ t[1] }}</option>
      </select>
      <span class="qp-diff-tools">
        <button class="qp-diff-icon" :title="inspector.sideBySide ? 'Unified view' : 'Split view'" @click="inspector.setView({ sideBySide: !inspector.sideBySide })">
          <component :is="inspector.sideBySide ? Rows2 : Columns2" :size="13" />
        </button>
        <button class="qp-diff-icon" :class="{ on: inspector.wrap }" title="Wrap lines" @click="inspector.setView({ wrap: !inspector.wrap })"><WrapText :size="13" /></button>
        <button class="qp-diff-icon" title="Refresh" @click="load"><RefreshCw :size="13" /></button>
      </span>
    </div>

    <div v-if="error" class="qp-diff-error">{{ error }}</div>
    <div v-else-if="!loading && list && !list.files.length" class="qp-diff-empty">
      {{ scope === 'turn' ? 'No turn has changed files yet.' : scope === 'uncommitted' ? 'Nothing uncommitted.' : 'This branch has no changes against its base.' }}
    </div>

    <template v-if="list?.files.length">
      <div class="qp-diff-files" role="listbox" aria-label="Changed files">
        <div class="qp-diff-sum">{{ list.files.length }} file{{ list.files.length === 1 ? '' : 's' }} · <span class="add">+{{ totals.a }}</span> <span class="del">−{{ totals.r }}</span></div>
        <button
          v-for="f in list.files"
          :key="f.path"
          class="qp-diff-file"
          role="option"
          :aria-selected="f.path === selected"
          :class="{ on: f.path === selected }"
          :title="f.path"
          @click="selected = f.path"
        >
          <span class="qp-diff-st" :data-st="f.status">{{ f.status }}</span>
          <span class="qp-diff-dir">{{ split(f.path).dir }}</span><span class="qp-diff-name">{{ split(f.path).name }}</span>
          <span v-if="staged.has(f.path)" class="qp-diff-staged" title="Staged">●</span>
          <span class="qp-diff-num"><span class="add">+{{ f.added }}</span> <span class="del">−{{ f.removed }}</span></span>
        </button>
      </div>

      <div v-if="current" class="qp-diff-actions">
        <button class="qp-diff-btn" @click="copyPath"><Copy :size="12" /> Copy path</button>
        <template v-if="scope === 'uncommitted'">
          <button class="qp-diff-btn" @click="act(staged.has(current.path) ? 'unstage' : 'stage')">{{ staged.has(current.path) ? 'Unstage' : 'Stage' }}</button>
          <button v-if="!confirmDiscard" class="qp-diff-btn danger" @click="confirmDiscard = true">Discard…</button>
          <span v-else class="qp-diff-confirm">
            Discard the changes to {{ split(current.path).name }}?
            <button class="qp-diff-btn danger" @click="act('discard')">Discard</button>
            <button class="qp-diff-btn" @click="confirmDiscard = false">Keep</button>
          </span>
        </template>
        <button class="qp-diff-btn" title="Comment on the selected lines — it goes into your message" @click="startComment"><MessageSquarePlus :size="12" /> Comment</button>
        <span v-if="note" class="qp-diff-note">{{ note }}</span>
      </div>
      <div v-if="commenting" class="qp-diff-comment">
        <div class="qp-diff-comment-head">Lines {{ commenting.start }}{{ commenting.end !== commenting.start ? `–${commenting.end}` : '' }} of {{ selected }}</div>
        <textarea v-model="commentText" class="qp-diff-comment-input" rows="3" placeholder="What should change here?" @keydown.enter.ctrl.prevent="addComment" />
        <div class="qp-diff-comment-actions">
          <button class="qp-diff-btn" @click="commenting = null">Cancel</button>
          <button class="qp-diff-btn primary" :disabled="!commentText.trim()" @click="addComment">Add to message</button>
        </div>
      </div>

      <div class="qp-diff-editor">
        <div v-if="current?.binary" class="qp-diff-empty">A binary file — no text diff.</div>
        <QDiffEditor
          v-else-if="sides"
          ref="editor"
          :original="sides.original"
          :modified="sides.modified"
          :language="language"
          :side-by-side="inspector.sideBySide"
          :word-wrap="inspector.wrap"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.qp-diff { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: 12px; color: var(--qss-text, #d4d4d8); }
.qp-diff-bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); flex-wrap: wrap; }
.qp-diff-scopes { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; overflow: hidden; }
.qp-diff-scope { padding: 2px 9px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; cursor: pointer; }
.qp-diff-scope.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-diff-turn { flex: 1 1 100px; min-width: 100px; max-width: 260px; text-overflow: ellipsis; padding: 2px 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font: inherit; }
.qp-diff-tools { margin-left: auto; display: inline-flex; gap: 2px; }
.qp-diff-icon { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-diff-icon:hover,
.qp-diff-icon.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-diff-error { margin: 8px; padding: 6px 8px; border-radius: 6px; background: color-mix(in srgb, var(--qss-warning, #d29a3f) 14%, transparent); color: var(--qss-warning, #d29a3f); }
.qp-diff-empty { padding: 16px; color: var(--qss-text-muted, #6e6e7a); text-align: center; }
.qp-diff-files { max-height: 32%; overflow-y: auto; padding: 4px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-diff-sum { padding: 2px 6px 4px; color: var(--qss-text-muted, #6e6e7a); }
.qp-diff-file {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  padding: 3px 6px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-diff-file.on { background: var(--qss-bg-card, #292930); }
.qp-diff-file:hover:not(.on) { background: var(--qss-bg-hover, #313139); }
.qp-diff-st { width: 14px; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; text-align: center; }
.qp-diff-st[data-st='A'] { color: var(--qss-success, #34d399); }
.qp-diff-st[data-st='D'] { color: var(--qss-error, #f87171); }
.qp-diff-st[data-st='M'] { color: var(--qss-warning, #d29a3f); }
.qp-diff-dir { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-diff-name { flex-shrink: 0; white-space: nowrap; }
.qp-diff-staged { color: var(--qss-success, #34d399); font-size: 9px; }
.qp-diff-num { margin-left: auto; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.add { color: var(--qss-success, #34d399); }
.del { color: var(--qss-error, #f87171); }
.qp-diff-actions { display: flex; align-items: center; gap: 4px; padding: 6px 8px; flex-wrap: wrap; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-diff-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-diff-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-diff-btn.danger { color: var(--qss-error, #f87171); }
.qp-diff-btn.primary { background: var(--qss-accent, #7aa2f7); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-diff-btn:disabled { opacity: 0.5; cursor: default; }
.qp-diff-confirm { display: inline-flex; align-items: center; gap: 4px; color: var(--qss-warning, #d29a3f); }
.qp-diff-note { color: var(--qss-text-muted, #6e6e7a); }
.qp-diff-comment { display: flex; flex-direction: column; gap: 6px; padding: 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-diff-comment-head { color: var(--qss-text-muted, #6e6e7a); }
.qp-diff-comment-input {
  resize: vertical;
  padding: 6px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  outline: none;
}
.qp-diff-comment-actions { display: flex; justify-content: flex-end; gap: 6px; }
.qp-diff-editor { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-diff-editor :deep(.q-diff-editor) { flex: 1; min-height: 0; }
</style>
