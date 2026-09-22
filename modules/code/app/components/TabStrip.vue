<script setup lang="ts">
/**
 * One editor group's tabs, in up to two rows.
 *
 * Pinned files get a row of their own above the working row (user, 2026-08-26).
 * VS Code sorts them to the front of a single strip instead; a row is louder
 * about it — the files you keep are a shelf that never moves, and the row below
 * can churn without pushing them around.
 *
 * The two rows look identical, because they are the same thing. Only the button
 * on the right differs: it unpins above and closes below. The pinned row is
 * absent until something is pinned, so the common case is still one row.
 *
 * ── Dragging: POINTER events, not HTML5 drag-and-drop ──
 *
 * Rewritten on 2026-08-26 after three rounds of HTML5 DnD failing inside
 * WebView2. That API hands the whole gesture to the browser's native drag
 * loop: whether a drag STARTS hinges on text-selection machinery
 * (`-webkit-user-drag`), a DOM mutation mid-gesture can cancel it, whether a
 * drop FIRES depends on every element under the pointer calling
 * `preventDefault` at the right moment, and `dataTransfer` is the only data
 * channel. Each of those broke differently in WebView2 — and none of it can be
 * exercised synthetically, because synthetic `dragstart` events skip the
 * browser's own machinery, so every fix was a guess against a black box.
 *
 * Pointer events keep the gesture in our hands: a press arms a candidate,
 * crossing a small threshold starts the drag, `setPointerCapture` streams
 * every later event to the source tab, and OUR hit-test (`elementFromPoint`
 * against data attributes) decides what the pointer is over. Real mice and
 * synthetic events run the identical path, so what the tests prove is what
 * the user gets.
 *
 * The drag state lives in the STORE: the strip the drag started in receives
 * all events (capture), but the tab may land on ANOTHER group's strip or
 * editor body — and that strip has to draw the insertion line for a drag it
 * does not own.
 */
import { useEditorStore } from '#code-root/stores/editor'
import type { CodeTab } from '#code-root/shared/types'

const props = defineProps<{
  groupId: string
  tabs: CodeTab[]
  activeTabId: string | null
}>()

const store = useEditorStore()

const pinned = computed(() => props.tabs.filter((t) => t.pinned))
const unpinned = computed(() => props.tabs.filter((t) => !t.pinned))

// ── Context menu ──

const menu = ref<{ x: number; y: number; tabId: string } | null>(null)
const menuTab = computed(() => props.tabs.find((t) => t.id === menu.value?.tabId))

function onContextMenu(e: MouseEvent, tabId: string) {
  e.preventDefault()
  menu.value = { x: e.clientX, y: e.clientY, tabId }
}

function closeMenu() {
  menu.value = null
}

onMounted(() => window.addEventListener('click', closeMenu))
onUnmounted(() => {
  window.removeEventListener('click', closeMenu)
  endDrag()
})

/**
 * Middle-click closes. Written out rather than `@mousedown.middle.prevent`,
 * because that chain's guard order decides whether `preventDefault` also runs
 * on a LEFT press — and a prevented press is a drag that never starts.
 */
function onTabMouseDown(e: MouseEvent, tabId: string) {
  if (e.button !== 1) return
  e.preventDefault()
  store.closeTab(props.groupId, tabId)
}

// ── The drag gesture ──

/** Movement below this is a click, not a drag — clicks must stay cheap. */
const DRAG_THRESHOLD = 5

let press: { tabId: string; x: number; y: number; pointerId: number; el: HTMLElement } | null = null
/** A completed drag ends in a click on the source tab (capture makes it so);
 *  that click must not re-activate a tab the drag just moved elsewhere. */
let suppressClick = false

function onTabPointerDown(e: PointerEvent, tabId: string) {
  if (e.button !== 0) return
  // A press on close/unpin presses the button; it never arms a drag.
  if ((e.target as HTMLElement).closest('button')) return
  press = {
    tabId,
    x: e.clientX,
    y: e.clientY,
    pointerId: e.pointerId,
    el: e.currentTarget as HTMLElement,
  }
}

