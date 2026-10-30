<script setup lang="ts">
/**
 * The Files tab (card 13): the checkout's tree — or the project search
 * (Ctrl+Shift+F) — beside the open file. Ctrl+P picks a file by name;
 * `path:line` links in messages open here at the line. The side column
 * can be hidden to give the file the whole width.
 */
import { computed } from 'vue'
import { FolderTree, PanelLeftClose, PanelLeftOpen, Search } from 'lucide-vue-next'
import { useInspectorStore } from '#pilot/stores/inspector'
import { useKeysStore } from '#pilot/stores/keys'

const props = defineProps<{ threadId: string }>()
const inspector = useInspectorStore()
const keys = useKeysStore()
const target = computed(() => inspector.file[props.threadId] ?? null)
/** The tree marks a file inside the checkout by its relative path. */
const currentRel = computed(() => {
  const p = target.value?.path ?? null
  return p && !/^[a-z]:[\\/]|^[\\/]/i.test(p) ? p.replace(/\\/g, '/') : null
})

function open(rel: string, line: number | null = null) {
  inspector.openFile(props.threadId, rel, line)
}
</script>

<template>
  <div class="qp-files">
    <div class="qp-files-bar">
      <button class="qp-files-icon" :title="inspector.showTree ? 'Hide the side column' : 'Show the side column'" @click="inspector.setShowTree(!inspector.showTree)">
        <component :is="inspector.showTree ? PanelLeftClose : PanelLeftOpen" :size="14" />
      </button>
      <span class="qp-files-seg" role="radiogroup" aria-label="Side column">
        <button class="qp-files-seg-btn" role="radio" :aria-checked="inspector.filesMode === 'tree'" :class="{ on: inspector.filesMode === 'tree' }" title="Files" @click="inspector.filesMode = 'tree'; inspector.setShowTree(true)">
          <FolderTree :size="13" />
        </button>
        <button class="qp-files-seg-btn" role="radio" :aria-checked="inspector.filesMode === 'search'" :class="{ on: inspector.filesMode === 'search' }" :title="keys.hint('projectSearch.toggle', 'Search the project')" @click="inspector.filesMode = 'search'; inspector.setShowTree(true)">
          <Search :size="13" />
        </button>
      </span>
      <span class="qp-files-hint">Ctrl+P opens a file</span>
    </div>
    <div class="qp-files-body">
      <div v-if="inspector.showTree" class="qp-files-side">
        <PilotInspectorTree v-if="inspector.filesMode === 'tree'" :thread-id="threadId" :current="currentRel" @open="(rel: string) => open(rel)" />
        <PilotInspectorSearch v-else :thread-id="threadId" :current="currentRel" @open="(rel: string, line: number) => open(rel, line)" />
      </div>
      <PilotInspectorFileView v-if="target" :thread-id="threadId" :path="target.path" :line="target.line" :nonce="target.nonce" />
      <div v-else class="qp-files-none">Pick a file in the tree, or press Ctrl+P.</div>
    </div>
  </div>
</template>

<style scoped>
.qp-files { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.qp-files-bar { display: flex; align-items: center; gap: 6px; padding: 5px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-files-icon { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-files-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-files-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; overflow: hidden; }
.qp-files-seg-btn { display: inline-flex; padding: 3px 8px; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-files-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-files-hint { margin-left: auto; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-files-body { flex: 1; min-height: 0; display: flex; }
.qp-files-side { width: clamp(170px, 34%, 300px); flex-shrink: 0; min-height: 0; border-right: 1px solid var(--qss-border-subtle, #35353d); }
.qp-files-none { flex: 1; display: flex; align-items: center; justify-content: center; padding: 16px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); text-align: center; }
</style>
