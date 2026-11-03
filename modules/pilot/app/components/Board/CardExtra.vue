<script setup lang="ts">
/**
 * What QuantPilot adds to a kanban card on its board (card 21): priority,
 * the branch (and worktree) an agent works it on, the threads started from
 * it (their agent's logo, ringed by its state) — a click opens one beside
 * the board — and Start thread. The agent is
 * the shared board's own badge.
 */
import { computed, ref } from 'vue'
import type { KanbanCard } from '@quantsuite/core'
import { ChevronDown, GitBranch, Play } from 'lucide-vue-next'
import type { AgentSetup } from '#pilot/utils/agentMenu'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'

const props = defineProps<{ card: KanbanCard; threadIds: string[]; openId: string | null; starting: boolean }>()
const emit = defineEmits<{ open: [threadId: string]; start: [setup?: AgentSetup] }>()
/** ▾ beside Start: the agent menu (round 2, card 02). */
const startMore = ref<HTMLElement | null>(null)
const menuOpen = ref(false)
function startWith(setup: AgentSetup) {
  menuOpen.value = false
  emit('start', setup)
}
const store = usePilotStore()
const sidebar = useSidebarStore()

const PRIORITY: Record<string, string> = { low: 'Low', medium: 'Medium', high: 'High', critical: 'Critical' }

const linked = computed(() =>
  props.threadIds
    .map((id) => store.sessions.find((s) => s.id === id))
    .filter((s) => s !== undefined)
    .map((s) => ({ id: s.id, title: s.title || 'New thread', status: sidebar.statusOf(s.id), provider: s.provider || null }))
)
const branchTitle = computed(() => (props.card.worktree_path ? `Branch ${props.card.branch} — worktree ${props.card.worktree_path}` : `Branch ${props.card.branch}`))
const STATUS: Record<string, string> = { working: 'Working', waiting: 'Needs you', error: 'Failed', limited: 'Limited', idle: 'Idle' }
</script>

<template>
  <div class="qp-bc">
    <span class="qp-bc-prio" :class="`is-${card.priority}`">{{ PRIORITY[card.priority] ?? card.priority }}</span>
    <span v-if="card.branch" class="qp-bc-chip" :title="branchTitle">
      <GitBranch :size="10" />
      <span class="qp-bc-clip">{{ card.branch }}</span>
    </span>
    <button
      v-for="t in linked"
      :key="t.id"
      class="qp-bc-thread"
      :class="{ on: t.id === openId }"
      :title="`${t.title} — ${STATUS[t.status] ?? t.status}. Open it beside the board.`"
      @click="emit('open', t.id)"
    >
      <PilotAgentIcon :agent="t.provider" :size="11" :status="t.status" decorative />
      <span class="qp-bc-clip">{{ t.title }}</span>
    </button>
    <span v-if="card.column !== 'done'" class="qp-bc-startgroup">
      <button
        class="qp-bc-start"
        :disabled="starting"
        title="A new thread in this card's workspace, its first message prefilled with the card"
        @click="emit('start')"
      >
        <Play :size="10" />
        {{ starting ? 'Starting…' : 'Start thread' }}
      </button>
      <button
        ref="startMore"
        class="qp-bc-start qp-bc-start-more"
        :disabled="starting"
        title="Start with another agent or model"
        aria-haspopup="menu"
        :aria-expanded="menuOpen"
        @click.stop="menuOpen = !menuOpen"
      >
        <ChevronDown :size="10" />
      </button>
      <PilotAgentMenu v-if="menuOpen" :anchor="startMore" title="Start thread with" :project="card.workspace_id" @pick="startWith" @close="menuOpen = false" />
    </span>
  </div>
</template>

<style scoped>
.qp-bc {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  padding-top: 2px;
  font-size: 11px;
}
.qp-bc-prio,
.qp-bc-chip,
.qp-bc-thread,
.qp-bc-start {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 100%;
  min-width: 0;
  height: 19px;
  padding: 0 7px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 999px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 11px;
  line-height: 1;
}
.qp-bc-prio,
.qp-bc-chip { border-color: var(--qss-border, #47474f); }
.qp-bc-prio.is-low { color: var(--qss-text-muted, #6e6e7a); }
.qp-bc-prio.is-high { color: var(--qss-warning, #d29a3f); border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 40%, transparent); }
.qp-bc-prio.is-critical { color: var(--qss-error, #f87171); border-color: color-mix(in srgb, var(--qss-error, #f87171) 45%, transparent); }
.qp-bc-clip { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-bc-thread,
.qp-bc-start { cursor: pointer; }
.qp-bc-thread { max-width: 180px; color: var(--qss-text, #d4d4d8); }
.qp-bc-thread:hover,
.qp-bc-start:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); border-color: var(--qss-border, #47474f); color: var(--qss-text, #d4d4d8); }
.qp-bc-thread.on { border-color: color-mix(in srgb, var(--qss-accent, #7c8cff) 60%, var(--qss-border, #47474f)); }
.qp-bc-startgroup { display: inline-flex; margin-left: auto; }
.qp-bc-startgroup .qp-bc-start { border-radius: 999px 0 0 999px; }
.qp-bc-startgroup .qp-bc-start-more { padding: 0 5px; border-left: none; border-radius: 0 999px 999px 0; }
.qp-bc-start:disabled { opacity: 0.5; cursor: default; }
</style>
