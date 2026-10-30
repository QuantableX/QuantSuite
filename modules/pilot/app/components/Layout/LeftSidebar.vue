<script setup lang="ts">
/**
 * The thread sidebar (card 7, after T3 Code's): a project filter, then
 * Pinned and Active in the user's order, the folded Working section, and
 * the Settled and Snoozed shelves.
 *
 * Dragging is pointer events (never HTML5 drag and drop — it breaks in
 * WebView2): reorder inside Pinned and Active, drop into Pinned to pin (at
 * the top edge when nothing is pinned), onto the Settled header to settle,
 * out of a shelf to un-settle or wake. Nothing re-lays out while dragging:
 * the dragged row's slot empties and the other rows slide (transforms only)
 * to open the gap where it will land; the Pinned / Active boundary is an
 * overlay. The ghost names what the drop does, Escape cancels. The hover
 * action (Settle / Un-settle / Wake) dragged across rows acts on all of
 * them. Files from Explorer use HTML5 drop on a row — the only way OS files
 * come in.
 *
 * Keyboard (T3 Code's "Browse threads"): Tab enters the list once,
 * Up/Down/Home/End move between rows and shelf headers without opening,
 * Enter opens a row or toggles a shelf, Right/Left open and close a shelf,
 * Tab goes on to the row's buttons, F2 renames, Shift+F10 or the menu key
 * opens the row's menu.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Archive, ArchiveRestore, ChevronDown, ChevronRight, Pin, PinOff, Plus, Sunrise } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import {
  arrangementFor,
  canDrop,
  dropAction,
  type DropAction,
  type DropZone,
  type Section,
  type SidebarThread,
} from '#pilot/utils/sidebar'
import type { SessionMeta } from '#pilot/types'
import { useKeysStore } from '#pilot/stores/keys'

const store = usePilotStore()
const threads = useThreadStore()
const sb = useSidebarStore()
const keys = useKeysStore()

const secs = computed(() => sb.sections)
const metaById = computed(() => {
  const m = new Map<string, SessionMeta>()
  for (const s of store.sessions) m.set(s.id, s)
  return m
})
const workingOn = computed(() => store.settings?.workingSection ?? true)
function projectOf(t: SidebarThread): string | null {
  if (sb.projectFilter) return null
  const ctx = store.contexts.find((c) => c.id === t.contextId)
  if (ctx) return ctx.kind === 'general' ? 'No project' : ctx.name
  return metaById.value.get(t.id)?.contextName || 'Removed workspace'
}

// ── Project filter and New ──
/** Every workspace, General as "No project", and workspaces that left the registry but still have threads. */
const filterOptions = computed(() => {
  const out = store.contexts.map((c) => ({ id: c.id, name: c.kind === 'general' ? 'No project' : c.name }))
  const known = new Set(out.map((o) => o.id))
  for (const s of store.sessions) {
    if (known.has(s.contextId)) continue
    known.add(s.contextId)
    out.push({ id: s.contextId, name: s.contextName || 'Removed workspace' })
  }
  return out
})
const filterModel = computed({
  get: () => sb.projectFilter ?? '',
  set: (v: string) => sb.setFilter(v || null),
})
const newMenu = ref(false)
async function newThread(contextId?: string) {
  newMenu.value = false
  await sb.newThread(contextId ? { contextId } : {})
}

// ── Menu ──
const menu = ref<{ id: string; x: number; y: number } | null>(null)
const menuThread = computed(() => (menu.value ? metaById.value.get(menu.value.id) ?? null : null))
function openMenu(id: string, x: number, y: number) {
  menu.value = { id, x, y }
}
function closeMenu() {
  const id = menu.value?.id
  menu.value = null
  if (id) void nextTick(() => rowEl(id)?.focus())
}
function renameFromMenu() {
  if (menu.value) sb.renaming = menu.value.id
  menu.value = null
}

