<script setup lang="ts">
/**
 * The inspector split (cards 11, 13, 14, 16): a pointer-dragged edge on the left
 * (the width is remembered per thread), closeable tabs — Ctrl+W closes the
 * one in front, Ctrl+Shift+T brings it back — and the tab's content.
 * Ctrl+Alt+B toggles the split, Ctrl+D the Diff tab; Ctrl+P and Ctrl+Shift+F
 * open Files.
 * Wide content lives here, never in the 220 px side panels.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Bot, FolderTree, GitCompare, Globe, X } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore, type InspectorTab } from '#pilot/stores/inspector'
import { useKeysStore } from '#pilot/stores/keys'

const props = defineProps<{ threadId: string }>()
const inspector = useInspectorStore()
const root = ref<HTMLElement | null>(null)
let drag: { startX: number; startW: number; max: number } | null = null

const keys = useKeysStore()
const META: Record<string, { label: string; icon: unknown; key: string; command?: string }> = {
  diff: { label: 'Diff', icon: GitCompare, key: 'Ctrl+D', command: 'diff.toggle' },
  files: { label: 'Files', icon: FolderTree, key: 'Ctrl+P picks a file', command: 'filePicker.toggle' },
  browser: { label: 'Browser', icon: Globe, key: 'Ctrl+Shift+J', command: 'preview.toggle' },
  agent: { label: 'Agent', icon: Bot, key: 'Agents in the thread details' },
}
const tabs = computed(() => inspector.tabs.filter((t) => t in META))
// The Agent tab comes back only for a thread with an agent picked.
const closedOnes = computed(() => (Object.keys(META) as InspectorTab[]).filter((t) => !inspector.tabs.includes(t) && (t !== 'agent' || inspector.agent[props.threadId])))

function onDown(e: PointerEvent) {
  if (e.button !== 0) return
  const host = root.value?.parentElement
  drag = { startX: e.clientX, startW: inspector.widthOf(props.threadId), max: (host?.clientWidth ?? 1200) - 360 }
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
  e.preventDefault()
}
function onMove(e: PointerEvent) {
  if (!drag) return
  const w = drag.startW + (drag.startX - e.clientX)
  inspector.setWidth(props.threadId, Math.max(320, Math.min(drag.max, w)))
}
function onUp() {
  drag = null
}
onBeforeUnmount(() => (drag = null))

// The Browser tab closed: its webview goes, and the page's session with it.
watch(
  () => inspector.tabs.includes('browser'),
  (open, was) => {
    if (was && !open) void pilot('plugin:pilot|pilot_browser_close').catch(() => undefined)
  }
)
</script>

<template>
  <aside ref="root" class="qp-insp" :style="{ width: `${inspector.widthOf(threadId)}px` }">
    <div class="qp-insp-edge" title="Drag to resize" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointercancel="onUp" />
    <div class="qp-insp-tabs" role="tablist">
      <div
        v-for="t in tabs"
        :key="t"
        class="qp-insp-tab"
        role="tab"
        tabindex="0"
        :aria-selected="inspector.tab === t"
        :class="{ on: inspector.tab === t }"
        :title="META[t].command ? keys.hint(META[t].command, META[t].label) : `${META[t].label} (${META[t].key})`"
        @click="inspector.show(t)"
        @keydown.enter="inspector.show(t)"
        @auxclick.middle.prevent="inspector.closeTab(t)"
      >
        <component :is="META[t].icon" :size="14" />
        <span>{{ META[t].label }}</span>
        <button class="qp-insp-tab-x" :title="keys.hint('rightPanel.close', `Close ${META[t].label}`)" @click.stop="inspector.closeTab(t)"><X :size="12" /></button>
      </div>
      <button
        v-for="t in closedOnes"
        :key="`add-${t}`"
        class="qp-insp-add"
        :title="META[t].command ? keys.hint(META[t].command, `Open ${META[t].label}`) : `Open ${META[t].label} (${META[t].key})`"
        @click="inspector.show(t)"
      >
        <component :is="META[t].icon" :size="14" />
      </button>
      <button class="qp-insp-close" :title="keys.hint('rightPanel.toggle', 'Close the inspector')" @click="inspector.toggle(false)"><X :size="14" /></button>
    </div>
    <div class="qp-insp-body">
      <PilotInspectorDiff v-if="inspector.tab === 'diff'" :thread-id="threadId" />
      <PilotInspectorFiles v-else-if="inspector.tab === 'files'" :thread-id="threadId" />
      <PilotInspectorBrowser v-else-if="inspector.tab === 'browser'" :thread-id="threadId" />
      <PilotInspectorAgent v-else-if="inspector.tab === 'agent'" :thread-id="threadId" />
    </div>
  </aside>
</template>

<style scoped>
.qp-insp {
  position: relative;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  min-width: 320px;
  min-height: 0;
  border-left: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg, #18181e);
}
.qp-insp-edge { position: absolute; left: -3px; top: 0; bottom: 0; width: 6px; cursor: col-resize; z-index: 5; }
.qp-insp-edge:hover { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 30%, transparent); }
.qp-insp-tabs { display: flex; align-items: center; gap: 2px; padding: 6px 8px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-insp-tab {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 4px 3px 9px;
  border-radius: 6px;
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 12px;
  cursor: pointer;
  user-select: none;
  outline: none;
}
.qp-insp-tab:hover { background: var(--qss-bg-hover, #313139); }
.qp-insp-tab:focus-visible { box-shadow: 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-insp-tab.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-insp-tab-x { display: inline-flex; padding: 2px; border: none; border-radius: 4px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; opacity: 0; }
.qp-insp-tab:hover .qp-insp-tab-x,
.qp-insp-tab.on .qp-insp-tab-x { opacity: 1; }
.qp-insp-tab-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-insp-add { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-insp-add:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-insp-close { margin-left: auto; display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-insp-close:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-insp-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