function onTabPointerMove(e: PointerEvent) {
  if (!press) return

  if (!store.dragTabId) {
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_THRESHOLD) return
    store.dragTabId = press.tabId
    try {
      press.el.setPointerCapture(press.pointerId)
    } catch {
      // Synthetic events carry no active pointer to capture — the handlers
      // still receive their directly-dispatched events.
    }
    // The pointer crosses the editor and its text mid-drag; neither may start
    // selecting, and the cursor should say "carrying" everywhere.
    document.body.style.userSelect = 'none'
    document.body.style.cursor = 'grabbing'
  }

  e.preventDefault()
  updateDropTarget(e.clientX, e.clientY)
}

function onTabPointerUp() {
  const dragged = store.dragTabId
  const target = store.dropTarget
  endDrag()
  if (!dragged || !target) return

  suppressClick = true
  // The click (if any) fires between pointerup and this timeout; if the drag
  // unmounted the source tab there is no click, and the flag must not swallow
  // the NEXT unrelated one.
  setTimeout(() => {
    suppressClick = false
  }, 0)

  store.moveTab(dragged, {
    groupId: target.groupId,
    before: target.before,
    pinned: target.row === 'pinned',
  })
}

function onTabPointerCancel() {
  endDrag()
}

function endDrag() {
  press = null
  store.dragTabId = null
  store.dropTarget = null
  document.body.style.userSelect = ''
  document.body.style.cursor = ''
}

function onTabClick(tabId: string) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  store.setActiveTab(props.groupId, tabId)
}

/**
 * Where would the pointer drop? Resolved from the DOM under it, not from event
 * routing: `[data-tab-id]` gives the precise slot (left half before, right
 * half after), `[data-row]` the row — which IS the pinned state — and
 * `[data-group-id]` alone means the group's editor body, appending to its
 * working row. Outside all three there is no target and no line.
 */
function updateDropTarget(x: number, y: number) {
  const el = document.elementFromPoint(x, y) as HTMLElement | null
  const groupEl = el?.closest<HTMLElement>('[data-group-id]')
  if (!groupEl) {
    store.dropTarget = null
    return
  }
  const groupId = groupEl.dataset.groupId!

  const rowEl = el?.closest<HTMLElement>('[data-row]')
  if (!rowEl) {
    store.dropTarget = { groupId, row: 'tabs', before: null }
    return
  }
  const row = rowEl.dataset.row as 'pinned' | 'tabs'

  const tabEl = el?.closest<HTMLElement>('[data-tab-id]')
  if (!tabEl) {
    store.dropTarget = { groupId, row, before: null }
    return
  }

  const box = tabEl.getBoundingClientRect()
  if (x <= box.left + box.width / 2) {
    store.dropTarget = { groupId, row, before: tabEl.dataset.tabId! }
  } else {
    const next = tabEl.nextElementSibling as HTMLElement | null
    // After the last tab the sibling is a marker div with no tab id — null,
    // meaning "the end of the row".
    store.dropTarget = { groupId, row, before: next?.dataset.tabId ?? null }
  }
}

// ── What this strip draws for the (possibly foreign) drag ──

function isDropRow(row: 'pinned' | 'tabs'): boolean {
  const t = store.dropTarget
  return !!t && t.groupId === props.groupId && t.row === row
}

function showsLineBefore(row: 'pinned' | 'tabs', tabId: string): boolean {
  const t = store.dropTarget
  return !!t && t.groupId === props.groupId && t.row === row && t.before === tabId
}

function showsLineAtEnd(row: 'pinned' | 'tabs'): boolean {
  const t = store.dropTarget
  return !!t && t.groupId === props.groupId && t.row === row && t.before === null
}
</script>