// ── Keyboard ──
const list = ref<HTMLElement | null>(null)
const focusId = ref<string | null>(null)
const tabStop = computed(() => {
  const ids = sb.visible.map((r) => r.id)
  if (focusId.value && ids.includes(focusId.value)) return focusId.value
  if (store.activeId && ids.includes(store.activeId)) return store.activeId
  return ids[0] ?? null
})
function rowEl(id: string): HTMLElement | null {
  return list.value?.querySelector<HTMLElement>(`[data-thread="${CSS.escape(id)}"]`) ?? null
}
function navItems(): HTMLElement[] {
  return [...(list.value?.querySelectorAll<HTMLElement>('[data-nav]') ?? [])]
}
function focusItem(el: HTMLElement | undefined) {
  if (!el) return
  if (el.dataset.thread) focusId.value = el.dataset.thread
  el.focus()
  el.scrollIntoView({ block: 'nearest' })
}
function onListKey(e: KeyboardEvent) {
  const target = e.target as HTMLElement
  if (target.closest('input, textarea, select')) return
  const item = target.closest<HTMLElement>('[data-nav]')
  // Tab and Enter on the row's own buttons stay theirs.
  if (item && target !== item && target.tagName === 'BUTTON' && !target.dataset.nav) return
  const items = navItems()
  const at = item ? items.indexOf(item) : -1
  const row = item?.dataset.thread ?? null
  const shelf = item?.dataset.shelf as 'working' | 'settled' | 'snoozed' | undefined
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    focusItem(items[e.key === 'ArrowDown' ? Math.min(items.length - 1, at + 1) : Math.max(0, at - 1)])
  } else if (e.key === 'Home' || e.key === 'End') {
    e.preventDefault()
    focusItem(e.key === 'Home' ? items[0] : items[items.length - 1])
  } else if (shelf && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
    e.preventDefault()
    sb.toggleShelf(shelf, e.key === 'ArrowRight')
  } else if (e.key === 'Enter' && row) {
    e.preventDefault()
    void store.openSession(row)
  } else if (e.key === 'F2' && row) {
    e.preventDefault()
    sb.renaming = row
  } else if (((e.key === 'F10' && e.shiftKey) || e.key === 'ContextMenu') && row) {
    e.preventDefault()
    const r = item!.getBoundingClientRect()
    openMenu(row, r.left + 24, r.bottom)
  }
}

// ── Files from Explorer ──
async function onFiles(id: string, files: File[]) {
  await store.openSession(id)
  threads.dropFiles(id, files)
}

// ── Dragging ──
interface Slot {
  id: string
  /** Middle of the row in list content coordinates, as if the dragged row were gone. */
  mid: number
}
interface Drag {
  id: string
  from: Section
  el: HTMLElement
  startX: number
  startY: number
  started: boolean
  x: number
  y: number
  offsetX: number
  offsetY: number
  width: number
  rowH: number
  zone: DropZone | null
  slots: { pinned: Slot[]; active: Slot[] }
  /** Content y between Pinned and Active as if the dragged row were gone (hit testing). */
  boundary: number
  /** The same with the dragged row still in its slot (no drop spot yet). */
  restBoundary: number
  /** Where the boundary overlay is drawn now: it moves with the gap. */
  shownBoundary: number
  noPins: boolean
  activeEnd: number
  settledHead: { top: number; bottom: number } | null
  /** The list's children in order, for the slide transforms. */
  kids: HTMLElement[]
}
const drag = ref<Drag | null>(null)
let scrollRaf = 0

interface Range {
  section: Section
  anchor: string
  current: string
}
const range = ref<Range | null>(null)

