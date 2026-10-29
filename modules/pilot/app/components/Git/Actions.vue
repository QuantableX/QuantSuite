<script setup lang="ts">
/**
 * The git actions control in the thread header (card 12; T3 Code's
 * GitActionsControl): the next sensible step as one button, every action in
 * the menu beside it — unavailable ones say why — and the outcome as a
 * toast. Commit and pull request open their dialogs; merge and discard ask
 * first. Outside git the control is not there.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ArrowDownToLine, ArrowUpFromLine, Check, ChevronDown, GitCommitHorizontal, GitMerge, GitPullRequest, LoaderCircle, X } from 'lucide-vue-next'
import { useGitStore } from '#pilot/stores/git'
import { useThreadStore } from '#pilot/stores/thread'
import { menuEntries, quickAction, type GitActionId } from '#pilot/utils/gitActions'
import { openExternal } from '#pilot/utils/external'

const props = defineProps<{ threadId: string }>()
const git = useGitStore()
const threads = useThreadStore()
const root = ref<HTMLElement | null>(null)
const menuOpen = ref(false)
const dialog = ref<null | 'commit' | 'pr' | 'merge' | 'discard'>(null)
const confirmBox = ref<HTMLElement | null>(null)
watch(dialog, () => void nextTick(() => confirmBox.value?.focus()))

const st = computed(() => git.status[props.threadId] ?? null)
const links = computed(() => git.links[props.threadId])
const busy = computed(() => git.busy[props.threadId] ?? null)
const quick = computed(() => quickAction(st.value, links.value, busy.value))
const entries = computed(() => menuEntries(st.value, links.value))
const running = computed(() => Boolean(threads.view(props.threadId)?.turnId))
const result = computed(() => (git.result?.threadId === props.threadId ? git.result : null))

const ICONS: Record<string, unknown> = { commit: GitCommitHorizontal, push: ArrowUpFromLine, pull: ArrowDownToLine, pr: GitPullRequest, merge: GitMerge }
const quickIcon = computed(() => (quick.value.id ? ICONS[quick.value.id] : Check))

watch(
  () => props.threadId,
  (id) => {
    menuOpen.value = false
    dialog.value = null
    void git.loadStatus(id)
    void git.refreshPrs(id)
  },
  { immediate: true }
)
// A finished turn usually changed the checkout.
watch(running, (now, before) => {
  if (before && !now) void git.loadStatus(props.threadId)
})

function act(id: string) {
  menuOpen.value = false
  if (busy.value) return
  switch (id as GitActionId) {
    case 'commit':
    case 'pr':
    case 'merge':
    case 'discard':
      dialog.value = id as 'commit' | 'pr' | 'merge' | 'discard'
      break
    case 'push':
      void git.push(props.threadId)
      break
    case 'pull':
      void git.pull(props.threadId)
      break
    case 'refresh':
      void git.loadStatus(props.threadId)
      void git.refreshPrs(props.threadId, true)
      break
  }
}

async function confirmDialog() {
  const which = dialog.value
  dialog.value = null
  if (which === 'merge') await git.merge(props.threadId)
  else if (which === 'discard') await git.discardAll(props.threadId)
}

let hideTimer: ReturnType<typeof setTimeout> | null = null
watch(result, (r) => {
  if (hideTimer) clearTimeout(hideTimer)
  // Success fades on its own; an error waits to be read.
  if (r?.ok) hideTimer = setTimeout(() => git.result === r && (git.result = null), 8000)
})

function onDocDown(e: MouseEvent) {
  if (menuOpen.value && root.value && !root.value.contains(e.target as Node)) menuOpen.value = false
}
function onFocus() {
  void git.loadStatus(props.threadId)
  void git.refreshPrs(props.threadId)
}
function onDialogKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    dialog.value = null
  } else if (e.key === 'Enter') {
    e.preventDefault()
    void confirmDialog()
  }
}
onMounted(() => {
  document.addEventListener('mousedown', onDocDown)
  window.addEventListener('focus', onFocus)
})
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocDown)
  window.removeEventListener('focus', onFocus)
  if (hideTimer) clearTimeout(hideTimer)
})
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? '' : 's'}`
</script>

<template>
  <div v-if="st" ref="root" class="qp-ga">
    <button class="qp-ga-main" :disabled="!quick.id || !!busy" :title="quick.hint" @click="quick.id && act(quick.id)">
      <LoaderCircle v-if="busy" class="qp-ga-spin" :size="13" />
      <component :is="quickIcon" v-else :size="13" />
      <span>{{ quick.label }}</span>
    </button>
    <button class="qp-ga-more" :class="{ on: menuOpen }" aria-label="Git actions" title="Git actions" @click="menuOpen = !menuOpen">
      <ChevronDown :size="13" />
    </button>
    <div v-if="menuOpen" class="qp-ga-pop">
      <div v-if="st.aiCredits.length" class="qp-ga-credit" :title="st.aiCredits.join('\n')">
        {{ plural(st.aiCredits.length, 'commit') }} to {{ st.worktree ? 'merge' : 'push' }} credit{{ st.aiCredits.length === 1 ? 's' : '' }} an AI — kept as written:
        <span class="qp-ga-credit-list">{{ st.aiCredits.slice(0, 3).join(' · ') }}</span>
      </div>
      <PilotThreadMenuList :options="entries" selected="" :title="st.branch ? `Git · ${st.branch}` : 'Git · detached HEAD'" @pick="act" @close="menuOpen = false" />
    </div>

    <PilotGitCommitDialog v-if="dialog === 'commit'" :thread-id="threadId" :status="st" @close="dialog = null" />
    <PilotGitPrDialog v-if="dialog === 'pr'" :thread-id="threadId" :status="st" @close="dialog = null" />
    <Teleport to="body">
      <div v-if="dialog === 'merge' || dialog === 'discard'" class="qp-ga-veil" @mousedown.self="dialog = null">
        <div ref="confirmBox" class="qp-ga-confirm" role="alertdialog" :aria-label="dialog === 'merge' ? 'Merge' : 'Discard all changes'" tabindex="-1" @keydown="onDialogKey">
          <template v-if="dialog === 'merge'">
            <div class="qp-ga-confirm-title">Merge {{ st.branch }} into {{ st.base }}?</div>
            <p class="qp-ga-confirm-text">
              {{ plural(st.aheadOfBase, 'commit') }} land in the project's checkout of {{ st.base }} as one merge commit. The worktree and its branch stay
              until you settle the thread. A conflict stops the merge and leaves {{ st.base }} untouched.
            </p>
            <p v-if="st.aiCredits.length" class="qp-ga-confirm-warn">{{ plural(st.aiCredits.length, 'commit') }} credit an AI and keep that line: {{ st.aiCredits.join(' · ') }}</p>
          </template>
          <template v-else>
            <div class="qp-ga-confirm-title">Discard all changes?</div>
            <p class="qp-ga-confirm-text">
              {{ plural(st.files.length, 'file') }} go back to the last commit and new files are deleted. Ignored files stay. This cannot be undone.
            </p>
          </template>
          <div class="qp-ga-confirm-actions">
            <button class="qp-ga-btn" @click="dialog = null">Cancel</button>
            <button class="qp-ga-btn" :class="dialog === 'merge' ? 'primary' : 'danger'" @click="confirmDialog">{{ dialog === 'merge' ? 'Merge' : 'Discard' }}</button>
          </div>
        </div>
      </div>
      <div v-if="result" class="qp-ga-toast" :class="{ err: !result.ok }" role="status">
        <component :is="result.ok ? Check : X" :size="13" class="qp-ga-toast-icon" />
        <span class="qp-ga-toast-text">{{ result.text }}</span>
        <button v-if="result.url" class="qp-ga-link" @click="openExternal(result.url)">Open</button>
        <button class="qp-ga-x" aria-label="Dismiss" @click="git.result = null"><X :size="12" /></button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.qp-ga { position: relative; display: inline-flex; align-items: stretch; flex-shrink: 0; }
.qp-ga-main,
.qp-ga-more {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--qss-border, #47474f);
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-ga-main { padding: 2px 9px; border-radius: 6px 0 0 6px; border-right: none; }
.qp-ga-more { padding: 2px 5px; border-radius: 0 6px 6px 0; }
.qp-ga-main:hover:not(:disabled),
.qp-ga-more:hover,
.qp-ga-more.on { background: var(--qss-bg-hover, #313139); }
.qp-ga-main:active:not(:disabled),
.qp-ga-more:active { background: var(--qss-bg-card, #292930); }
.qp-ga-main:disabled { color: var(--qss-text-muted, #6e6e7a); cursor: default; }
.qp-ga-spin { animation: qp-ga-rot 0.9s linear infinite; }
@keyframes qp-ga-rot { to { transform: rotate(360deg); } }
.qp-ga-pop { position: absolute; top: calc(100% + 6px); right: 0; z-index: 40; display: flex; flex-direction: column; gap: 4px; align-items: flex-end; }
.qp-ga-credit {
  max-width: 340px;
  padding: 6px 8px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, var(--qss-border, #47474f));
  background: var(--qss-bg-raised, #1f1f25);
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--qss-warning, #d29a3f);
}
.qp-ga-credit-list { display: block; color: var(--qss-text-secondary, #9a9aa5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ga-veil { position: fixed; inset: 0; z-index: 1100; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.45); }
.qp-ga-confirm {
  width: min(440px, calc(100% - 32px));
  padding: 16px 18px 14px;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
  animation: qp-ga-in 0.17s ease-out;
}
@keyframes qp-ga-in {
  from { opacity: 0; transform: translateY(4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-ga-confirm { animation: none; }
}
.qp-ga-confirm-title { font-size: 13.5px; font-weight: 600; margin-bottom: 8px; overflow-wrap: anywhere; }
.qp-ga-confirm-text { margin: 0 0 8px; line-height: 1.55; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ga-confirm-warn { margin: 0 0 8px; line-height: 1.5; color: var(--qss-warning, #d29a3f); }
.qp-ga-confirm-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 12px; }
.qp-ga-btn {
  padding: 4px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-ga-btn:hover { background: var(--qss-bg-hover, #313139); }
.qp-ga-btn.primary { background: var(--qss-accent, #7aa2f7); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-ga-btn.danger { background: var(--qss-error, #f87171); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-ga-toast {
  position: fixed;
  right: 24px;
  bottom: 24px;
  z-index: 1050;
  display: flex;
  align-items: flex-start;
  gap: 8px;
  max-width: 460px;
  padding: 9px 10px 9px 12px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  animation: qp-ga-in 0.17s ease-out;
}
.qp-ga-toast.err { border-color: color-mix(in srgb, var(--qss-error, #f87171) 50%, var(--qss-border, #47474f)); }
.qp-ga-toast-icon { flex-shrink: 0; margin-top: 2px; color: var(--qss-success, #34d399); }
.qp-ga-toast.err .qp-ga-toast-icon { color: var(--qss-error, #f87171); }
.qp-ga-toast-text { flex: 1; min-width: 0; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
.qp-ga-link { border: none; background: transparent; color: var(--qss-accent, #7aa2f7); font: inherit; cursor: pointer; padding: 0 2px; }
.qp-ga-x { display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-ga-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
