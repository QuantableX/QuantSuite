<script setup lang="ts">
/**
 * The drawer's To-dos tab (PLAN-V2 E3) — the suite's shared to-dos
 * (`todos/sections` in core.db), the same document the QuantHUD overlay
 * edits. Quick capture and ticking; the overlay keeps the heavier tooling
 * (subtask editing, reordering, duplication).
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { dataplaneId, todosStore, type TodoSection } from '@quantsuite/core'

const sections = ref<TodoSection[]>([])
const loaded = ref(false)
const drafts = ref<Record<string, string>>({})

let saveTimer: ReturnType<typeof setTimeout> | null = null
function persist() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    void todosStore.save(JSON.parse(JSON.stringify(sections.value)))
  }, 250)
}

function addSection() {
  sections.value.push({ id: dataplaneId(), name: 'New section', collapsed: false, tasks: [] })
  persist()
}
function deleteSection(id: string) {
  sections.value = sections.value.filter((s) => s.id !== id)
  persist()
}
function addTask(sectionId: string) {
  const title = (drafts.value[sectionId] ?? '').trim()
  if (!title) return
  const s = sections.value.find((s) => s.id === sectionId)
  if (!s) return
  s.tasks.push({ id: dataplaneId(), title, done: false, expanded: false, subtasks: [] })
  drafts.value[sectionId] = ''
  persist()
}
function deleteTask(sectionId: string, taskId: string) {
  const s = sections.value.find((s) => s.id === sectionId)
  if (!s) return
  s.tasks = s.tasks.filter((t) => t.id !== taskId)
  persist()
}

let off: (() => void) | undefined
let disposed = false

onMounted(async () => {
  sections.value = (await todosStore.load().catch(() => null)) ?? []
  loaded.value = true
  // A tab switch unmounts this inside the load above — the unmount hook then
  // ran with `off` still undefined and the subscription outlived the tab.
  const stop = todosStore.onChange((next) => {
    // The store echoes this window's own save too. While a save is still
    // pending, local state is the newer one — adopting the echo would drop
    // every edit made since it went out, and then persist the rollback.
    if (saveTimer) return
    if (JSON.stringify(next) !== JSON.stringify(sections.value)) sections.value = next
  })
  if (disposed) stop()
  else off = stop
})
onUnmounted(() => {
  disposed = true
  off?.()
})
</script>

<template>
  <div class="dtw">
    <div v-for="s in sections" :key="s.id" class="dtw-section">
      <div class="dtw-head">
        <input v-model="s.name" class="dtw-name" @change="persist" />
        <span class="dtw-count">{{ s.tasks.filter(t => !t.done).length }}</span>
        <button class="dtw-mini" title="Delete section" @click="deleteSection(s.id)">&times;</button>
      </div>

      <label v-for="t in s.tasks" :key="t.id" class="dtw-task" :class="{ 'is-done': t.done }">
        <input type="checkbox" :checked="t.done" @change="t.done = !t.done; persist()" />
        <span class="dtw-task-title">{{ t.title }}</span>
        <span v-if="t.subtasks.length" class="dtw-sub">
          {{ t.subtasks.filter(st => st.done).length }}/{{ t.subtasks.length }}
        </span>
        <button class="dtw-mini" title="Delete" @click.prevent="deleteTask(s.id, t.id)">&times;</button>
      </label>

      <input
        v-model="drafts[s.id]"
        class="dtw-new"
        placeholder="Add a task…"
        @keydown.enter="addTask(s.id)"
      />
    </div>

    <button class="dtw-add" @click="addSection">+ Section</button>

    <p v-if="loaded && !sections.length" class="dtw-empty">
      No to-dos yet. They are shared with the QuantHUD overlay — add one on either side.
    </p>
  </div>
</template>

<style scoped>
.dtw {
  height: 100%;
  overflow-y: auto;
  padding: 12px 14px;
  display: flex;
  flex-wrap: wrap;
  align-content: flex-start;
  gap: 14px;
}

.dtw-section {
  width: 300px;
  padding: 10px 12px;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 9px;
  background: color-mix(in srgb, var(--qss-bg-card) 40%, transparent);
}

.dtw-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.dtw-name {
  flex: 1;
  min-width: 0;
  border: none;
  background: none;
  color: var(--qss-text-secondary);
  font: 600 11px/1.6 var(--qss-font-sans);
  letter-spacing: 0.03em;
  text-transform: uppercase;
}
.dtw-name:focus {
  outline: none;
  color: var(--qss-text);
}
.dtw-count {
  font: 500 10px/1 var(--qss-font-mono);
  color: var(--qss-text-muted);
}

.dtw-task {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 2px;
  cursor: pointer;
}
.dtw-task input {
  accent-color: var(--qss-accent);
}
.dtw-task-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text);
  font-size: 12.5px;
}
.dtw-task.is-done .dtw-task-title {
  color: var(--qss-text-muted);
  text-decoration: line-through;
}
.dtw-sub {
  font: 500 10px/1 var(--qss-font-mono);
  color: var(--qss-text-muted);
}

.dtw-mini {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--qss-text-muted);
  font-size: 12px;
  cursor: pointer;
  opacity: 0;
}
.dtw-task:hover .dtw-mini,
.dtw-head:hover .dtw-mini {
  opacity: 1;
}
.dtw-mini:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.dtw-new {
  width: 100%;
  margin-top: 4px;
  padding: 5px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: none;
  color: var(--qss-text);
  font-size: 12px;
}
.dtw-new::placeholder {
  color: var(--qss-text-muted);
}
.dtw-new:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--qss-accent) 50%, var(--qss-border));
}

.dtw-add {
  align-self: flex-start;
  padding: 8px 12px;
  border: 1px solid var(--qss-border);
  border-radius: 9px;
  background: none;
  color: var(--qss-text-muted);
  font: 500 11px/1 var(--qss-font-sans);
  cursor: pointer;
}
.dtw-add:hover {
  color: var(--qss-text);
  background: var(--qss-bg-hover);
}

.dtw-empty {
  width: 100%;
  margin: auto;
  color: var(--qss-text-muted);
  font-size: 12px;
  text-align: center;
}
</style>