const dragging = computed(() => Boolean(drag.value?.started))
const dragAction = computed<DropAction | null>(() => {
  const d = drag.value
  if (!d?.started || !d.zone || !canDrop(d.from, d.zone, workingOn.value)) return null
  return dropAction(d.from, d.zone)
})
const dropValid = computed(() => {
  const d = drag.value
  return Boolean(d?.started && d.zone && canDrop(d.from, d.zone, workingOn.value))
})
const dragMeta = computed(() => (drag.value ? metaById.value.get(drag.value.id) ?? null : null))
const overSection = computed(() => (dropValid.value ? drag.value?.zone?.section ?? null : null))
/** Where the Pinned / Active boundary overlay sits (content y). */
const boundaryTop = computed(() => (drag.value?.started ? drag.value.shownBoundary : 0))

const rangeIds = computed(() => {
  const r = range.value
  if (!r) return new Set<string>()
  const ids = (secs.value[r.section] ?? []).map((t) => t.id)
  const a = ids.indexOf(r.anchor)
  const b = ids.indexOf(r.current)
  if (a < 0 || b < 0) return new Set([r.anchor])
  return new Set(ids.slice(Math.min(a, b), Math.max(a, b) + 1))
})

function contentY(clientY: number): number {
  const el = list.value
  if (!el) return clientY
  return clientY - el.getBoundingClientRect().top + el.scrollTop
}

function measure(d: Drag) {
  const el = list.value
  if (!el) return
  const top = el.getBoundingClientRect().top - el.scrollTop
  const kids = [...el.children].filter((k): k is HTMLElement => k instanceof HTMLElement && !k.classList.contains('qp-overlay'))
  const pDrag = kids.indexOf(d.el)
  const slots: Drag['slots'] = { pinned: [], active: [] }
  // Rows below the dragged one move up into its slot once the gap moves:
  // "virtual" positions are the list without the dragged row.
  const lift = (p: number) => (pDrag >= 0 && p > pDrag ? d.rowH : 0)
  let restFirstActive: number | null = null
  let restLastPinned: number | null = null
  let firstActive: number | null = null
  let lastPinned: number | null = null
  kids.forEach((k, p) => {
    const sec = k.dataset.section
    if (sec !== 'pinned' && sec !== 'active') return
    const r = k.getBoundingClientRect()
    if (sec === 'active' && restFirstActive === null) restFirstActive = r.top - top
    if (sec === 'pinned') restLastPinned = r.bottom - top
    if (k === d.el) return
    if (sec === 'active' && firstActive === null) firstActive = r.top - top - lift(p)
    if (sec === 'pinned') lastPinned = r.bottom - top - lift(p)
    slots[sec].push({ id: k.dataset.thread!, mid: r.top - top + r.height / 2 - lift(p) })
  })
  d.kids = kids
  d.slots = slots
  d.noPins = slots.pinned.length === 0
  const end = el.querySelector<HTMLElement>('[data-zone-end]')
  const endAt = end ? kids.indexOf(end) : -1
  d.activeEnd = end ? end.getBoundingClientRect().top - top - lift(endAt) : el.scrollHeight
  d.boundary = firstActive ?? lastPinned ?? d.activeEnd
  d.restBoundary = restFirstActive ?? restLastPinned ?? d.boundary
  d.shownBoundary = d.restBoundary
  const head = el.querySelector<HTMLElement>('[data-zone="settled"]')
  if (head) {
    const r = head.getBoundingClientRect()
    d.settledHead = { top: r.top - top, bottom: r.bottom - top }
  } else {
    d.settledHead = null
  }
}

/** With no pins, the top edge of the list pins (T3 Code). */
const PIN_EDGE = 10

function zoneAt(d: Drag, clientY: number): DropZone | null {
  const y = contentY(clientY)
  if (d.settledHead && y >= d.settledHead.top - 4 && y <= d.settledHead.bottom + 4) return { section: 'settled' }
  if (d.noPins ? y < d.boundary + PIN_EDGE : y < d.boundary) {
    return { section: 'pinned', index: d.slots.pinned.filter((s) => s.mid < y).length }
  }
  if (y < d.activeEnd + d.rowH / 2) return { section: 'active', index: d.slots.active.filter((s) => s.mid < y).length }
  return null
}