<template>
  <div class="strip" @mousedown="store.setActiveGroup(groupId)">
    <!-- The shelf renders only when something IS pinned (user, 2026-08-26):
         an empty shelf that appeared during drags kept catching drops meant
         for the working row, so every drag pinned. Absent row, absent target —
         the hit-test resolves rows from the DOM, so what does not render
         cannot be hit. The FIRST pin happens via the context menu. -->
    <div
      v-if="pinned.length"
      class="row row--pinned"
      data-row="pinned"
      :class="{ 'is-drop-target': isDropRow('pinned') }"
    >
      <div
        v-for="tab in pinned"
        :key="tab.id"
        class="tab tab--pinned"
        :data-tab-id="tab.id"
        :class="{
          'is-active': tab.id === activeTabId,
          'is-dragging': tab.id === store.dragTabId,
          'drop-before': showsLineBefore('pinned', tab.id),
        }"
        :title="tab.path"
        @click="onTabClick(tab.id)"
        @pointerdown="onTabPointerDown($event, tab.id)"
        @pointermove="onTabPointerMove"
        @pointerup="onTabPointerUp"
        @pointercancel="onTabPointerCancel"
        @contextmenu="onContextMenu($event, tab.id)"
        @mousedown="onTabMouseDown($event, tab.id)"
      >
        <span class="tab-name">{{ tab.fileName }}</span>
        <span v-if="store.isDirty(tab)" class="tab-dot" />
        <button class="tab-btn" title="Unpin" @click.stop="store.togglePin(groupId, tab.id)">
          <!-- A pushpin seen head-on: flat head, tapered collar, needle. The old
               mark was a five-pointed shape that read as a star at this size. -->
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="12" y1="16" x2="12" y2="22" />
            <path d="M8 3h8 M10 3v5.2a2 2 0 0 1-.6 1.4l-1.6 1.6a2 2 0 0 0-.6 1.4V14h10.4v-1.4a2 2 0 0 0-.6-1.4l-1.6-1.6a2 2 0 0 1-.6-1.4V3" />
          </svg>
        </button>
      </div>

      <div v-if="showsLineAtEnd('pinned')" class="drop-end" />
    </div>

    <!-- The working row. -->
    <div
      class="row row--tabs"
      data-row="tabs"
      :class="{ 'is-drop-target': isDropRow('tabs') }"
    >
      <div
        v-for="tab in unpinned"
        :key="tab.id"
        class="tab"
        :data-tab-id="tab.id"
        :class="{
          'is-active': tab.id === activeTabId,
          'is-dragging': tab.id === store.dragTabId,
          'drop-before': showsLineBefore('tabs', tab.id),
        }"
        :title="tab.path"
        @click="onTabClick(tab.id)"
        @pointerdown="onTabPointerDown($event, tab.id)"
        @pointermove="onTabPointerMove"
        @pointerup="onTabPointerUp"
        @pointercancel="onTabPointerCancel"
        @contextmenu="onContextMenu($event, tab.id)"
        @mousedown="onTabMouseDown($event, tab.id)"
      >
        <span class="tab-name">{{ tab.fileName }}</span>
        <button
          class="tab-btn tab-close"
          :class="{ 'is-dirty': store.isDirty(tab) }"
          :title="store.isDirty(tab) ? 'Unsaved changes — close' : 'Close'"
          @click.stop="store.closeTab(groupId, tab.id)"
        >
          <span v-if="store.isDirty(tab)" class="tab-dot" />
          <svg v-else width="9" height="9" viewBox="0 0 10 10" fill="none" aria-hidden="true">
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>

      <div v-if="showsLineAtEnd('tabs')" class="drop-end" />
    </div>

    <Teleport to="body">
      <div v-if="menu" class="ctx" :style="{ left: menu.x + 'px', top: menu.y + 'px' }">
        <button class="ctx-item" @click="store.togglePin(groupId, menu!.tabId)">
          {{ menuTab?.pinned ? 'Unpin' : 'Pin to top row' }}
        </button>
        <button class="ctx-item" @click="store.closeTab(groupId, menu!.tabId)">Close</button>
        <button class="ctx-item" @click="store.closeOthers(groupId, menu!.tabId)">
          Close others
          <span class="ctx-note">keeps pinned</span>
        </button>

        <div class="ctx-sep" />

        <!-- Splitting used to be a button on the strip. It moved here (user,
             2026-08-26): it is a rare command, and a permanent cap on every
             group's strip cost more attention than it was worth. -->
        <button class="ctx-item" @click="store.splitGroup({ groupId, tabId: menu!.tabId })">
          Split editor right
          <kbd class="ctx-key">Ctrl+\</kbd>
        </button>
        <button v-if="store.groups.length > 1" class="ctx-item" @click="store.closeGroup(groupId)">
          Close editor group
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.strip {
  position: relative;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  background: var(--qss-bg-raised);
  border-bottom: 1px solid var(--qss-border);
}

