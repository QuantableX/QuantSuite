<script setup lang="ts">
/**
 * Create a pull request (card 12): base branch, title and description —
 * written from the commits, the diff and the thread as the dialog opens,
 * editable — and Draft. Creating pushes the branch first when it needs
 * it, opens the pull request with `gh` and links it to the thread.
 */
import { computed, nextTick, onMounted, ref } from 'vue'
import { GitPullRequest, LoaderCircle, Sparkles, X } from 'lucide-vue-next'
import { useGitStore } from '#pilot/stores/git'
import type { GitStatus } from '#pilot/types'

const props = defineProps<{ threadId: string; status: GitStatus }>()
const emit = defineEmits<{ close: [] }>()
const git = useGitStore()

const titleInput = ref<HTMLInputElement | null>(null)
const base = ref(props.status.base)
const bases = ref<string[]>([props.status.base])
const title = ref('')
const body = ref('')
const draft = ref(false)
const generating = ref(false)
const edited = ref(false)
const error = ref('')

const pushes = computed(() => !props.status.upstream || props.status.ahead > 0)

async function generate() {
  if (generating.value) return
  generating.value = true
  error.value = ''
  const r = await git.generate(props.threadId, 'pr', { base: base.value })
  generating.value = false
  if (typeof r === 'string') {
    error.value = r
    return
  }
  // A title the user wrote stays theirs.
  if (!edited.value || !title.value.trim()) {
    title.value = r.title
    body.value = r.body
    edited.value = false
  }
}

async function create() {
  if (!title.value.trim() || git.busy[props.threadId]) return
  const req = { base: base.value, title: title.value.trim(), body: body.value, draft: draft.value }
  emit('close')
  await git.createPr(props.threadId, req)
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    void create()
  }
}

onMounted(async () => {
  void nextTick(() => titleInput.value?.focus())
  void generate()
  const names = (await git.branches(props.threadId)).map((b) => b.name).filter((n) => n !== props.status.branch)
  bases.value = [...new Set([props.status.base, ...names])]
})
</script>

<template>
  <Teleport to="body">
    <div class="qp-gd-veil" @mousedown.self="emit('close')">
      <div class="qp-gd" role="dialog" aria-label="Create pull request" @keydown="onKey">
        <div class="qp-gd-head">
          <GitPullRequest :size="15" />
          <span class="qp-gd-title">Create pull request</span>
          <button class="qp-gd-x" aria-label="Close" @click="emit('close')"><X :size="14" /></button>
        </div>
        <div class="qp-gd-row">
          <span class="qp-gd-label">Into</span>
          <select v-model="base" class="qp-gd-select" aria-label="Base branch">
            <option v-for="b in bases" :key="b" :value="b">{{ b }}</option>
          </select>
          <span class="qp-gd-from">from <strong>{{ status.branch }}</strong> on {{ status.github }}</span>
        </div>
        <div class="qp-gd-row">
          <span class="qp-gd-label">Title</span>
          <input ref="titleInput" v-model="title" class="qp-gd-field" :placeholder="generating ? 'Writing…' : 'What the change does'" @input="edited = true" />
          <button class="qp-gd-gen" :disabled="generating" title="Write title and description from the commits, the diff and this thread" @click="generate">
            <LoaderCircle v-if="generating" class="qp-gd-spin" :size="12" />
            <Sparkles v-else :size="12" />
            {{ generating ? 'Writing…' : 'Generate' }}
          </button>
        </div>
        <textarea v-model="body" class="qp-gd-input" rows="12" spellcheck="true" placeholder="## Summary&#10;&#10;## Testing" @input="edited = true" />
        <div v-if="error" class="qp-gd-error">{{ error }}</div>
        <div class="qp-gd-foot">
          <label class="qp-gd-check"><input v-model="draft" type="checkbox" /> Draft</label>
          <span class="qp-gd-hint">{{ pushes ? `Pushes ${status.branch} to ${status.remote} first · ` : '' }}Ctrl+Enter creates</span>
          <button class="qp-gd-btn" @click="emit('close')">Cancel</button>
          <button class="qp-gd-btn primary" :disabled="!title.trim()" @click="create">Create pull request</button>
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
  width: min(680px, calc(100% - 32px));
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
.qp-gd-head { display: flex; align-items: center; gap: 6px; }
.qp-gd-title { font-size: 14px; font-weight: 600; }
.qp-gd-x { margin-left: auto; display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-gd-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-gd-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.qp-gd-label { width: 36px; flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-gd-from { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-gd-from strong { color: var(--qss-text, #d4d4d8); font-weight: 600; }
.qp-gd-select,
.qp-gd-field {
  padding: 4px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  outline: none;
}
.qp-gd-field { flex: 1; min-width: 0; font-size: 13px; }
.qp-gd-field:focus,
.qp-gd-select:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-gd-gen {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
  flex-shrink: 0;
}
.qp-gd-gen:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-gd-gen:disabled { opacity: 0.6; cursor: default; }
.qp-gd-spin { animation: qp-gd-rot 0.9s linear infinite; }
@keyframes qp-gd-rot { to { transform: rotate(360deg); } }
.qp-gd-input {
  resize: vertical;
  min-height: 180px;
  padding: 8px 10px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  line-height: 1.5;
  outline: none;
}
.qp-gd-input:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-gd-error { color: var(--qss-error, #f87171); white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-gd-foot { display: flex; align-items: center; gap: 6px; }
.qp-gd-check { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
.qp-gd-hint { margin-right: auto; margin-left: 8px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
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
