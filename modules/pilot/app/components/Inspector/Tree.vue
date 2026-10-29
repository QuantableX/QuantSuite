<script setup lang="ts">
/**
 * The checkout as a tree (card 13): folders load when opened, `.gitignore`
 * is honoured (the crate lists), the open file is marked and its folders
 * open when it changes. Up/Down move, Right/Left open and close a folder,
 * Enter opens a file.
 */
import { computed, nextTick, ref, watch } from 'vue'
import { ChevronRight, File, Folder, FolderOpen, RefreshCw } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import type { DirEntry } from '#pilot/types'

const props = defineProps<{ threadId: string; current: string | null }>()
const emit = defineEmits<{ open: [rel: string] }>()

const children = ref<Record<string, DirEntry[]>>({})
const expanded = ref(new Set<string>())
const loading = ref(new Set<string>())
const error = ref('')
const focusRel = ref<string | null>(null)
const list = ref<HTMLElement | null>(null)

async function load(rel: string) {
  if (loading.value.has(rel)) return
  loading.value = new Set([...loading.value, rel])
  try {
    // Await first: a spread written before the `await` would capture a stale map and drop a folder loaded meanwhile.
    const list = (await pilot<DirEntry[]>('plugin:pilot|pilot_files_list', { id: props.threadId, rel })) ?? []
    children.value = { ...children.value, [rel]: list }
    error.value = ''
  } catch (e) {
    if (rel === '') error.value = String(e)
  } finally {
    const next = new Set(loading.value)
    next.delete(rel)
    loading.value = next
  }
}

function reset() {
  children.value = {}
  expanded.value = new Set()
  void load('')
}
watch(() => props.threadId, reset, { immediate: true })

/** The open file's folders open, and its row is in view. */
watch(
  () => props.current,
  async (rel) => {
    if (!rel) return
    const parts = rel.split('/')
    const next = new Set(expanded.value)
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join('/')
      next.add(dir)
      if (!children.value[dir]) await load(dir)
    }
    expanded.value = next
    focusRel.value = rel
    await nextTick()
    list.value?.querySelector(`[data-rel="${CSS.escape(rel)}"]`)?.scrollIntoView({ block: 'nearest' })
  },
  { immediate: true }
)

interface Row {
  entry: DirEntry
  depth: number
}
const rows = computed<Row[]>(() => {
  const out: Row[] = []
  const walk = (rel: string, depth: number) => {
    for (const e of children.value[rel] ?? []) {
      out.push({ entry: e, depth })
      if (e.dir && expanded.value.has(e.rel)) walk(e.rel, depth + 1)
    }
  }
  walk('', 0)
  return out
})

function toggle(e: DirEntry, open?: boolean) {
  const next = new Set(expanded.value)
  const want = open ?? !next.has(e.rel)
  if (want) {
    next.add(e.rel)
    if (!children.value[e.rel]) void load(e.rel)
  } else next.delete(e.rel)
  expanded.value = next
}

function activate(e: DirEntry) {
  focusRel.value = e.rel
  if (e.dir) toggle(e)
  else emit('open', e.rel)
}

function onKey(ev: KeyboardEvent) {
  const i = rows.value.findIndex((r) => r.entry.rel === focusRel.value)
  const row = rows.value[i]
  const move = (to: number) => {
    const r = rows.value[Math.max(0, Math.min(rows.value.length - 1, to))]
    if (!r) return
    focusRel.value = r.entry.rel
    void nextTick(() => list.value?.querySelector(`[data-rel="${CSS.escape(r.entry.rel)}"]`)?.scrollIntoView({ block: 'nearest' }))
  }
  if (ev.key === 'ArrowDown') move(i + 1)
  else if (ev.key === 'ArrowUp') move(i < 0 ? 0 : i - 1)
  else if (ev.key === 'Home') move(0)
  else if (ev.key === 'End') move(rows.value.length - 1)
  else if (ev.key === 'ArrowRight' && row?.entry.dir) {
    if (!expanded.value.has(row.entry.rel)) toggle(row.entry, true)
    else move(i + 1)
  } else if (ev.key === 'ArrowLeft' && row) {
    if (row.entry.dir && expanded.value.has(row.entry.rel)) toggle(row.entry, false)
    else {
      const parent = row.entry.rel.split('/').slice(0, -1).join('/')
      if (parent) focusRel.value = parent
    }
  } else if (ev.key === 'Enter' && row) activate(row.entry)
  else return
  ev.preventDefault()
}

defineExpose({ refresh: reset })
</script>

<template>
  <div class="qp-tree">
    <div class="qp-tree-head">
      <span>Files</span>
      <button class="qp-tree-icon" title="Refresh the tree" @click="reset"><RefreshCw :size="12" /></button>
    </div>
    <div v-if="error" class="qp-tree-error">{{ error }}</div>
    <div ref="list" class="qp-tree-list" role="tree" tabindex="0" aria-label="Files" @keydown="onKey">
      <div
        v-for="r in rows"
        :key="r.entry.rel"
        class="qp-tree-row"
        role="treeitem"
        :data-rel="r.entry.rel"
        :aria-expanded="r.entry.dir ? expanded.has(r.entry.rel) : undefined"
        :aria-selected="r.entry.rel === current"
        :class="{ on: r.entry.rel === current, focus: r.entry.rel === focusRel }"
        :style="{ paddingLeft: `${6 + r.depth * 12}px` }"
        :title="r.entry.rel"
        @click="activate(r.entry)"
      >
        <ChevronRight v-if="r.entry.dir" class="qp-tree-chev" :class="{ open: expanded.has(r.entry.rel) }" :size="12" />
        <span v-else class="qp-tree-chev" />
        <component :is="r.entry.dir ? (expanded.has(r.entry.rel) ? FolderOpen : Folder) : File" class="qp-tree-kind" :size="13" />
        <span class="qp-tree-name">{{ r.entry.name }}</span>
        <span v-if="loading.has(r.entry.rel)" class="qp-tree-wait">…</span>
      </div>
      <div v-if="!rows.length && !error && !loading.has('')" class="qp-tree-empty">Nothing here.</div>
    </div>
  </div>
</template>

<style scoped>
.qp-tree { display: flex; flex-direction: column; min-height: 0; height: 100%; }
.qp-tree-head { display: flex; align-items: center; padding: 6px 8px 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-tree-icon { margin-left: auto; display: inline-flex; padding: 3px; border: none; border-radius: 4px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-tree-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-tree-error { padding: 6px 8px; font-size: 12px; color: var(--qss-warning, #d29a3f); }
.qp-tree-list { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 8px; outline: none; font-size: 12.5px; }
.qp-tree-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding-right: 6px;
  border-radius: 5px;
  color: var(--qss-text, #d4d4d8);
  white-space: nowrap;
  cursor: pointer;
  user-select: none;
}
.qp-tree-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-tree-row.on { background: var(--qss-bg-card, #292930); }
.qp-tree-list:focus-visible .qp-tree-row.focus { box-shadow: inset 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-tree-chev { width: 12px; flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); transition: transform 0.12s; }
.qp-tree-chev.open { transform: rotate(90deg); }
.qp-tree-kind { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-tree-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.qp-tree-wait { color: var(--qss-text-muted, #6e6e7a); }
.qp-tree-empty { padding: 8px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
@media (prefers-reduced-motion: reduce) {
  .qp-tree-chev { transition: none; }
}
</style>