/**
 * Slide the rows: everything below the dragged row closes its slot, and
 * everything from the drop spot down opens one. With no valid spot the
 * slot stays where the row came from.
 */
function slide(d: Drag) {
  const kids = d.kids
  const pDrag = kids.indexOf(d.el)
  let pInsert = -1
  const zone = d.zone
  if (zone && zone.section !== 'settled' && canDrop(d.from, zone, workingOn.value)) {
    const ids = d.slots[zone.section].map((s) => s.id)
    const at = (id: string) => kids.findIndex((k) => k.dataset.thread === id)
    if (zone.index < ids.length) {
      pInsert = at(ids[zone.index])
    } else if (ids.length) {
      pInsert = at(ids[ids.length - 1]) + 1
      if (pInsert === pDrag) pInsert++
    } else if (zone.section === 'pinned') {
      pInsert = d.slots.active.length ? at(d.slots.active[0].id) : kids.findIndex((k) => 'zoneEnd' in k.dataset)
    } else {
      pInsert = kids.findIndex((k) => 'zoneEnd' in k.dataset)
    }
  }
  // Above the gap when it opens at the top of Active, below it when it is in Pinned.
  d.shownBoundary = pInsert < 0 ? d.restBoundary : d.boundary + (zone?.section === 'pinned' ? d.rowH : 0)
  kids.forEach((k, p) => {
    if (k === d.el) return
    let shift = 0
    if (pInsert >= 0) {
      if (pDrag >= 0 && p > pDrag) shift -= d.rowH
      if (p >= pInsert) shift += d.rowH
    }
    k.style.transform = shift ? `translateY(${shift}px)` : ''
  })
}

function unslide(d: Drag) {
  const el = list.value
  el?.classList.add('no-anim')
  for (const k of d.kids) k.style.transform = ''
  // The drop lands without a second transition (T3 Code).
  requestAnimationFrame(() => requestAnimationFrame(() => el?.classList.remove('no-anim')))
}

function sectionOfRow(id: string): Section | null {
  for (const s of ['pinned', 'active', 'working', 'settled', 'snoozed'] as Section[]) {
    if (secs.value[s].some((r) => r.id === id)) return s
  }
  return null
}

function onRowPointerDown(id: string, e: PointerEvent) {
  if (drag.value || range.value) return
  const from = sectionOfRow(id)
  const el = e.currentTarget as HTMLElement | null
  if (!from || !el) return
  const r = el.getBoundingClientRect()
  drag.value = {
    id,
    from,
    el,
    startX: e.clientX,
    startY: e.clientY,
    started: false,
    x: e.clientX,
    y: e.clientY,
    offsetX: e.clientX - r.left,
    offsetY: e.clientY - r.top,
    width: r.width,
    rowH: r.height,
    zone: null,
    slots: { pinned: [], active: [] },
    boundary: 0,
    restBoundary: 0,
    shownBoundary: 0,
    noPins: true,
    activeEnd: 0,
    settledHead: null,
    kids: [],
  }
  arm()
}

function onActionPointerDown(id: string, e: PointerEvent) {
  if (drag.value || range.value || e.button !== 0) return
  const section = sectionOfRow(id)
  if (!section) return
  range.value = { section, anchor: id, current: id }
  arm()
}

function arm() {
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', cancel)
  window.addEventListener('keydown', onDragKey, true)
}
function disarm() {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', cancel)
  window.removeEventListener('keydown', onDragKey, true)
  cancelAnimationFrame(scrollRaf)
  scrollRaf = 0
}

function onMove(e: PointerEvent) {
  const r = range.value
  if (r) {
    const row = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>('[data-thread]')
    if (row?.dataset.section === r.section && row.dataset.thread) r.current = row.dataset.thread
    return
  }
  const d = drag.value
  if (!d) return
  d.x = e.clientX
  d.y = e.clientY
  if (!d.started) {
    if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < 5) return
    d.started = true
    menu.value = null
    // The Settled header joins as a drop target below the rows — measure after it rendered.
    void nextTick(() => {
      if (drag.value !== d) return
      measure(d)
      update(d)
      autoScroll()
    })
    return
  }
  if (d.kids.length) update(d)
}

