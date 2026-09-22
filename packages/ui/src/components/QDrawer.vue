<script setup lang="ts">
/**
 * The drawer — the suite's integration surface (PLAN-V2 §2).
 *
 * A full-width strip along the window's bottom edge, modelled on QuantCode's
 * expandable notes strip. A right sidebar was considered and rejected: a
 * kanban board is unusable in a narrow column, and this panel will hold one.
 *
 * Collapsed: one slim row with the tabs. Expanded: a full-width panel whose
 * height the user drags; the height is persisted. The drawer takes height
 * from the module by relayout, never by overlaying it — a chart half-hidden
 * behind a panel reads as broken (PLAN-V2 §7).
 *
 * E1 ships the shell only. The tabs' real content arrives with the shared
 * data plane (E3: Notes, To-dos, Kanban) and the agent layer (E4: Agent,
 * Feed); until then each tab states plainly what will live in it.
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { isModuleEnabled, type PendingAgentCall } from '@quantsuite/core'

const props = defineProps<{
  /** Prompted agent calls awaiting the user (E4). Badges the Agent tab. */
  agentQueue?: PendingAgentCall[]
}>()
const emit = defineEmits<{
  (e: 'approve', call: PendingAgentCall): void
  (e: 'reject', call: PendingAgentCall): void
}>()

const TABS = [
  { id: 'notes', label: 'Notes' },
  { id: 'todos', label: 'To-dos' },
  { id: 'kanban', label: 'Kanban' },
  { id: 'agent', label: 'Agent' },
  { id: 'feed', label: 'Feed' },
] as const
type TabId = (typeof TABS)[number]['id']
const tabModules: Partial<Record<TabId, string>> = { notes: 'notes', todos: 'flow', kanban: 'mcp' }
const visibleTabs = computed(() => TABS.filter((t) => isModuleEnabled(tabModules[t.id] ?? '')))

const open = ref(false)
const height = ref(300)
const tab = ref<TabId>('notes')
watch([visibleTabs, tab], () => {
  if (!visibleTabs.value.some((t) => t.id === tab.value)) {
    tab.value = visibleTabs.value[0]!.id
    open.value = false
  }
}, { immediate: true })

const MIN_H = 160
const maxH = () => Math.round(window.innerHeight * 0.6)

/** Persisted so the drawer greets you the way you left it. */
const STORE = 'qss-drawer'

/** Bumped to signal quick capture into the freshly opened tab (E5). */
const captureTick = ref(0)

/**
 * `qss:drawer` — the palette and global shortcuts open the drawer from
 * anywhere: `{ tab, capture? }`. A window event, not a prop, because the
 * senders (palette, app-level keybindings) live outside this tree.
 */
function onDrawerEvent(e: Event) {
  const detail = (e as CustomEvent).detail as { tab?: TabId; capture?: boolean }
  if (detail?.tab && !visibleTabs.value.some((t) => t.id === detail.tab)) return
  if (detail?.tab) tab.value = detail.tab
  open.value = true
  if (detail?.capture) captureTick.value++
  persist()
}

onMounted(() => {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE) ?? '{}')
    if (typeof saved.open === 'boolean') open.value = saved.open
    if (typeof saved.height === 'number') height.value = Math.min(Math.max(saved.height, MIN_H), maxH())
    if (visibleTabs.value.some((t) => t.id === saved.tab)) tab.value = saved.tab
  } catch {
    // corrupt state — defaults are fine
  }
  window.addEventListener('qss:drawer', onDrawerEvent)
})
onUnmounted(() => {
  window.removeEventListener('qss:drawer', onDrawerEvent)
})

function persist() {
  localStorage.setItem(STORE, JSON.stringify({ open: open.value, height: height.value, tab: tab.value }))
}

function pick(id: TabId) {
  captureTick.value = 0 // manual navigation is never a capture
  if (open.value && tab.value === id) {
    open.value = false
  } else {
    tab.value = id
    open.value = true
  }
  persist()
}

function toggle() {
  captureTick.value = 0
  open.value = !open.value
  persist()
}

/** Drag the top edge to resize. Pointer events, so capture survives fast drags. */
function startResize(down: PointerEvent) {
  const startY = down.clientY
  const startH = height.value
  const el = down.currentTarget as HTMLElement
  el.setPointerCapture(down.pointerId)

  const move = (e: PointerEvent) => {
    height.value = Math.min(Math.max(startH + (startY - e.clientY), MIN_H), maxH())
  }
  const up = () => {
    el.removeEventListener('pointermove', move)
    el.removeEventListener('pointerup', up)
    persist()
  }
  el.addEventListener('pointermove', move)
  el.addEventListener('pointerup', up)
}
</script>

