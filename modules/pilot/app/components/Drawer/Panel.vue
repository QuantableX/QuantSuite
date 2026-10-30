<script setup lang="ts">
/**
 * The terminal drawer under the conversation (Ctrl+J, card 13): tabs of
 * shells in the thread's checkout and the project actions' Run terminals;
 * a tab splits beside (Ctrl+D) or below (Ctrl+Shift+D); Ctrl+N opens a new
 * one and Ctrl+W closes the pane with focus while the drawer has focus.
 * Selected output goes into the message as a "Terminal" chip. The top edge
 * drags the height.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { Columns2, MessageSquarePlus, Plus, Rows2, SquareTerminal, X } from 'lucide-vue-next'
import { useDrawerStore, type DrawerTab } from '#pilot/stores/drawer'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'

const props = defineProps<{ threadId: string }>()
const drawer = useDrawerStore()
const store = usePilotStore()
const threads = useThreadStore()
const root = ref<HTMLElement | null>(null)
const panes = ref<Record<string, { focusTerm: () => void; selection: () => { text: string; startLine: number; endLine: number } | null } | null>>({})
const selected = ref<{ ptyId: string; text: string } | null>(null)
const OVERRIDES = ['Ctrl+N', 'Ctrl+D', 'Ctrl+Shift+D', 'Ctrl+W', 'Ctrl+J']

const tabs = computed<DrawerTab[]>(() => drawer.tabs[props.threadId] ?? [])
const current = computed(() => drawer.current(props.threadId))
const cwd = computed(() => store.sessions.find((s) => s.id === props.threadId)?.cwd ?? '.')

watch(
  () => props.threadId,
  (id) => {
    selected.value = null
    void drawer.load(id)
  },
  { immediate: true }
)

function focusActive() {
  const t = current.value
  if (t) void nextTick(() => panes.value[t.active]?.focusTerm())
}
watch(() => current.value?.id, focusActive)

function onKey(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return
  const k = e.key.toLowerCase()
  const t = current.value
  if (k === 'n' && !e.shiftKey) void drawer.newTab(props.threadId)
  else if (k === 'd') void drawer.split(props.threadId, e.shiftKey ? 'column' : 'row')
  else if (k === 'w' && !e.shiftKey && t) void drawer.closePane(props.threadId, t.active)
  else if (k === 'j' && !e.shiftKey) drawer.toggle(props.threadId, false)
  else return
  // Handled here: the module's own Ctrl+N (new thread) and Ctrl+D (Diff) stand down.
  e.preventDefault()
  e.stopPropagation()
}

function onSelection(ptyId: string, text: string) {
  selected.value = text.trim() ? { ptyId, text } : selected.value?.ptyId === ptyId ? null : selected.value
}

function addToComposer() {
  const s = selected.value
  const sel = s ? panes.value[s.ptyId]?.selection() : null
  if (!s || !sel) return
  const tab = tabs.value.find((t) => t.panes.includes(s.ptyId))
  const lines = sel.startLine === sel.endLine ? `line ${sel.startLine}` : `lines ${sel.startLine}–${sel.endLine}`
  const chip = { kind: 'terminal', label: `${tab?.label ?? 'Terminal'} ${lines}`, content: `Terminal output (${tab?.label ?? 'Terminal'}, ${lines}):\n${sel.text}`, path: null }
  window.dispatchEvent(new CustomEvent('qp-add-context', { detail: { threadId: props.threadId, chip } }))
  selected.value = null
}

// ── Height ──
let drag: { startY: number; startH: number; max: number } | null = null
function onDown(e: PointerEvent) {
  if (e.button !== 0) return
  drag = { startY: e.clientY, startH: drawer.height, max: (root.value?.parentElement?.clientHeight ?? 800) * 0.75 }
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
  e.preventDefault()
}
function onMove(e: PointerEvent) {
  if (!drag) return
  drawer.setHeight(Math.max(120, Math.min(drag.max, drag.startH + (drag.startY - e.clientY))))
}
function onUp() {
  drag = null
}
onBeforeUnmount(() => (drag = null))

/** A pane's program ended; the agent's TUI hands the conversation back to the thread (card 15). */
function onExited(t: DrawerTab, ptyId: string) {
  drawer.exited[ptyId] = true
  if (t.action === 'tui') void threads.tuiDone(props.threadId)
}

function tabLabel(t: DrawerTab, i: number) {
  return t.action ? t.label : tabs.value.filter((x) => !x.action).length > 1 ? `${t.label} ${i + 1}` : t.label
}
</script>

