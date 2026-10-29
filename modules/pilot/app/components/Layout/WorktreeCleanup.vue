<script setup lang="ts">
/**
 * After a worktree thread settles (card 10): remove its worktree, and its
 * branch? Safe defaults — a clean worktree goes, its branch only when
 * merged; unmerged commits keep the branch unless ticked; uncommitted
 * changes keep everything.
 */
import { computed, ref, watch } from 'vue'
import { FolderGit2 } from 'lucide-vue-next'
import { useGitStore } from '#pilot/stores/git'

const git = useGitStore()
const c = computed(() => git.cleanup)
const removeIt = ref(true)
const deleteBranch = ref(false)
const busy = ref(false)
const error = ref('')

watch(c, (v) => {
  if (!v) return
  removeIt.value = v.check.suggestRemove
  deleteBranch.value = v.check.suggestDeleteBranch
  error.value = ''
})

const dirty = computed(() => c.value?.check.dirty.length ?? 0)

async function apply() {
  const v = c.value
  if (!v) return
  if (!removeIt.value) {
    git.cleanup = null
    return
  }
  busy.value = true
  const err = await git.removeWorktree(v.threadId, deleteBranch.value, deleteBranch.value && !v.check.merged)
  busy.value = false
  if (err) {
    error.value = err
    return
  }
  git.cleanup = null
}
</script>

<template>
  <div v-if="c" class="qp-wc" role="dialog" aria-label="Clean up the worktree">
    <div class="qp-wc-head"><FolderGit2 :size="14" /> "{{ c.title }}" is settled. Clean up its worktree?</div>
    <p v-if="dirty" class="qp-wc-warn">{{ dirty }} uncommitted file{{ dirty === 1 ? '' : 's' }} — the worktree stays until they are committed or discarded.</p>
    <label class="qp-wc-check">
      <input v-model="removeIt" type="checkbox" :disabled="dirty > 0" />
      <span>Remove the worktree folder</span>
    </label>
    <label class="qp-wc-check">
      <input v-model="deleteBranch" type="checkbox" :disabled="!removeIt || dirty > 0" />
      <span>
        Delete branch <code>{{ c.check.branch }}</code>
        <em v-if="c.check.merged">(merged into {{ c.check.base }})</em>
        <em v-else class="qp-wc-risk">({{ c.check.ahead }} commit{{ c.check.ahead === 1 ? '' : 's' }} not in {{ c.check.base }} would be lost)</em>
      </span>
    </label>
    <p v-if="error" class="qp-wc-warn">{{ error }}</p>
    <div class="qp-wc-actions">
      <button class="qp-wc-btn" @click="git.cleanup = null">Keep everything</button>
      <button class="qp-wc-btn primary" :disabled="busy || dirty > 0 || !removeIt" @click="apply">{{ busy ? 'Removing…' : 'Clean up' }}</button>
    </div>
  </div>
</template>

<style scoped>
.qp-wc {
  position: absolute;
  right: 16px;
  bottom: 16px;
  z-index: 45;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: min(420px, calc(100% - 32px));
  padding: 12px 14px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 10px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  font-size: 12.5px;
  color: var(--qss-text, #d4d4d8);
}
.qp-wc-head { display: flex; align-items: center; gap: 6px; font-weight: 600; }
.qp-wc-warn { margin: 0; font-size: 12px; color: var(--qss-warning, #d29a3f); }
.qp-wc-check { display: flex; align-items: flex-start; gap: 8px; cursor: pointer; }
.qp-wc-check code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-wc-check em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-wc-check em.qp-wc-risk { color: var(--qss-warning, #d29a3f); }
.qp-wc-actions { display: flex; justify-content: flex-end; gap: 6px; }
.qp-wc-btn {
  padding: 4px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-wc-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-wc-btn.primary { background: var(--qss-accent, #7aa2f7); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-wc-btn:disabled { opacity: 0.5; cursor: default; }
</style>