function update(d: Drag) {
  const next = zoneAt(d, d.y)
  const same =
    next?.section === d.zone?.section && (next && 'index' in next ? next.index : -1) === (d.zone && 'index' in d.zone ? d.zone.index : -1)
  if (same && d.kids.length) return
  d.zone = next
  slide(d)
}

/** Near the list's edges the list scrolls while dragging. */
function autoScroll() {
  const d = drag.value
  const el = list.value
  if (!d?.started || !el) return
  const r = el.getBoundingClientRect()
  const edge = 28
  const step = d.y < r.top + edge ? -8 : d.y > r.bottom - edge ? 8 : 0
  if (step) {
    el.scrollTop += step
    update(d)
  }
  scrollRaf = requestAnimationFrame(autoScroll)
}

function swallowClick(e: MouseEvent) {
  e.stopPropagation()
  e.preventDefault()
}
function suppressNextClick() {
  window.addEventListener('click', swallowClick, { capture: true, once: true })
  setTimeout(() => window.removeEventListener('click', swallowClick, { capture: true }), 0)
}

async function onUp() {
  const r = range.value
  if (r) {
    const ids = [...rangeIds.value]
    disarm()
    range.value = null
    if (r.section === 'settled') await sb.arrangeMany(ids, { settled: false })
    else if (r.section === 'snoozed') await sb.arrangeMany(ids, { snoozedUntil: null })
    else await sb.arrangeMany(ids, { settled: true }, ids.length > 1 ? `Settled ${ids.length} threads` : 'Settled')
    return
  }
  const d = drag.value
  disarm()
  drag.value = null
  if (!d?.started) return
  suppressNextClick()
  unslide(d)
  if (!d.zone || !canDrop(d.from, d.zone, workingOn.value)) return
  const row = sb.snapshot(d.id)
  if (!row) return
  const zone = d.zone
  const target = zone.section === 'settled' ? [] : secs.value[zone.section].filter((t) => t.id !== d.id)
  if (zone.section === d.from && zone.section !== 'settled') {
    const old = secs.value[d.from].findIndex((t) => t.id === d.id)
    if (old === zone.index) return
  }
  const action = dropAction(d.from, zone)
  const label = action === 'Unpin' ? 'Unpinned' : action === 'Settle' ? 'Settled' : undefined
  await sb.arrange(d.id, arrangementFor(row, d.from, zone, target), label)
}

function cancel() {
  const d = drag.value
  disarm()
  drag.value = null
  range.value = null
  if (d?.started) {
    unslide(d)
    suppressNextClick()
  }
}

function onDragKey(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  e.preventDefault()
  e.stopPropagation()
  cancel()
}

// ── Shelves ──
// A shelf that empties closes, so the next thread to land there does not pop it open.
watch(
  () => secs.value.settled.length,
  (n) => {
    if (!n && sb.shelves.settled) sb.toggleShelf('settled', false)
  }
)

// ── Clock: "3m ago" and wake times move on; a woken thread comes back ──
let clock: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  clock = setInterval(() => sb.tick(), 30_000)
})
onBeforeUnmount(() => {
  if (clock) clearInterval(clock)
  disarm()
})