<template>
  <section ref="root" class="qp-dr" :style="{ height: `${drawer.height}px` }" @keydown.capture="onKey">
    <div class="qp-dr-edge" title="Drag to resize" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointercancel="onUp" />
    <div class="qp-dr-bar" role="tablist" aria-label="Terminals">
      <div
        v-for="(t, i) in tabs"
        :key="t.id"
        class="qp-dr-tab"
        role="tab"
        tabindex="0"
        :aria-selected="current?.id === t.id"
        :class="{ on: current?.id === t.id, exited: t.panes.every((p) => drawer.exited[p]) }"
        :title="t.action ? `Run: ${t.label}` : 'Terminal'"
        @click="drawer.activeTab[threadId] = t.id"
        @auxclick.middle.prevent="drawer.closeTab(threadId, t.id)"
      >
        <SquareTerminal :size="12" />
        <span class="qp-dr-tab-label">{{ tabLabel(t, i) }}</span>
        <Columns2 v-if="t.panes.length > 1 && t.dir === 'row'" class="qp-dr-tab-split" :size="11" />
        <Rows2 v-else-if="t.panes.length > 1" class="qp-dr-tab-split" :size="11" />
        <button class="qp-dr-tab-x" title="Close (Ctrl+W closes the pane with focus)" @click.stop="drawer.closeTab(threadId, t.id)"><X :size="11" /></button>
      </div>
      <button class="qp-dr-icon" title="New terminal (Ctrl+N)" @click="drawer.newTab(threadId)"><Plus :size="13" /></button>
      <button class="qp-dr-icon" title="Split beside (Ctrl+D)" @click="drawer.split(threadId, 'row')"><Columns2 :size="13" /></button>
      <button class="qp-dr-icon" title="Split below (Ctrl+Shift+D)" @click="drawer.split(threadId, 'column')"><Rows2 :size="13" /></button>
      <span class="qp-dr-fill" />
      <button v-if="selected" class="qp-dr-add" title="Add the selected output to your message" @mousedown.prevent @click="addToComposer">
        <MessageSquarePlus :size="12" /> Add to composer
      </button>
      <button class="qp-dr-icon" title="Hide the drawer (Ctrl+J)" @click="drawer.toggle(threadId, false)"><X :size="13" /></button>
    </div>
    <div v-if="drawer.error[threadId]" class="qp-dr-error">{{ drawer.error[threadId] }}</div>
    <div class="qp-dr-body">
      <div v-for="t in tabs" v-show="current?.id === t.id" :key="t.id" class="qp-dr-split" :class="t.dir">
        <div v-for="p in t.panes" :key="p" class="qp-dr-pane" :class="{ focus: t.active === p && t.panes.length > 1 }" @pointerdown="drawer.updateTab(threadId, t.id, { active: p })">
          <ConsolePane
            :ref="(el: any) => (panes[p] = el)"
            :session-id="p"
            :cwd="cwd"
            owner="pilot-drawer"
            :label="t.label"
            :visible="current?.id === t.id"
            :active="t.active === p"
            :terminal-overrides="OVERRIDES"
            @exited="onExited(t, p)"
            @failed="(m: string) => (drawer.error[threadId] = m)"
            @selection="(text: string) => onSelection(p, text)"
          />
        </div>
      </div>
      <div v-if="!tabs.length" class="qp-dr-empty">No terminal. <button class="qp-dr-link" @click="drawer.newTab(threadId)">Open one</button></div>
    </div>
  </section>
</template>

<style scoped>
.qp-dr {
  position: relative;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-height: 120px;
  border-top: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg, #18181e);
  --cpane-font-size: 12.5px;
}
.qp-dr-edge { position: absolute; left: 0; right: 0; top: -3px; height: 6px; cursor: row-resize; z-index: 5; }
.qp-dr-edge:hover { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 30%, transparent); }
.qp-dr-bar { display: flex; align-items: center; gap: 2px; padding: 4px 6px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); min-width: 0; }
.qp-dr-tab {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 200px;
  padding: 2px 3px 2px 8px;
  border-radius: 6px;
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 12px;
  cursor: pointer;
  user-select: none;
  outline: none;
}
.qp-dr-tab:hover { background: var(--qss-bg-hover, #313139); }
.qp-dr-tab:focus-visible { box-shadow: 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-dr-tab.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-dr-tab.exited .qp-dr-tab-label { text-decoration: line-through; color: var(--qss-text-muted, #6e6e7a); }
.qp-dr-tab-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-dr-tab-split { flex-shrink: 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-dr-tab-x { display: inline-flex; padding: 2px; border: none; border-radius: 4px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; opacity: 0; }
.qp-dr-tab:hover .qp-dr-tab-x,
.qp-dr-tab.on .qp-dr-tab-x { opacity: 1; }
.qp-dr-tab-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-dr-icon { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-dr-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-dr-fill { flex: 1; }
.qp-dr-add {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-dr-add:hover { background: var(--qss-bg-hover, #313139); }
.qp-dr-error { padding: 4px 10px; font-size: 12px; color: var(--qss-warning, #d29a3f); }
.qp-dr-body { position: relative; flex: 1; min-height: 0; }
.qp-dr-split { position: absolute; inset: 0; display: flex; }
.qp-dr-split.column { flex-direction: column; }
.qp-dr-pane { position: relative; flex: 1; min-width: 0; min-height: 0; }
.qp-dr-split.row .qp-dr-pane + .qp-dr-pane { border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dr-split.column .qp-dr-pane + .qp-dr-pane { border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-dr-pane.focus { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--qss-accent, #7aa2f7) 40%, transparent); }
.qp-dr-empty { padding: 14px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-dr-link { border: none; background: transparent; color: var(--qss-accent, #7aa2f7); font: inherit; cursor: pointer; padding: 0; }
</style>
