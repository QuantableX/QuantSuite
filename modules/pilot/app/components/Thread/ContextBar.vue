<script setup lang="ts">
/**
 * The line above the composer (card 10; T3 Code's branch toolbar): the
 * project, Local or Worktree (switchable until the first message), the
 * branch — a picker to switch or create in a Local thread, the base and
 * the planned branch name in a new Worktree thread — a "Worktree missing"
 * chip with Recreate, and the checkout path to copy. Threads outside git
 * show only the project.
 *
 * Keys: Ctrl+Shift+X Local/Worktree, Ctrl+Shift+G branch, Ctrl+Shift+L
 * reuse the previous worktree (dispatched as `qp-context-bar` events).
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { AlertTriangle, Copy, FolderGit2, GitBranch, Laptop } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useGitStore } from '#pilot/stores/git'
import { useSidebarStore } from '#pilot/stores/sidebar'

const props = defineProps<{ threadId: string }>()
const store = usePilotStore()
const git = useGitStore()

const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const info = computed(() => git.info[props.threadId] ?? null)
const repo = computed(() => info.value?.repo ?? null)
const started = computed(() => Boolean(info.value?.started))
const isWorktree = computed(() => info.value?.mode === 'worktree')
const draftWorktree = computed(() => isWorktree.value && !thread.value?.worktreePath)
const project = computed(() => thread.value?.contextName ?? '')
const picker = ref<'branch' | 'base' | null>(null)
const message = ref('')

watch(
  () => [props.threadId, thread.value?.cwd, thread.value?.providerSessionId, thread.value?.worktreePath],
  () => void git.refresh(props.threadId),
  { immediate: true }
)

function say(text: string | null) {
  message.value = text ?? ''
  if (text) setTimeout(() => message.value === text && (message.value = ''), 6000)
}

async function setMode(mode: 'local' | 'worktree') {
  if (started.value || info.value?.mode === mode) return
  say(await git.setWorkspace(props.threadId, { mode, baseBranch: info.value?.baseBranch }))
}

async function onPick(name: string, create: boolean) {
  const which = picker.value
  picker.value = null
  if (which === 'base') say(await git.setWorkspace(props.threadId, { mode: 'worktree', baseBranch: name }))
  else if (draftWorktree.value) say(await git.setWorkspace(props.threadId, { mode: 'worktree', baseBranch: info.value?.baseBranch, branch: name }))
  else say(await git.checkout(props.threadId, name, create, create ? repo.value?.branch : undefined))
}

async function copyPath() {
  const path = thread.value?.cwd
  if (!path) return
  await navigator.clipboard.writeText(path).catch(() => undefined)
  say('Path copied')
}

async function recreate() {
  say(await git.recreate(props.threadId))
}

function onKeyEvent(e: Event) {
  const d = (e as CustomEvent<{ threadId: string; which: 'mode' | 'branch' | 'reuse' }>).detail
  if (d?.threadId !== props.threadId || !repo.value) return
  if (d.which === 'mode') void setMode(isWorktree.value ? 'local' : 'worktree')
  else if (d.which === 'branch') picker.value = draftWorktree.value ? 'base' : started.value && isWorktree.value ? null : 'branch'
  else if (d.which === 'reuse' && !started.value) void git.reusePrevious(props.threadId).then(say)
}
onMounted(() => window.addEventListener('qp-context-bar', onKeyEvent))
onBeforeUnmount(() => window.removeEventListener('qp-context-bar', onKeyEvent))

const branchLabel = computed(() => {
  if (!info.value) return ''
  if (draftWorktree.value) return info.value.branch
  return repo.value?.branch ?? info.value.branch
})
const baseLabel = computed(() => info.value?.baseBranch || repo.value?.branch || 'current branch')
</script>

<template>
  <div class="qp-cb">
    <span class="qp-cb-project" :title="thread?.cwd">{{ project }}</span>
    <template v-if="info?.worktreeMissing">
      <span class="qp-cb-sep">·</span>
      <span class="qp-cb-missing"><AlertTriangle :size="12" /> Worktree missing</span>
      <button class="qp-cb-btn" @click="recreate">Recreate</button>
      <button class="qp-cb-btn" @click="useSidebarStore().settle(threadId)">Settle</button>
    </template>
    <template v-else-if="repo">
      <span class="qp-cb-sep">·</span>
      <span v-if="!started" class="qp-cb-seg" role="radiogroup" aria-label="Where the thread works" title="Local or Worktree (Ctrl+Shift+X)">
        <button class="qp-cb-seg-btn" role="radio" :aria-checked="!isWorktree" :class="{ on: !isWorktree }" @click="setMode('local')">
          <Laptop :size="12" /> Local
        </button>
        <button class="qp-cb-seg-btn" role="radio" :aria-checked="isWorktree" :class="{ on: isWorktree }" @click="setMode('worktree')">
          <FolderGit2 :size="12" /> Worktree
        </button>
      </span>
      <span v-else class="qp-cb-mode"><component :is="isWorktree ? FolderGit2 : Laptop" :size="12" /> {{ isWorktree ? 'Worktree' : 'Local' }}</span>
      <span class="qp-cb-sep">·</span>
      <template v-if="draftWorktree">
        <span class="qp-cb-anchor">
          <button class="qp-cb-chip" title="Base branch (Ctrl+Shift+G)" @click="picker = picker === 'base' ? null : 'base'">from {{ baseLabel }}</button>
          <PilotThreadBranchPicker v-if="picker === 'base'" :thread-id="threadId" mode="base" :current="baseLabel" @pick="onPick" @close="picker = null" />
        </span>
        <span class="qp-cb-anchor">
          <button class="qp-cb-chip" title="The new branch — created with the first message" @click="picker = picker === 'branch' ? null : 'branch'">
            <GitBranch :size="12" /> {{ branchLabel }}
          </button>
          <PilotThreadBranchPicker v-if="picker === 'branch'" :thread-id="threadId" mode="name" :current="branchLabel" @pick="onPick" @close="picker = null" />
        </span>
      </template>
      <span v-else class="qp-cb-anchor">
        <button
          class="qp-cb-chip"
          :disabled="isWorktree"
          :title="isWorktree ? 'The worktree stays on its branch' : 'Switch or create a branch (Ctrl+Shift+G)'"
          @click="picker = picker === 'branch' ? null : 'branch'"
        >
          <GitBranch :size="12" /> {{ branchLabel }}
          <span v-if="repo.dirty" class="qp-cb-dirty" :title="`${repo.dirty} changed file(s)`">●</span>
        </button>
        <PilotThreadBranchPicker v-if="picker === 'branch'" :thread-id="threadId" mode="switch" :current="branchLabel" @pick="onPick" @close="picker = null" />
      </span>
    </template>
    <span v-if="message" class="qp-cb-msg" role="status">{{ message }}</span>
    <button class="qp-cb-copy" title="Copy the checkout's path" @click="copyPath"><Copy :size="12" /></button>
  </div>
</template>

<style scoped>
.qp-cb {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 0 4px 6px;
  font-size: 12px;
  color: var(--qss-text-secondary, #9a9aa5);
}
.qp-cb-project { flex-shrink: 0; max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text, #d4d4d8); }
.qp-cb-sep { color: var(--qss-text-muted, #6e6e7a); }
.qp-cb-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; overflow: hidden; }
.qp-cb-seg-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 8px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  cursor: pointer;
}
.qp-cb-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-cb-mode { display: inline-flex; align-items: center; gap: 4px; }
.qp-cb-anchor { position: relative; display: inline-flex; min-width: 0; }
.qp-cb-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 260px;
  padding: 1px 8px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}
.qp-cb-chip:hover:not(:disabled) { border-color: var(--qss-border, #47474f); }
.qp-cb-chip:disabled { cursor: default; }
.qp-cb-dirty { color: var(--qss-warning, #d29a3f); font-size: 9px; }
.qp-cb-missing { display: inline-flex; align-items: center; gap: 4px; color: var(--qss-warning, #d29a3f); }
.qp-cb-btn {
  padding: 1px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-cb-btn:hover { background: var(--qss-bg-hover, #313139); }
.qp-cb-msg { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-warning, #d29a3f); }
.qp-cb-copy {
  margin-left: auto;
  display: inline-flex;
  padding: 3px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  cursor: pointer;
}
.qp-cb-copy:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