function rowProps(t: SidebarThread, section: Section) {
  const meta = metaById.value.get(t.id)!
  return {
    thread: meta,
    section,
    status: t.status,
    pending: threads.pending(t.id).length,
    unread: Boolean(sb.unread[t.id]),
    active: store.activeId === t.id,
    focused: tabStop.value === t.id,
    renaming: sb.renaming === t.id,
    regenerating: Boolean(sb.regenerating[t.id]),
    project: projectOf(t),
    providerLabel: store.adapterLabel(meta.provider || 'claude'),
    source: drag.value?.started === true && drag.value.id === t.id,
    inRange: rangeIds.value.has(t.id),
    now: sb.clock,
    'data-section': section,
  }
}
const SECTIONS: Section[] = ['pinned', 'active']
const SHELVES = [
  { id: 'working', label: 'Working' },
  { id: 'settled', label: 'Settled' },
  { id: 'snoozed', label: 'Snoozed' },
] as const
function shelfShown(id: 'working' | 'settled' | 'snoozed'): boolean {
  if (secs.value[id].length) return true
  return id === 'settled' && dragging.value && drag.value?.from !== 'settled'
}
const badgeIcon = computed(() => {
  switch (dragAction.value) {
    case 'Pin':
      return Pin
    case 'Unpin':
      return PinOff
    case 'Settle':
      return Archive
    case 'Un-settle':
      return ArchiveRestore
    case 'Wake':
      return Sunrise
    default:
      return null
  }
})
const empty = computed(() => !store.sessions.some((s) => !sb.hidden[s.id] && (!sb.projectFilter || s.contextId === sb.projectFilter)))
</script>

