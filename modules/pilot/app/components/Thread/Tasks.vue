<script setup lang="ts">
/**
 * The turn's task list as one row outside the fold (card 16): "Tasks 3/7 ·
 * Running the tests". It opens into the checklist. Claude's TodoWrite or
 * TaskCreate/TaskUpdate and Codex's plan updates all arrive as one list.
 */
import { computed, ref } from 'vue'
import { Check, ChevronRight, Circle, ListChecks, Loader2 } from 'lucide-vue-next'
import type { ThreadItem } from '#pilot/types'
import { taskCounts } from '#pilot/utils/timeline'

const props = defineProps<{ item: ThreadItem; running: boolean }>()
const open = ref(false)

const todos = computed<{ status?: string; content?: string; activeForm?: string | null }[]>(() => (Array.isArray(props.item.data.todos) ? props.item.data.todos : []))
const counts = computed(() => taskCounts(todos.value))
</script>

<template>
  <div v-if="todos.length" class="qp-tasks" :data-item-id="item.id">
    <button class="qp-tasks-head" :aria-expanded="open" @click="open = !open">
      <ChevronRight class="qp-tasks-caret" :class="{ open }" :size="13" />
      <ListChecks :size="12" />
      <span class="qp-tasks-title">Tasks</span>
      <span class="qp-tasks-num">{{ counts.done }}/{{ counts.total }}</span>
      <span v-if="counts.current && running" class="qp-tasks-now">· {{ counts.current }}</span>
    </button>
    <ul v-if="open" class="qp-tasks-list">
      <li v-for="(t, i) in todos" :key="i" :class="`is-${t.status}`">
        <Check v-if="t.status === 'completed'" :size="12" class="qp-tasks-icon" />
        <Loader2 v-else-if="t.status === 'in_progress' && running" :size="12" class="qp-tasks-icon qp-spin" />
        <Circle v-else :size="12" class="qp-tasks-icon" />
        <span>{{ t.status === 'in_progress' ? t.activeForm || t.content : t.content }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.qp-tasks { margin: 0 0 8px; }
.qp-tasks-head {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 3px 6px 3px 2px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-tasks-head:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-tasks-caret { flex-shrink: 0; transition: transform 0.12s; }
.qp-tasks-caret.open { transform: rotate(90deg); }
.qp-tasks-title { font-weight: 500; }
.qp-tasks-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-tasks-now { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-tasks-list { list-style: none; margin: 2px 0 0; padding: 0 0 0 22px; display: flex; flex-direction: column; gap: 3px; }
.qp-tasks-list li { display: flex; align-items: flex-start; gap: 6px; font-size: 12.5px; line-height: 1.45; color: var(--qss-text, #d4d4d8); }
.qp-tasks-icon { flex-shrink: 0; margin-top: 2px; color: var(--qss-text-muted, #6e6e7a); }
.qp-tasks-list li.is-completed { color: var(--qss-text-muted, #6e6e7a); text-decoration: line-through; }
.qp-tasks-list li.is-completed .qp-tasks-icon { color: var(--qss-success, #34d399); }
.qp-tasks-list li.is-in_progress .qp-tasks-icon { color: var(--qss-accent, #7aa2f7); }
.qp-spin { animation: qp-spin 0.9s linear infinite; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
</style>
