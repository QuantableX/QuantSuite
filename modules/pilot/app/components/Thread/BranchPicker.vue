<script setup lang="ts">
/**
 * Branches of the thread's checkout with a search (card 10). `switch`
 * switches or creates a branch in a Local thread; `base` picks what a new
 * worktree starts from; `name` names the worktree's new branch (an
 * existing one is checked out in the worktree instead).
 */
import { computed, nextTick, onMounted, ref } from 'vue'
import { Check, GitBranch, Plus } from 'lucide-vue-next'
import { useGitStore } from '#pilot/stores/git'
import type { BranchInfo } from '#pilot/types'

const props = defineProps<{ threadId: string; mode: 'switch' | 'base' | 'name'; current: string }>()
const emit = defineEmits<{ pick: [name: string, create: boolean]; close: [] }>()
const git = useGitStore()

const all = ref<BranchInfo[]>([])
const query = ref(props.mode === 'name' ? props.current : '')
const active = ref(0)
const input = ref<HTMLInputElement | null>(null)
const loading = ref(true)

onMounted(async () => {
  all.value = await git.branches(props.threadId)
  loading.value = false
  await nextTick()
  input.value?.focus()
  input.value?.select()
})

const VALID = /^(?!\/|-)(?!.*(\.\.|\/\/|@\{|\.lock(\/|$)|\/\.))[^\s~^:?*[\\]+(?<![./])$/
const rows = computed(() => {
  const q = query.value.trim().toLowerCase()
  const list = all.value.filter((b) => !q || b.name.toLowerCase().includes(q))
  const exact = all.value.some((b) => b.name === query.value.trim())
  const out: { name: string; create: boolean; note?: string; disabled?: boolean; current?: boolean }[] = []
  if (props.mode !== 'base' && query.value.trim() && !exact && VALID.test(query.value.trim())) {
    out.push({ name: query.value.trim(), create: true, note: props.mode === 'name' ? 'new branch for the worktree' : 'create from the current branch' })
  }
  for (const b of list) {
    const busy = props.mode === 'switch' && b.elsewhere
    out.push({ name: b.name, create: false, current: b.current, disabled: busy, note: busy ? 'checked out in a worktree' : props.mode === 'name' ? 'check out in the worktree' : undefined })
  }
  return out
})

function pick(i: number) {
  const r = rows.value[i]
  if (!r || r.disabled) return
  emit('pick', r.name, r.create)
}

function onKey(e: KeyboardEvent) {
  const n = rows.value.length
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (n) active.value = (active.value + (e.key === 'ArrowDown' ? 1 : -1) + n) % n
  } else if (e.key === 'Enter') {
    e.preventDefault()
    pick(active.value)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  }
}
const title = computed(() => (props.mode === 'base' ? 'Start the worktree from' : props.mode === 'name' ? "The worktree's branch" : 'Switch branch'))
</script>

<template>
  <div class="qp-bp" role="dialog" :aria-label="title" @mousedown.stop>
    <div class="qp-bp-title">{{ title }}</div>
    <input ref="input" v-model="query" class="qp-bp-search" spellcheck="false" placeholder="Search or name a branch" @keydown="onKey" @input="active = 0" />
    <div class="qp-bp-list" role="listbox">
      <div v-if="loading" class="qp-bp-empty">Reading branches…</div>
      <div v-else-if="!rows.length" class="qp-bp-empty">No branch matches</div>
      <button
        v-for="(r, i) in rows"
        :key="(r.create ? '+' : '') + r.name"
        class="qp-bp-row"
        role="option"
        :aria-selected="i === active"
        :class="{ on: i === active, off: r.disabled }"
        :disabled="r.disabled"
        @mouseenter="active = i"
        @click="pick(i)"
      >
        <Plus v-if="r.create" :size="12" class="qp-bp-icon" />
        <GitBranch v-else :size="12" class="qp-bp-icon" />
        <span class="qp-bp-name">{{ r.create ? `Create "${r.name}"` : r.name }}</span>
        <Check v-if="r.current || r.name === current" :size="12" class="qp-bp-check" />
        <span v-if="r.note" class="qp-bp-note">{{ r.note }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.qp-bp {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  z-index: 40;
  display: flex;
  flex-direction: column;
  width: 320px;
  max-height: 340px;
  padding: 6px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 10px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
}
.qp-bp-title { padding: 2px 6px 6px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-bp-search {
  margin-bottom: 4px;
  padding: 5px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  outline: none;
}
.qp-bp-search:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-bp-list { overflow-y: auto; }
.qp-bp-empty { padding: 8px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-bp-row {
  display: flex;
  align-items: center;
  gap: 6px;
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
.qp-bp-row.on { background: var(--qss-bg-hover, #313139); }
.qp-bp-row.off { opacity: 0.45; cursor: default; }
.qp-bp-icon { flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-bp-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-bp-check { flex-shrink: 0; color: var(--qss-accent, #7aa2f7); }
.qp-bp-note { margin-left: auto; flex-shrink: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
</style>