<template>
  <div class="qp-side" :class="{ dragging }">
    <div class="qp-side-head">
      <select v-model="filterModel" class="qp-filter" aria-label="Project">
        <option value="">All projects</option>
        <option v-for="o in filterOptions" :key="o.id" :value="o.id">{{ o.name }}</option>
      </select>
      <div class="qp-new">
        <button class="qp-new-btn" :title="`${keys.hint('chat.new', 'New thread')} — ${keys.hint('chat.newWithoutProject', 'without a project')}`" @click="newThread()">
          <Plus :size="14" />
        </button>
        <button class="qp-new-more" title="New thread in…" aria-haspopup="menu" :aria-expanded="newMenu" @click="newMenu = !newMenu">
          <ChevronDown :size="12" />
        </button>
        <div v-if="newMenu" class="qp-new-menu" role="menu" @keydown.esc.stop="newMenu = false">
          <div class="qp-new-menu-title">New thread in</div>
          <button v-for="o in filterOptions" :key="o.id" class="qp-new-menu-row" role="menuitem" @click="newThread(o.id)">{{ o.name }}</button>
        </div>
      </div>
    </div>
    <div v-if="newMenu" class="qp-scrim" @pointerdown="newMenu = false" />

    <div ref="list" class="qp-side-list" role="listbox" aria-label="Threads" @keydown="onListKey">
      <!-- The Pinned / Active boundary: an overlay while dragging, so no row moves. -->
      <div v-if="dragging" class="qp-overlay qp-boundary" :style="{ top: `${boundaryTop}px` }" aria-hidden="true">
        <span class="qp-boundary-tag up" :class="{ over: overSection === 'pinned' }">
          <Pin :size="10" /> Pinned{{ secs.pinned.length ? '' : ' — drop at the top edge' }}
        </span>
        <span class="qp-boundary-line" />
        <span class="qp-boundary-tag down" :class="{ over: overSection === 'active' }">Active</span>
      </div>

      <template v-for="section in SECTIONS" :key="section">
        <PilotLayoutThreadRow
          v-for="t in secs[section]"
          :key="t.id"
          v-bind="rowProps(t, section)"
          @open="store.openSession(t.id)"
          @menu="(x: number, y: number) => openMenu(t.id, x, y)"
          @rename="(title: string) => sb.rename(t.id, title)"
          @rename-cancel="sb.renaming = null"
          @drag-start="(e: PointerEvent) => onRowPointerDown(t.id, e)"
          @action-start="(e: PointerEvent) => onActionPointerDown(t.id, e)"
          @files="(f: File[]) => onFiles(t.id, f)"
          @focus="focusId = t.id"
        />
      </template>
      <div v-if="empty" class="qp-side-empty">
        <p>No threads{{ sb.projectFilter ? ' in this project' : '' }} yet.</p>
        <button class="qp-side-empty-btn" @click="newThread()">New thread</button>
        <p class="qp-side-empty-hint">Ctrl+N</p>
      </div>
      <div data-zone-end />

      <template v-for="shelf in SHELVES" :key="shelf.id">
        <template v-if="shelfShown(shelf.id)">
          <button
            class="qp-shelf"
            data-nav
            :data-shelf="shelf.id"
            :data-zone="shelf.id"
            tabindex="-1"
            :class="{ over: shelf.id === 'settled' && overSection === 'settled' }"
            :aria-expanded="sb.shelves[shelf.id]"
            @click="sb.toggleShelf(shelf.id)"
          >
            <component :is="sb.shelves[shelf.id] ? ChevronDown : ChevronRight" :size="12" />
            <span>{{ shelf.label }}</span>
            <span v-if="shelf.id === 'settled' && overSection === 'settled'" class="qp-sec-hint">drop to settle</span>
            <span class="qp-shelf-count">{{ secs[shelf.id].length }}</span>
          </button>
          <template v-if="sb.shelves[shelf.id]">
            <PilotLayoutThreadRow
              v-for="t in secs[shelf.id]"
              :key="t.id"
              v-bind="rowProps(t, shelf.id)"
              @open="store.openSession(t.id)"
              @menu="(x: number, y: number) => openMenu(t.id, x, y)"
              @rename="(title: string) => sb.rename(t.id, title)"
              @rename-cancel="sb.renaming = null"
              @drag-start="(e: PointerEvent) => onRowPointerDown(t.id, e)"
              @action-start="(e: PointerEvent) => onActionPointerDown(t.id, e)"
              @files="(f: File[]) => onFiles(t.id, f)"
              @focus="focusId = t.id"
            />
          </template>
        </template>
      </template>
    </div>

    <div v-if="sb.error || store.lastError" class="qp-side-error" role="alert">
      <span>{{ sb.error || store.lastError }}</span>
      <button class="qp-side-error-x" aria-label="Dismiss" @click="sb.error = ''; store.lastError = ''">×</button>
    </div>

    <PilotLayoutThreadMenu v-if="menu && menuThread" :thread="menuThread" :x="menu.x" :y="menu.y" @close="closeMenu" @rename="renameFromMenu" />

    <Teleport to="body">
      <div
        v-if="drag?.started && dragMeta"
        class="qp-ghost"
        :class="{ invalid: !dropValid }"
        :style="{ left: `${drag.x - drag.offsetX}px`, top: `${drag.y - drag.offsetY}px`, width: `${drag.width}px` }"
      >
        <span class="qp-ghost-title">{{ dragMeta.title || 'New thread' }}</span>
        <span v-if="dragAction" class="qp-ghost-badge">
          <component :is="badgeIcon" v-if="badgeIcon" :size="11" />
          {{ dragAction }}
        </span>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.qp-side { position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0; }