<template>
  <!-- The strip keeps its slim height in the layout at ALL times; the
       expanded body floats ABOVE it as an overlay. The module never
       relayouts when the drawer opens — its floating UI stays put and the
       drawer simply covers whatever it rises over (user decision
       2026-08-14, reversing the earlier relayout rule in PLAN-V2 §7). -->
  <section class="qss-drawer" :class="{ 'is-open': open }">
    <div class="qss-drawer-bar">
      <button
        v-for="t in visibleTabs"
        :key="t.id"
        class="qss-drawer-tab"
        :class="{ 'is-active': open && tab === t.id }"
        @click="pick(t.id)"
      >
        {{ t.label }}
        <span v-if="t.id === 'agent' && props.agentQueue?.length" class="qss-drawer-badge">
          {{ props.agentQueue.length }}
        </span>
      </button>

      <span class="qss-drawer-spacer" />

      <button class="qss-drawer-toggle" :title="open ? 'Collapse' : 'Expand'" @click="toggle">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" :style="{ transform: open ? 'rotate(180deg)' : undefined }">
          <path d="M18 15l-6-6-6 6" />
        </svg>
      </button>
    </div>

    <div v-if="open" class="qss-drawer-body" :style="{ height: height + 'px' }">
      <div class="qss-drawer-grip" title="Drag to resize" @pointerdown="startResize" />
      <QDrawerNotes v-if="tab === 'notes'" :capture="captureTick" />
      <QDrawerTodos v-else-if="tab === 'todos'" />
      <QDrawerKanban v-else-if="tab === 'kanban'" />
      <QDrawerAgent
        v-else-if="tab === 'agent'"
        :queue="props.agentQueue ?? []"
        @approve="emit('approve', $event)"
        @reject="emit('reject', $event)"
      />
      <QDrawerFeed v-else-if="tab === 'feed'" />
    </div>
  </section>
</template>

<style scoped>
.qss-drawer {
  position: relative;
  /* Above the module panel's stacking context AND the rail's (z-index 40,
     for its flyouts) — the expanded sheet spans the whole window, so it must
     cover module UI near the bottom edge and the rail column alike; a rail
     painting over it slices the sheet's left edge off. */
  z-index: 50;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--qss-bg-chrome, var(--qss-bg-raised));
  user-select: none;
}

.qss-drawer-grip {
  position: absolute;
  top: -3px;
  left: 0;
  right: 0;
  height: 8px;
  cursor: ns-resize;
  z-index: 1;
}

.qss-drawer-bar {
  display: flex;
  align-items: center;
  gap: 2px;
  /* The strip owns the gutter above the tab row as well (the shell no longer
     pads the stage's bottom), so `align-items: center` lands the labels in
     the middle of the bar the user actually sees. */
  height: var(--qss-drawer-bar-h, 41px);
  flex-shrink: 0;
  padding: 0 8px;
}

.qss-drawer-tab {
  border: none;
  background: none;
  /* Symmetric on purpose. `line-height: 1` puts the baseline low enough in
     the box that the ink of a caps-only label already lands centred in the
     pill (6/6.5); nudging the padding to "correct" the empty descender slot
     only pushes the text back down. */
  padding: 5px 10px;
  border-radius: 6px;
  color: var(--qss-text-secondary);
  font: 500 11.5px/1 var(--qss-font-sans);
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.qss-drawer-tab:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qss-drawer-tab.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

/* Waiting approvals must be visible even with the drawer collapsed. */
.qss-drawer-badge {
  display: inline-block;
  margin-left: 5px;
  padding: 1px 6px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--qss-warning) 22%, transparent);
  border: 1px solid color-mix(in srgb, var(--qss-warning) 55%, var(--qss-border));
  color: var(--qss-warning);
  font: 600 9.5px/1.4 var(--qss-font-mono);
}

.qss-drawer-spacer {
  flex: 1;
}

.qss-drawer-toggle {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--qss-text-secondary);
  cursor: pointer;
}
.qss-drawer-toggle:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qss-drawer-toggle svg {
  transition: transform 160ms ease;
}

/* The expanded body floats ABOVE the strip as an overlay sheet — the module
   panel behind it keeps its size, so module UI stays put and gets covered. */
.qss-drawer-body {
  position: absolute;
  bottom: 100%;
  left: 8px;
  right: 8px;
  overflow: auto;
  border-radius: var(--qss-radius-lg, 14px);
  background: var(--qss-bg);
  box-shadow: 0 -10px 30px rgb(0 0 0 / 0.35);
}

@media (prefers-reduced-motion: reduce) {
  .qss-drawer-tab,
  .qss-drawer-toggle svg {
    transition: none;
  }
}
</style>
