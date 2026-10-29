<script setup lang="ts">
/**
 * What a turn changed (card 11): a card under the answer with the files and
 * their +/− counts. A file opens its diff in the inspector; "Open diff"
 * opens the whole turn.
 */
import { computed, ref } from 'vue'
import { FileDiff } from 'lucide-vue-next'
import { useInspectorStore } from '#pilot/stores/inspector'
import type { FileChange, ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const inspector = useInspectorStore()
const all = ref(false)

const files = computed<FileChange[]>(() => (Array.isArray(props.item.data.files) ? props.item.data.files : []))
const shown = computed(() => (all.value ? files.value : files.value.slice(0, 6)))
const split = (p: string) => {
  const i = p.lastIndexOf('/')
  return i < 0 ? { dir: '', name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) }
}

function open(path: string | null = null) {
  inspector.showDiff('turn', props.item.turnId, path)
}
</script>

<template>
  <div class="qp-chg" :data-item-id="item.id">
    <div class="qp-chg-head">
      <FileDiff :size="13" />
      <span class="qp-chg-title">{{ files.length }} file{{ files.length === 1 ? '' : 's' }} changed</span>
      <span class="qp-chg-num"><span class="add">+{{ item.data.added ?? 0 }}</span> <span class="del">−{{ item.data.removed ?? 0 }}</span></span>
      <button class="qp-chg-open" title="Open the turn's diff (Ctrl+D)" @click="open()">Open diff</button>
    </div>
    <button v-for="f in shown" :key="f.path" class="qp-chg-file" :title="f.path" @click="open(f.path)">
      <span class="qp-chg-st" :data-st="f.status">{{ f.status }}</span>
      <span class="qp-chg-dir">{{ split(f.path).dir }}</span><span class="qp-chg-name">{{ split(f.path).name }}</span>
      <span class="qp-chg-num"><span class="add">+{{ f.added }}</span> <span class="del">−{{ f.removed }}</span></span>
    </button>
    <button v-if="files.length > 6" class="qp-chg-more" @click="all = !all">{{ all ? 'Show fewer' : `Show all ${files.length}` }}</button>
  </div>
</template>

<style scoped>
.qp-chg {
  display: flex;
  flex-direction: column;
  margin: 8px 0 2px;
  padding: 6px;
  border-radius: 10px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg-raised, #1f1f25);
  font-size: 12px;
}
.qp-chg-head { display: flex; align-items: center; gap: 6px; padding: 2px 6px 4px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-chg-title { font-weight: 600; }
.qp-chg-open {
  margin-left: 6px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-chg-open:hover { background: var(--qss-bg-hover, #313139); }
.qp-chg-file {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 3px 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-chg-file:hover { background: var(--qss-bg-hover, #313139); }
.qp-chg-st { width: 14px; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; text-align: center; }
.qp-chg-st[data-st='A'] { color: var(--qss-success, #34d399); }
.qp-chg-st[data-st='D'] { color: var(--qss-error, #f87171); }
.qp-chg-st[data-st='M'] { color: var(--qss-warning, #d29a3f); }
.qp-chg-dir { flex-shrink: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-chg-name { flex-shrink: 0; white-space: nowrap; }
.qp-chg-num { margin-left: auto; flex-shrink: 0; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-chg-head .qp-chg-num { margin-left: 4px; }
.qp-chg-head .qp-chg-open { margin-left: auto; }
.add { color: var(--qss-success, #34d399); }
.del { color: var(--qss-error, #f87171); }
.qp-chg-more { align-self: flex-start; margin: 2px 6px; padding: 0; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); font: inherit; cursor: pointer; }
.qp-chg-more:hover { color: var(--qss-text, #d4d4d8); }
</style>