.qp-side.dragging { cursor: grabbing; }
.qp-side.dragging :deep(.qp-row) { cursor: grabbing; }
.qp-side.dragging :deep(.qp-row:hover) { background: transparent; }
.qp-side.dragging :deep(.qp-row.active) { background: var(--qss-bg-card, #292930); }
.qp-side.dragging :deep(.qp-row-end) { visibility: hidden; }
.qp-side-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 8px 8px 10px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-filter {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  font-weight: 600;
  padding: 3px 4px;
  outline: none;
  cursor: pointer;
}
.qp-filter:hover,
.qp-filter:focus-visible { border-color: var(--qss-border, #47474f); }
.qp-filter option { background: var(--qss-bg-raised, #1f1f25); font-weight: 400; }
.qp-new { position: relative; display: flex; align-items: center; }
.qp-new-btn,
.qp-new-more {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 24px;
  padding: 0;
  border: 1px solid var(--qss-border, #47474f);
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  cursor: pointer;
}
.qp-new-btn { width: 26px; border-radius: 6px 0 0 6px; }
.qp-new-more { width: 16px; border-left: none; border-radius: 0 6px 6px 0; color: var(--qss-text-muted, #6e6e7a); }
.qp-new-btn:hover,
.qp-new-more:hover { background: var(--qss-bg-hover, #313139); }
.qp-new-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 20;
  min-width: 180px;
  padding: 4px;
  background: var(--qss-bg-raised, #1f1f25);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
}
.qp-new-menu-title { padding: 4px 8px 3px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-new-menu-row {
  display: block;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-new-menu-row:hover,
.qp-new-menu-row:focus-visible { background: var(--qss-bg-hover, #313139); outline: none; }
.qp-scrim { position: absolute; inset: 0; z-index: 10; }
.qp-side-list { position: relative; flex: 1; min-height: 0; overflow-y: auto; padding: 6px 0 10px; outline: none; }
.qp-side-list > * { transition: transform 140ms ease; }
.qp-side-list.no-anim > *,
.qp-side-list.no-anim :deep(.qp-row) { transition: none; }
@media (prefers-reduced-motion: reduce) {
  .qp-side-list > * { transition: none; }
}
.qp-boundary {
  position: absolute;
  left: 8px;
  right: 8px;
  z-index: 5;
  height: 0;
  pointer-events: none;
}
.qp-boundary-line { position: absolute; left: 0; right: 0; top: 0; border-top: 1px dashed var(--qss-border, #47474f); }
.qp-boundary-tag {
  position: absolute;
  right: 0;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
  border-radius: 4px;
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text-muted, #6e6e7a);
  font-size: 10.5px;
  line-height: 15px;
  white-space: nowrap;
}
.qp-boundary-tag.up { bottom: 1px; }
.qp-boundary-tag.down { top: 1px; }
.qp-boundary-tag.over { color: var(--qss-accent, #7c8cff); }
.qp-sec-hint { font-size: 10.5px; opacity: 0.8; }
.qp-shelf {
  display: flex;
  align-items: center;
  gap: 5px;
  width: calc(100% - 12px);
  margin: 6px 6px 1px;
  padding: 4px 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11.5px;
  text-align: left;
  cursor: pointer;
  outline: none;
}
.qp-shelf:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text-secondary, #9a9aa5); }
.qp-shelf:focus-visible { box-shadow: inset 0 0 0 1px var(--qss-accent, #7c8cff); }
.qp-shelf.over { color: var(--qss-accent, #7c8cff); box-shadow: inset 0 0 0 1px var(--qss-accent, #7c8cff); }
.qp-shelf-count { margin-left: auto; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; }
.qp-side-empty { padding: 18px 14px; text-align: center; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-side-empty p { margin: 0 0 8px; }
.qp-side-empty-hint { font-size: 11px; }
.qp-side-empty-btn {
  padding: 4px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-side-empty-btn:hover { background: var(--qss-bg-hover, #313139); }
.qp-side-error {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0 8px 8px;
  padding: 6px 8px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 14%, transparent);
  color: var(--qss-warning, #d29a3f);
  font-size: 11.5px;
  line-height: 1.4;
}
.qp-side-error span { flex: 1; overflow-wrap: anywhere; }
.qp-side-error-x { border: none; background: transparent; color: inherit; font-size: 14px; line-height: 1; cursor: pointer; padding: 0; }
.qp-ghost {
  position: fixed;
  z-index: 1001;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: 6px;
  background: var(--qss-bg-card, #292930);
  border: 1px solid var(--qss-border, #47474f);
  box-shadow: 0 8px 22px rgba(0, 0, 0, 0.45);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  pointer-events: none;
}
.qp-ghost.invalid { opacity: 0.6; }
.qp-ghost-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ghost-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  padding: 1px 7px;
  border-radius: 9px;
  background: var(--qss-accent, #7c8cff);
  color: var(--qss-bg, #18181e);
  font-size: 11px;
  font-weight: 600;
}
</style>
