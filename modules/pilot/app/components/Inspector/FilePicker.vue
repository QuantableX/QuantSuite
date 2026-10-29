<script setup lang="ts">
/**
 * Ctrl+P (card 13): a file of the thread's checkout by name — the same
 * gitignore-aware index the composer's `@` uses — opened in the Files tab.
 * Up/Down move, Enter opens, Escape closes.
 */
import { nextTick, onMounted, ref, watch } from 'vue'
import { File, Folder } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore } from '#pilot/stores/inspector'

const props = defineProps<{ threadId: string }>()
const emit = defineEmits<{ close: [] }>()
const inspector = useInspectorStore()

const query = ref('')
const hits = ref<{ path: string; dir: boolean }[]>([])
const active = ref(0)
const input = ref<HTMLInputElement | null>(null)
const list = ref<HTMLElement | null>(null)
let generation = 0

async function search() {
  const gen = ++generation
  try {
    const r = (await pilot<{ path: string; dir: boolean }[]>('plugin:pilot|pilot_files_search', { id: props.threadId, query: query.value })) ?? []
    if (gen !== generation) return
    hits.value = r.filter((h) => !h.dir)
    active.value = 0
  } catch {
    if (gen === generation) hits.value = []
  }
}
watch(query, () => void search())

function pick(i: number) {
  const h = hits.value[i]
  if (!h) return
  inspector.openFile(props.threadId, h.path)
  emit('close')
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') active.value = Math.min(hits.value.length - 1, active.value + 1)
  else if (e.key === 'ArrowUp') active.value = Math.max(0, active.value - 1)
  else if (e.key === 'Enter') pick(active.value)
  else if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  } else return
  e.preventDefault()
  void nextTick(() => list.value?.children[active.value]?.scrollIntoView({ block: 'nearest' }))
}

onMounted(() => {
  void search()
  void nextTick(() => input.value?.focus())
})
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i), name: p.slice(i + 1) }
}
</script>

<template>
  <Teleport to="body">
    <div class="qp-fp-veil" @mousedown.self="emit('close')">
      <div class="qp-fp" role="dialog" aria-label="Open a file" @keydown="onKey">
        <input ref="input" v-model="query" class="qp-fp-input" placeholder="Open a file by name" spellcheck="false" aria-label="File name" />
        <div ref="list" class="qp-fp-list" role="listbox">
          <button
            v-for="(h, i) in hits"
            :key="h.path"
            class="qp-fp-row"
            role="option"
            :aria-selected="i === active"
            :class="{ on: i === active }"
            @mouseenter="active = i"
            @click="pick(i)"
          >
            <component :is="h.dir ? Folder : File" :size="13" class="qp-fp-kind" />
            <span class="qp-fp-name">{{ split(h.path).name }}</span>
            <span class="qp-fp-dir">{{ split(h.path).dir }}</span>
          </button>
          <div v-if="!hits.length" class="qp-fp-empty">{{ query ? 'No file matches.' : 'Type to find a file.' }}</div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-fp-veil { position: fixed; inset: 0; z-index: 1100; display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh; background: rgba(0, 0, 0, 0.35); }
.qp-fp {
  display: flex;
  flex-direction: column;
  width: min(560px, calc(100% - 32px));
  max-height: 60vh;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5);
  overflow: hidden;
  animation: qp-fp-in 0.17s ease-out;
}
@keyframes qp-fp-in {
  from { opacity: 0; transform: translateY(-4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-fp { animation: none; }
}
.qp-fp-input { padding: 11px 14px; border: none; border-bottom: 1px solid var(--qss-border-subtle, #35353d); background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 13.5px; outline: none; }
.qp-fp-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px; }
.qp-fp-row { display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 5px 8px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; text-align: left; cursor: pointer; }
.qp-fp-row.on { background: var(--qss-bg-hover, #313139); }
.qp-fp-kind { flex-shrink: 0; align-self: center; color: var(--qss-text-muted, #6e6e7a); }
.qp-fp-name { white-space: nowrap; }
.qp-fp-dir { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-fp-empty { padding: 12px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); }
</style>
