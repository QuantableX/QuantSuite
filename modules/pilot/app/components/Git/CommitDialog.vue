<script setup lang="ts">
/**
 * Commit (card 12; T3 Code's commit dialog): the branch, the changed files
 * — untick what stays out — and the message. An empty message is written by
 * the thread's own provider (Generate runs as the dialog opens; typing
 * first wins). Commit, or Commit & push when there is a remote; Ctrl+Enter
 * takes the primary one. AI credit lines never reach the commit.
 */
import { computed, nextTick, onMounted, ref } from 'vue'
import { GitBranch, LoaderCircle, Sparkles, X } from 'lucide-vue-next'
import { useGitStore } from '#pilot/stores/git'
import type { GitStatus } from '#pilot/types'

const props = defineProps<{ threadId: string; status: GitStatus }>()
const emit = defineEmits<{ close: [] }>()
const git = useGitStore()

const box = ref<HTMLElement | null>(null)
const input = ref<HTMLTextAreaElement | null>(null)
const excluded = ref(new Set<string>())
const message = ref('')
const generated = ref('')
const generating = ref(false)
const error = ref('')

const files = computed(() => props.status.files)
const picked = computed(() => files.value.filter((f) => !excluded.value.has(f.path)))
const allPicked = computed(() => excluded.value.size === 0)
const totals = computed(() => picked.value.reduce((t, f) => ({ a: t.a + f.added, r: t.r + f.removed }), { a: 0, r: 0 }))
const subject = computed(() => message.value.split('\n')[0] ?? '')
const canPush = computed(() => Boolean(props.status.remote && props.status.branch))
const onDefault = computed(() => !props.status.worktree && props.status.branch !== '' && props.status.branch === props.status.defaultBranch)
const selection = () => (allPicked.value ? null : picked.value.map((f) => f.path))

function toggle(path: string) {
  const next = new Set(excluded.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  excluded.value = next
}
function toggleAll() {
  excluded.value = allPicked.value ? new Set(files.value.map((f) => f.path)) : new Set()
}
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) }
}

async function generate(): Promise<boolean> {
  if (generating.value || !picked.value.length) return false
  generating.value = true
  error.value = ''
  const r = await git.generate(props.threadId, 'commit', { files: selection() ?? undefined })
  generating.value = false
  if (typeof r === 'string') {
    error.value = r
    return false
  }
  const text = r.body ? `${r.title}\n\n${r.body}` : r.title
  // What the user typed meanwhile stays.
  if (!message.value.trim() || message.value === generated.value) {
    message.value = text
    generated.value = text
  }
  return true
}

async function commit(push: boolean) {
  if (!picked.value.length || git.busy[props.threadId]) return
  if (!message.value.trim() && !(await generate())) return
  const text = message.value
  const sel = selection()
  emit('close')
  await git.commit(props.threadId, text, sel, push)
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    void commit(canPush.value)
  }
}

onMounted(() => {
  void nextTick(() => input.value?.focus())
  void generate()
})
</script>

<template>
  <Teleport to="body">
    <div class="qp-gd-veil" @mousedown.self="emit('close')">
      <div ref="box" class="qp-gd" role="dialog" aria-label="Commit changes" @keydown="onKey">
        <div class="qp-gd-head">
          <span class="qp-gd-title">Commit changes</span>
          <button class="qp-gd-x" aria-label="Close" @click="emit('close')"><X :size="14" /></button>
        </div>
        <div class="qp-gd-branch">
          <GitBranch :size="13" />
          <span class="qp-gd-branch-name">{{ status.branch || 'detached HEAD' }}</span>
          <span v-if="onDefault" class="qp-gd-warn">the default branch</span>
        </div>

        <div class="qp-gd-files-head">
          <label class="qp-gd-check">
            <input type="checkbox" :checked="allPicked" :indeterminate.prop="!allPicked && picked.length > 0" @change="toggleAll" />
            <span>Files</span>
          </label>
          <span class="qp-gd-count">{{ picked.length }} of {{ files.length }}</span>
          <span class="qp-gd-num"><span class="add">+{{ totals.a }}</span> <span class="del">−{{ totals.r }}</span></span>
        </div>
        <div class="qp-gd-files" role="list">
          <label v-for="f in files" :key="f.path" class="qp-gd-file" role="listitem" :class="{ out: excluded.has(f.path) }" :title="f.path">
            <input type="checkbox" :checked="!excluded.has(f.path)" @change="toggle(f.path)" />
            <span class="qp-gd-st" :data-st="f.status">{{ f.status }}</span>
            <span class="qp-gd-dir">{{ split(f.path).dir }}</span><span class="qp-gd-name">{{ split(f.path).name }}</span>
            <span class="qp-gd-num">
              <template v-if="excluded.has(f.path)">left out</template>
              <template v-else><span class="add">+{{ f.added }}</span> <span class="del">−{{ f.removed }}</span></template>
            </span>
          </label>
        </div>

        <div class="qp-gd-msg-head">
          <span>Message</span>
          <span v-if="subject.length" class="qp-gd-len" :class="{ long: subject.length > 72 }" title="Subject length">{{ subject.length }}/72</span>
          <button class="qp-gd-gen" :disabled="generating || !picked.length" title="Write the message from the selected changes" @click="generate">
            <LoaderCircle v-if="generating" class="qp-gd-spin" :size="12" />
            <Sparkles v-else :size="12" />
            {{ generating ? 'Writing…' : 'Generate' }}
          </button>
        </div>
        <textarea
          ref="input"
          v-model="message"
          class="qp-gd-input"
          rows="7"
          spellcheck="true"
          :placeholder="generating ? 'Writing a message from the changes…' : 'Leave empty to generate'"
        />
        <div v-if="error" class="qp-gd-error">{{ error }}</div>

        <div class="qp-gd-foot">
          <span class="qp-gd-hint">Ctrl+Enter {{ canPush ? 'commits and pushes' : 'commits' }}</span>
          <button class="qp-gd-btn" @click="emit('close')">Cancel</button>
          <button class="qp-gd-btn" :class="{ primary: !canPush }" :disabled="!picked.length" @click="commit(false)">Commit</button>
          <button v-if="canPush" class="qp-gd-btn primary" :disabled="!picked.length" @click="commit(true)">Commit &amp; push</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-gd-veil { position: fixed; inset: 0; z-index: 1100; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.45); }