/* Both rows are the same thing — tabs — so they share height, ground and type.
   The pinned row was shorter and darker for a day; that made two kinds of tab
   out of one (user, 2026-08-26). The only difference left is what the button on
   the right does: unpin above, close below. */
.row {
  display: flex;
  align-items: stretch;
  height: 34px;
  overflow-x: auto;
  scrollbar-width: none;
}
.row::-webkit-scrollbar {
  display: none;
}

/* A hairline between them, so the shelf still reads as its own row. */
.row--pinned {
  border-bottom: 1px solid var(--qss-border-subtle, var(--qss-border));
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  max-width: 190px;
  padding: 0 8px 0 12px;
  border-right: 1px solid var(--qss-border);
  background: transparent;
  color: var(--qss-text-muted);
  font-size: 12px;
  /* A tab is for clicking; dragging is the secondary gesture, so the resting
     cursor stays the pointer (user, 2026-08-26). `grabbing` appears once a
     drag actually runs — set on the body in `onTabPointerMove`, not here, so
     it holds across the whole window while the tab is being carried. */
  cursor: pointer;
  user-select: none;
  /* The drag is pointer-event driven; without this a touch drag would scroll
     the row instead of moving the tab. */
  touch-action: none;
  transition: background var(--qss-dur-fast, 120ms) ease, color var(--qss-dur-fast, 120ms) ease;
}
.tab:hover {
  color: var(--qss-text-secondary);
  background: var(--qss-bg-hover);
}
.tab.is-active {
  color: var(--qss-text);
  background: var(--qss-bg);
  box-shadow: inset 0 2px 0 var(--qss-accent);
}

.tab--pinned {
  max-width: 150px;
}

/* The row a drop would land in, while a drag is over it. */
.row.is-drop-target {
  background: color-mix(in srgb, var(--qss-accent) 6%, transparent);
}

/* The insertion line — on the tab it would push right, or after the last one.
   A line rather than a gap: a gap reflows the whole row on every pointer move,
   and the eye loses the place it was aiming at. */
.tab.drop-before::before,
.drop-end {
  content: '';
  width: 2px;
  background: var(--qss-accent);
  border-radius: 1px;
}
.tab.drop-before {
  position: relative;
}
.tab.drop-before::before {
  position: absolute;
  left: -1px;
  top: 4px;
  bottom: 4px;
}
.drop-end {
  flex-shrink: 0;
  margin: 4px 0;
}

/* The tab being dragged fades, so the insertion line is what the eye follows. */
.tab.is-dragging {
  opacity: 0.4;
}

.tab-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tab-btn {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.tab-btn:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

/* The pin stays visible on a pinned tab — it is what the row is about, and it
   doubles as the unpin control. */
.tab--pinned .tab-btn {
  opacity: 0.5;
}
.tab--pinned:hover .tab-btn,
.tab--pinned.is-active .tab-btn {
  opacity: 1;
}

.tab-close {
  opacity: 0;
}
.tab:hover .tab-close,
.tab-close.is-dirty {
  opacity: 1;
}

/* The dirty marker doubles as the close button — hovering swaps it for the
   cross, which is where the eye already is. */
.tab-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
}
.tab-close.is-dirty:hover .tab-dot {
  display: none;
}
.tab-close.is-dirty:hover::after {
  content: '\00d7';
  font-size: 12px;
  line-height: 1;
}

.ctx {
  position: fixed;
  z-index: 9999;
  min-width: 178px;
  padding: 4px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: var(--qss-bg-overlay, var(--qss-bg-raised));
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
}
.ctx-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 5px 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.ctx-item:hover {
  background: var(--qss-bg-hover);
}

.ctx-key,
.ctx-note {
  margin-left: auto;
  font-size: 10px;
  color: var(--qss-text-muted);
}
.ctx-key {
  font-family: var(--qss-font-mono);
}

.ctx-sep {
  height: 1px;
  margin: 4px 6px;
  background: var(--qss-border);
}
</style>