.qp-gd {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: min(620px, calc(100% - 32px));
  max-height: calc(100% - 48px);
  padding: 14px 16px;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  animation: qp-gd-in 0.17s ease-out;
}
@keyframes qp-gd-in {
  from { opacity: 0; transform: translateY(4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-gd { animation: none; }
}
.qp-gd-head { display: flex; align-items: center; }
.qp-gd-title { font-size: 14px; font-weight: 600; }
.qp-gd-x { margin-left: auto; display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-gd-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-gd-branch { display: flex; align-items: center; gap: 6px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-gd-branch-name { color: var(--qss-text, #d4d4d8); font-weight: 600; }
.qp-gd-warn { color: var(--qss-warning, #d29a3f); }
.qp-gd-files-head,
.qp-gd-msg-head { display: flex; align-items: center; gap: 8px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-gd-check { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
.qp-gd-count { color: var(--qss-text-muted, #6e6e7a); }
.qp-gd-files-head .qp-gd-num { margin-left: auto; }
.qp-gd-files {
  min-height: 60px;
  max-height: 220px;
  overflow-y: auto;
  padding: 4px;
  border-radius: 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg, #18181e);
}
.qp-gd-file { display: flex; align-items: center; gap: 6px; min-width: 0; padding: 3px 6px; border-radius: 5px; cursor: pointer; }
.qp-gd-file:hover { background: var(--qss-bg-hover, #313139); }
.qp-gd-file.out .qp-gd-name,
.qp-gd-file.out .qp-gd-dir { color: var(--qss-text-muted, #6e6e7a); text-decoration: line-through; }
.qp-gd-st { width: 14px; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; text-align: center; }
.qp-gd-st[data-st='A'] { color: var(--qss-success, #34d399); }
.qp-gd-st[data-st='D'] { color: var(--qss-error, #f87171); }
.qp-gd-st[data-st='M'] { color: var(--qss-warning, #d29a3f); }
.qp-gd-dir { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-gd-name { flex-shrink: 0; white-space: nowrap; }
.qp-gd-num { margin-left: auto; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.add { color: var(--qss-success, #34d399); }
.del { color: var(--qss-error, #f87171); }
.qp-gd-len { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-gd-len.long { color: var(--qss-warning, #d29a3f); }
.qp-gd-gen {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-gd-gen:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-gd-gen:disabled { opacity: 0.6; cursor: default; }
.qp-gd-spin { animation: qp-gd-rot 0.9s linear infinite; }
@keyframes qp-gd-rot { to { transform: rotate(360deg); } }
.qp-gd-input {
  resize: vertical;
  min-height: 110px;
  padding: 8px 10px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 13px;
  line-height: 1.5;
  outline: none;
}
.qp-gd-input:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-gd-error { color: var(--qss-error, #f87171); white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-gd-foot { display: flex; align-items: center; gap: 6px; }
.qp-gd-hint { margin-right: auto; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-gd-btn {
  padding: 4px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-gd-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-gd-btn.primary { background: var(--qss-accent, #7aa2f7); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-gd-btn:disabled { opacity: 0.5; cursor: default; }
</style>
