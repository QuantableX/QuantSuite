<script setup lang="ts">
/**
 * The window groups — `ConsoleGroups`, the left sidebar's content and, since the
 * horizontal tab strip is gone, the module's *only* session navigation.
 *
 * Shape follows Warp's sidebar, because the shape is the point: a collapsible
 * heading per window, and under it one row per pane — a status badge on the pane
 * mark, what the pane is doing on the first line, where it is doing it on the
 * second. A single line per pane cannot carry both, and "PowerShell" four times
 * over is not navigation. A search field sits on top for the case this list is
 * thirty rows long, which on a working machine it is.
 *
 * Paths are clipped at the *front*, never the back: `…heckouts\Bitzer_WebWaage`
 * identifies a directory, `C:\Checkouts\Bit…` identifies a drive.
 *
 * Everything the list can do is reachable three ways — pointer, row menu and
 * keyboard — because a sidebar that is the only navigation cannot have a mouse
 * requirement anywhere in it.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { qs } from '@quantsuite/core'
import type { ConsoleGroup, ConsoleTab, PaneSlot } from '../stores/console'
import type { ConsoleTabView } from '../types/multiplex'
import { useAnsiPalette } from '../composables/useAnsiPalette'

const props = defineProps<{
  tabs: ConsoleTab[]
  /** Titles as the store derives them, so nothing here re-invents a name. */
  views: ConsoleTabView[]
  /** Slots per tab key, resolved by the page — the tree does not read the store. */
  panesByTab: Record<string, PaneSlot[]>
  activeTabKey: string | null
  /** A new group can be opened (false when no shell was detected). */
  canCreate?: boolean
  /**
   * Shells this machine actually has.
   *
   * The `+` opens them as a menu rather than always spawning the default: a
   * terminal that can only ever start one interpreter is not a terminal, and
   * the list is short enough that a submenu would be ceremony.
   */
  shells?: { id: string; label: string; isDefault?: boolean }[]
  /**
   * Current shortcut text per action id, the same map `Pane.vue` takes.
   *
   * Passed in rather than hard-coded: the keymap is user-editable (P7), and a
   * tooltip that promises `Ctrl+T` for a binding the user — or the next release —
   * moved elsewhere is worse than a tooltip with no key in it at all. Missing
   * entries print nothing.
   */
  keys?: Record<string, string>
  /**
   * Panes with output since they were last focused, for the unread dot.
   *
   * Optional, and absent means *unknown*, not "nothing unread" — without the
   * prop no dots are drawn at all, rather than a row of permanently quiet ones
   * claiming the sidebar has seen everything.
   */
  unread?: string[]
  /**
   * Per-tab colour as an ANSI index (1–6), `null` for none.
   *
   * The colour row in the tab menu appears only when this prop is bound: the
   * store has no colour of its own, and a swatch that paints nothing is the kind
   * of menu entry this sidebar exists to be rid of.
   */
  tabColors?: Record<string, number | null>
  /**
   * The named folders of the top level (`store.groups`), in order. A terminal
   * group whose `groupKey` names one of these renders under its heading; the
   * rest render ungrouped, exactly as before folders existed.
   */
  groupList?: ConsoleGroup[]
}>()

const emit = defineEmits<{
  (e: 'selectTab', key: string): void
  (e: 'closeTab', key: string): void
  (e: 'renameTab', payload: { key: string; title: string }): void
  (e: 'focusPane', paneKey: string): void
  (e: 'closePane', paneKey: string): void
  /** `shellId` opens that shell; without one, the configured default. */
  (e: 'create', shellId?: string): void
  /** Indices into `tabs`, for `reorderTabs` — drag, or Alt+Arrow on a heading. */
  (e: 'reorderTab', payload: { from: number; to: number }): void
  /** A terminal group filed under a folder (`null` = ungrouped). It stays its
   * own split unit — filing never merges anything. */
  (e: 'moveTabToGroup', payload: { tabKey: string; groupKey: string | null }): void
  /** A terminal group dropped on the new-group zone: found a folder around it. */
  (e: 'createGroupWith', tabKey: string): void
  (e: 'renameGroup', payload: { key: string; title: string }): void
  /** Dissolve a folder; its terminal groups survive as ungrouped. */
  (e: 'deleteGroup', key: string): void
  /** Indices into `groupList` — one folder heading dropped on another. */
  (e: 'reorderGroup', payload: { from: number; to: number }): void
  (e: 'setTabColor', payload: { key: string; color: number | null }): void
}>()

const SEARCH_ICON = ['M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14', 'M16.2 16.2L20 20']
const CREATE_ICON = ['M12 5v14', 'M5 12h14']
const SLIDERS_ICON = ['M4 7.5h8', 'M16 7.5h4', 'M4 16.5h4', 'M12 16.5h8', 'M14 4.5v6', 'M10 13.5v6']
const CHEVRON_ICON = ['M9.5 6.5L15 12l-5.5 5.5']
const KEBAB_ICON = ['M12 6.5v.01', 'M12 12v.01', 'M12 17.5v.01']
const CLOSE_ICON = ['M6.5 6.5l11 11', 'M17.5 6.5l-11 11']
const NO_COLOUR_ICON = ['M5.5 18.5L18.5 5.5']
/** The prompt mark, as in the pane title bar: one object, one icon. */
const PANE_ICON = ['M7 8.5l3.5 3.5L7 15.5', 'M13.5 15.5h4']
const SUITE_ICON = ['M4 6h16', 'M4 12h16', 'M4 18h10']

/**
 * The badge vocabulary, drawn over the pane mark's bottom-right corner.
 *
 * Five states and no more, each one something the slot actually reports. Warp
 * also badges "blocked — waiting on the user"; nothing downstream can tell a
 * shell waiting at a password prompt from one waiting at an idle prompt, so that
 * badge is not in this table rather than being guessed at.
 */
const BADGES = {
  exited: { paths: ['M7 7h10v10H7z'], tone: 'off' },
  idle: { paths: ['M6 12.5l4 4 8-8.5'], tone: 'ok' },
} as const

type BadgeId = keyof typeof BADGES

/** ANSI 1–6 is the suite's red/green/yellow/blue/magenta/cyan, themed and overridable. */
const SWATCHES = [
  { index: 1, name: 'Red' },
  { index: 2, name: 'Green' },
  { index: 3, name: 'Yellow' },
  { index: 4, name: 'Blue' },
  { index: 5, name: 'Magenta' },
  { index: 6, name: 'Cyan' },
]

const ansi = useAnsiPalette()

const filter = ref('')
const root = ref<HTMLElement | null>(null)

// ── display settings ─────────────────────────────────────────────────────────
//
// Warp's sliders popup, minus the rows we have no data for: there is no branch
// to put in a title and no per-pane git state to put in a subtitle, so the menu
// offers what the slot reports and nothing else.

type Display = {
  view: 'panes' | 'tabs'
  density: 'compact' | 'expanded'
  title: 'command' | 'directory'
  subtitle: 'path' | 'shell'
}

const DISPLAY_DEFAULT: Display = {
  view: 'panes',
  density: 'compact',
  title: 'command',
  subtitle: 'path',
}

const display = ref<Display>({ ...DISPLAY_DEFAULT })

function setDisplay<K extends keyof Display>(key: K, value: Display[K]) {
  display.value = { ...display.value, [key]: value }
  // Fire and forget: the view has already changed, and a settings backend that is
  // not there (plain `nuxt dev`) must not make the popup feel broken.
  void qs.core.setSetting('console', 'sidebar.display', display.value).catch(() => {})
}

onMounted(async () => {
  try {
    const stored = await qs.core.getSetting<Partial<Display>>('console', 'sidebar.display')
    if (stored) display.value = { ...DISPLAY_DEFAULT, ...stored }
  } catch {
    // No backend: the defaults are already in place.
  }
})

// ── group state ──────────────────────────────────────────────────────────────

/** Collapsed headings, by tab key. Absent means expanded. */
const collapsed = ref<Record<string, boolean>>({})

/** Which heading is being renamed, and the buffer it is being renamed into. */
const renaming = ref<string | null>(null)
const draft = ref('')
/**
 * A *function* ref, not `ref="renameInput"`.
 *
 * The field lives inside the `v-for` over groups, and Vue collects a string ref
 * inside a loop into an **array** — `renameInput.value.focus()` then fails with
 * "not a function" and the rename opens unfocused, so the first keystroke goes to
 * the terminal instead. Only one group renames at a time, so a single slot is the
 * honest shape.
 */
const renameInput = ref<HTMLInputElement | null>(null)

function bindRenameInput(el: unknown) {
  renameInput.value = el instanceof HTMLInputElement ? el : null
}

function title(key: string): string {
  return props.views.find((view) => view.key === key)?.title ?? 'Session'
}

function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean)
  return parts[parts.length - 1] ?? path
}

/** What a pane calls itself under the current title setting. */
function paneTitle(slot: PaneSlot): string {
  if (display.value.title === 'directory') return baseName(slot.cwd) || slot.shellLabel
  return slot.shellLabel
}

function paneSubtitle(slot: PaneSlot): string {
  return display.value.subtitle === 'shell' ? slot.shellLabel : slot.cwd
}

/** The third line in expanded density: what the two-line row had to leave out. */
function paneDetail(slot: PaneSlot): string {
  const state = slot.exited ? 'exited' : 'live'
  const where = display.value.subtitle === 'shell' ? baseName(slot.cwd) : slot.shellLabel
  return `${where} · ${state}`
}

/**
 * The pane's badge — two states in the plain edition: the shell is live, or it
 * has ended. A plain terminal cannot know more (there is no shell integration
 * to report a running command), and a badge that guesses is worse than one
 * that says less.
 */
function badgeOf(slot: PaneSlot): BadgeId {
  return slot.exited ? 'exited' : 'idle'
}

const BADGE_HINT: Record<BadgeId, (slot: PaneSlot) => string> = {
  exited: () => 'The shell has ended.',
  idle: () => 'Live.',
}

/** Everything the template needs to draw one badge, resolved in one call. */
function paneBadge(slot: PaneSlot): { paths: readonly string[]; tone: string; hint: string } {
  const id = badgeOf(slot)
  const badge = BADGES[id]
  return { paths: badge.paths, tone: badge.tone, hint: BADGE_HINT[id](slot) }
}

/** The heading's own badge, for the tabs view where the pane rows are hidden. */
function groupBadge(panes: PaneSlot[]): { paths: readonly string[]; tone: string } | null {
  if (!panes.length) return null
  if (panes.every((slot) => slot.exited)) return BADGES.exited
  return BADGES.idle
}

function isUnread(paneKey: string): boolean {
  return props.unread?.includes(paneKey) ?? false
}

function groupUnread(panes: PaneSlot[]): boolean {
  return panes.some((slot) => isUnread(slot.key))
}

/** The tab's colour as a paintable value, or `null` when it has none. */
function tint(key: string): string | null {
  const index = props.tabColors?.[key]
  return typeof index === 'number' ? ansi.colour(index) : null
}

/**
 * The tint as a style object.
 *
 * A custom property is not part of `CSSProperties`, so an inline object literal
 * in the template does not type-check against `HTMLAttributes['style']`. Building
 * it here keeps the cast in one place instead of in the markup.
 */
function tintStyle(key: string): Record<string, string> | undefined {
  const colour = tint(key)
  return colour ? { '--cgr-tint': colour } : undefined
}

/**
 * Groups and their panes after the filter, matching on everything visible —
 * heading, pane title and subtitle. A group whose *heading* matches keeps all its
 * panes: having searched for a window, you want to see what is in it.
 */
const groups = computed(() => {
  const needle = filter.value.trim().toLowerCase()
  const rows = props.tabs.map((tab) => ({
    tab,
    name: title(tab.key),
    panes: props.panesByTab[tab.key] ?? [],
  }))
  if (!needle) return rows

  return rows
    .map((row) => {
      if (row.name.toLowerCase().includes(needle)) return row
      const panes = row.panes.filter((slot) =>
        `${slot.shellLabel} ${slot.cwd}`.toLowerCase().includes(needle)
      )
      return { ...row, panes }
    })
    .filter((row) => row.panes.length > 0 || row.name.toLowerCase().includes(needle))
})

const hiddenByFilter = computed(() => {
  if (!filter.value.trim()) return 0
  return props.tabs.length - groups.value.length
})

/** Panes are listed unless the filter is hiding them or the heading is shut. */
function panesOf(group: { tab: ConsoleTab; panes: PaneSlot[] }): PaneSlot[] {
  if (display.value.view === 'tabs') return []
  return collapsed.value[group.tab.key] ? [] : group.panes
}

function toggleCollapse(key: string) {
  collapsed.value = { ...collapsed.value, [key]: !collapsed.value[key] }
}

function setCollapsed(key: string, value: boolean) {
  if (!!collapsed.value[key] === value) return
  collapsed.value = { ...collapsed.value, [key]: value }
}

/**
 * Clicking a heading selects its group; clicking the one already in front folds
 * it away.
 *
 * Warp's headings toggle on click, but Warp's headings are also its tab strip —
 * ours is the only way to *reach* a session, so a plain click must never be able
 * to hide a group without first bringing it forward. The chevron toggles without
 * selecting for the times you want only that.
 */
function onHeadClick(key: string) {
  if (key === props.activeTabKey) {
    toggleCollapse(key)
    return
  }
  setCollapsed(key, false)
  emit('selectTab', key)
}

// ── rename ───────────────────────────────────────────────────────────────────

function startRename(key: string) {
  closeOverlay()
  // Renaming something you cannot see is a strange thing to be doing, and the
  // double-click that opened this may itself have folded the group shut.
  setCollapsed(key, false)
  renaming.value = key
  draft.value = title(key)
  nextTick(() => {
    renameInput.value?.focus()
    renameInput.value?.select()
  })
}

function commitRename() {
  const key = renaming.value
  if (!key) return
  const next = draft.value.trim()
  renaming.value = null
  // An empty name is not a name — the store's derived title comes back instead of
  // a group labelled with nothing.
  if (next && next !== title(key)) emit('renameTab', { key, title: next })
}

function onRenameKey(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    event.preventDefault()
    commitRename()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    renaming.value = null
  }
}

// ── bulk closes ──────────────────────────────────────────────────────────────
//
// Built out of `closeTab` rather than asking for store actions of their own: the
// store removes a tab from the list before its first `await`, so a run of emits
// resolves against the list each one leaves behind. The keys are read up front
// for the same reason — indices would shift underneath the loop.

function closeOthers(key: string) {
  props.tabs
    .filter((tab) => tab.key !== key)
    .forEach((tab) => emit('closeTab', tab.key))
}

function closeBelow(key: string) {
  const index = props.tabs.findIndex((tab) => tab.key === key)
  if (index < 0) return
  props.tabs.slice(index + 1).forEach((tab) => emit('closeTab', tab.key))
}

function closeOtherPanes(tabKey: string, paneKey: string) {
  ;(props.panesByTab[tabKey] ?? [])
    .filter((slot) => slot.key !== paneKey)
    .forEach((slot) => emit('closePane', slot.key))
}

function moveTab(key: string, delta: -1 | 1) {
  const from = props.tabs.findIndex((tab) => tab.key === key)
  const to = from + delta
  if (from < 0 || to < 0 || to >= props.tabs.length) return
  emit('reorderTab', { from, to })
}

// ── overlays ─────────────────────────────────────────────────────────────────
//
// Absolutely positioned inside the list, never `fixed`: a fixed overlay is laid
// out against the window and walks straight out of the module panel the moment
// the sidebar is docked anywhere but the far left.

const GUTTER = 6

type Overlay =
  | { kind: 'tab'; tabKey: string }
  | { kind: 'pane'; tabKey: string; paneKey: string }
  | { kind: 'group'; groupKey: string }
  | { kind: 'display' }

/** What is open, kept apart from where it is: the clamp rewrites only the where. */
const overlay = ref<Overlay | null>(null)
const overlayAt = ref({ x: 0, y: 0 })
const overlayEl = ref<HTMLElement | null>(null)
/** Where focus goes when the overlay shuts, so a keyboard open is not a dead end. */
let opener: HTMLElement | null = null

// Resolved here rather than in the template: the menu's subject is a union member
// away, and a template that has to narrow one is a template full of casts.
const menuTabKey = computed(() => {
  const open = overlay.value
  return open && (open.kind === 'tab' || open.kind === 'pane') ? open.tabKey : ''
})
const menuGroupKey = computed(() => {
  const open = overlay.value
  return open?.kind === 'group' ? open.groupKey : ''
})
const menuGroupTitle = computed(
  () => props.groupList?.find((g) => g.key === menuGroupKey.value)?.title ?? ''
)
/** The folder the menu's tab currently sits in, `null` when ungrouped. */
const menuTabGroupKey = computed(
  () => props.tabs.find((t) => t.key === menuTabKey.value)?.groupKey ?? null
)
const menuPaneKey = computed(() => {
  const open = overlay.value
  return open?.kind === 'pane' ? open.paneKey : ''
})
const menuTabName = computed(() => (menuTabKey.value ? title(menuTabKey.value) : ''))
const menuTabIndex = computed(() => props.tabs.findIndex((tab) => tab.key === menuTabKey.value))
const menuTabColor = computed(() => props.tabColors?.[menuTabKey.value] ?? null)
const menuPaneCount = computed(() => props.panesByTab[menuTabKey.value]?.length ?? 0)

/**
 * Place an overlay at a point in the list, then pull it back in if it hangs out.
 *
 * The sidebar's minimum is 120px and this menu is wider than that at any sane
 * font size, so the clamp is not a nicety: the menu is capped to the list's width
 * in CSS and shifted left here until its measured box fits, and near the bottom
 * it flips above the anchor the way a native context menu does.
 */
async function place(value: Overlay, clientX: number, clientY: number) {
  const host = root.value?.getBoundingClientRect()
  overlay.value = value
  overlayAt.value = {
    x: Math.round(clientX - (host?.left ?? 0)),
    y: Math.round(clientY - (host?.top ?? 0)),
  }
  if (!host) return

  await nextTick()
  const box = overlayEl.value?.getBoundingClientRect()
  if (!box || !overlay.value) return

  // Vertically the list scrolls, so the edge that matters is the viewport the
  // list sits in, not the list's own (much taller) box.
  const view = root.value?.parentElement?.getBoundingClientRect() ?? host
  let { x, y } = overlayAt.value
  if (box.right > host.right - GUTTER) x -= box.right - host.right + GUTTER
  x = Math.max(GUTTER, x)
  // Too low and it flips *above* the anchor rather than being nudged up into it,
  // which is the one correction that never lands the menu under the pointer.
  if (box.bottom > view.bottom - GUTTER) y = Math.max(GUTTER, y - box.height)
  overlayAt.value = { x, y }

  // A menu opened from the keyboard has to land somewhere focusable, or Tab
  // continues past it into the rows behind.
  overlayEl.value?.querySelector<HTMLElement>('[data-item]')?.focus()
}

function closeOverlay() {
  if (!overlay.value) return
  overlay.value = null
  opener?.focus()
  opener = null
}

function openFrom(event: MouseEvent | KeyboardEvent, value: Overlay) {
  opener = event.currentTarget instanceof HTMLElement ? event.currentTarget : null
  const anchor = opener?.getBoundingClientRect()
  // A right-click opens where the pointer is; a button opens under the button,
  // because inside an 18px control a pointer-positioned menu covers its opener.
  const pointer = 'clientX' in event && event.type === 'contextmenu' ? event : null
  const x = pointer ? pointer.clientX : (anchor?.right ?? 0)
  const y = pointer ? pointer.clientY : (anchor?.bottom ?? 0) + 2
  void place(value, x, y)
}

function toggleDisplay(event: MouseEvent) {
  if (overlay.value?.kind === 'display') {
    closeOverlay()
    return
  }
  openFrom(event, { kind: 'display' })
}

/** Arrow keys walk the open overlay; Escape hands focus back to what opened it. */
function onOverlayKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    closeOverlay()
    return
  }
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
  const items = Array.from(overlayEl.value?.querySelectorAll<HTMLElement>('[data-item]') ?? [])
  if (!items.length) return
  event.preventDefault()
  const at = items.indexOf(document.activeElement as HTMLElement)
  const step = event.key === 'ArrowDown' ? 1 : -1
  items[(at + step + items.length) % items.length]?.focus()
}

/**
 * Dismissal: a pointer outside the overlay, or the list scrolling away from it.
 * Registered only while one is open — a permanent pair of window listeners for a
 * menu that is shut is two listeners too many.
 */
let detachOverlay: (() => void) | null = null

watch(overlay, (open) => {
  detachOverlay?.()
  detachOverlay = null
  if (!open || typeof window === 'undefined') return

  const onDown = (event: Event) => {
    const target = event.target
    if (target instanceof Node && overlayEl.value?.contains(target)) return
    // Not `closeOverlay`: the pointer is choosing where focus goes next, and
    // yanking it back to the opener would fight the click that is in flight.
    overlay.value = null
    opener = null
  }
  const onScroll = () => {
    overlay.value = null
    opener = null
  }

  window.addEventListener('pointerdown', onDown, true)
  window.addEventListener('scroll', onScroll, true)
  detachOverlay = () => {
    window.removeEventListener('pointerdown', onDown, true)
    window.removeEventListener('scroll', onScroll, true)
  }
})

onBeforeUnmount(() => {
  detachOverlay?.()
  detachOverlay = null
  overlay.value = null
  opener = null
})

/**
 * Run a menu item, then put focus back where the menu was opened from — unless
 * the item took focus itself, as rename does. Without this a keyboard user who
 * picks "Move tab down" is left on `<body>` and has to Tab in from the top again.
 */
function run(action: () => void) {
  const from = opener
  opener = null
  // The action FIRST, the close second — the items read their subject through
  // `menuTabKey`/`menuGroupKey`, which resolve against the open overlay. With
  // the close first every one of them saw '' and silently did nothing.
  action()
  overlay.value = null
  void nextTick(() => {
    if (from?.isConnected && document.activeElement === document.body) from.focus()
  })
}

// ── row input ────────────────────────────────────────────────────────────────

function onHeadKey(event: KeyboardEvent, key: string) {
  if (event.key === 'F2') {
    event.preventDefault()
    startRename(key)
  } else if (event.key === 'Delete') {
    event.preventDefault()
    emit('closeTab', key)
  } else if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
    event.preventDefault()
    moveTab(key, event.key === 'ArrowUp' ? -1 : 1)
  } else if (event.key === 'ArrowLeft') {
    event.preventDefault()
    setCollapsed(key, true)
  } else if (event.key === 'ArrowRight') {
    event.preventDefault()
    setCollapsed(key, false)
  } else if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
    event.preventDefault()
    openFrom(event, { kind: 'tab', tabKey: key })
  }
}

function onPaneKey(event: KeyboardEvent, tabKey: string, paneKey: string) {
  if (event.key === 'Delete') {
    event.preventDefault()
    emit('closePane', paneKey)
  } else if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
    event.preventDefault()
    openFrom(event, { kind: 'pane', tabKey, paneKey })
  }
}

/** Middle-click closes, as it does on every tab strip ever shipped. */
function onAux(event: MouseEvent, close: () => void) {
  if (event.button !== 1) return
  event.preventDefault()
  close()
}

// ── dragging: terminal groups reorder AND file into folders ─────────────────
//
// One drag gesture, one target model. While a terminal group is in flight:
//   - a row of the SAME section arms a reorder,
//   - anywhere on ANOTHER section (its whole area, not just the heading)
//     arms filing it there — into that folder, or out to ungrouped,
//   - the new-group zone arms founding a fresh folder around it.
// Exactly one target is armed at a time, and leaving a target disarms it —
// a highlight that survives the pointer's departure feels broken.

type DropTarget =
  | { kind: 'row'; key: string }
  | { kind: 'section'; groupKey: string | null }
  | { kind: 'folder'; key: string }
  | { kind: 'new' }

const dragKey = ref<string | null>(null)
/** A FOLDER heading in flight (reordering the folders themselves). Mutually
 * exclusive with `dragKey` — one `dragstart` fires per gesture. */
const dragGroup = ref<string | null>(null)
const dropTarget = ref<DropTarget | null>(null)

/** For the template's class bindings. */
const dropKey = computed(() =>
  dropTarget.value?.kind === 'row' ? dropTarget.value.key : null
)
const dropSection = computed(() =>
  dropTarget.value?.kind === 'section' ? dropTarget.value.groupKey : undefined
)
const dropNew = computed(() => dropTarget.value?.kind === 'new')
const dropFolder = computed(() =>
  dropTarget.value?.kind === 'folder' ? dropTarget.value.key : null
)

const draggedTab = () => props.tabs.find((t) => t.key === dragKey.value)

function onDragStart(event: DragEvent, key: string) {
  // A filtered list is a list with holes in it, and a drop index read off it
  // would move the wrong group. Reordering waits until the list is whole again.
  if (filter.value.trim()) {
    event.preventDefault()
    return
  }
  dragKey.value = key
  event.dataTransfer?.setData('text/plain', key)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

/** Over a terminal-group row: reorder within its section, filing across. */
function onDragOver(event: DragEvent, key: string) {
  const dragged = draggedTab()
  if (!dragged || dragged.key === key) return
  const target = props.tabs.find((t) => t.key === key)
  if (!target) return
  event.preventDefault()
  event.stopPropagation()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value =
    (dragged.groupKey ?? null) === (target.groupKey ?? null)
      ? { kind: 'row', key }
      : { kind: 'section', groupKey: target.groupKey ?? null }
}

function onDrop(event: DragEvent, key: string) {
  event.preventDefault()
  event.stopPropagation()
  const dragged = draggedTab()
  const target = props.tabs.find((t) => t.key === key)
  endDrag()
  if (!dragged || !target || dragged.key === target.key) return
  if ((dragged.groupKey ?? null) === (target.groupKey ?? null)) {
    const from = props.tabs.findIndex((tab) => tab.key === dragged.key)
    const to = props.tabs.findIndex((tab) => tab.key === target.key)
    if (from >= 0 && to >= 0 && from !== to) emit('reorderTab', { from, to })
  } else {
    emit('moveTabToGroup', { tabKey: dragged.key, groupKey: target.groupKey ?? null })
  }
}

/** Over a section's area (folder or the ungrouped tail): arm filing there. */
function onSectionDragOver(event: DragEvent, groupKey: string | null) {
  const dragged = draggedTab()
  if (!dragged || dragged.kind !== 'shell') return
  if ((dragged.groupKey ?? null) === groupKey) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = { kind: 'section', groupKey }
}

function onSectionDrop(event: DragEvent, groupKey: string | null) {
  event.preventDefault()
  const dragged = draggedTab()
  endDrag()
  if (!dragged || dragged.kind !== 'shell') return
  if ((dragged.groupKey ?? null) === groupKey) return
  emit('moveTabToGroup', { tabKey: dragged.key, groupKey })
}

function onNewGroupDragOver(event: DragEvent) {
  const dragged = draggedTab()
  if (!dragged || dragged.kind !== 'shell') return
  event.preventDefault()
  event.stopPropagation()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = { kind: 'new' }
}

function onNewGroupDrop(event: DragEvent) {
  event.preventDefault()
  event.stopPropagation()
  const tabKey = dragKey.value
  endDrag()
  if (tabKey) emit('createGroupWith', tabKey)
}

// ── folder drag: reorder the folders themselves ─────────────────────────────

function onFolderDragStart(event: DragEvent, key: string) {
  if (filter.value.trim()) {
    event.preventDefault()
    return
  }
  dragGroup.value = key
  event.dataTransfer?.setData('text/plain', key)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function onFolderDragOver(event: DragEvent, key: string) {
  if (!dragGroup.value || dragGroup.value === key) return
  event.preventDefault()
  event.stopPropagation()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = { kind: 'folder', key }
}

function onFolderDrop(event: DragEvent, key: string) {
  if (!dragGroup.value) return
  event.preventDefault()
  event.stopPropagation()
  const from = (props.groupList ?? []).findIndex((g) => g.key === dragGroup.value)
  const to = (props.groupList ?? []).findIndex((g) => g.key === key)
  endDrag()
  if (from < 0 || to < 0 || from === to) return
  emit('reorderGroup', { from, to })
}

/** Disarm when the pointer truly leaves the target (not just its children). */
function onTargetDragLeave(event: DragEvent) {
  const el = event.currentTarget
  const to = event.relatedTarget
  if (el instanceof HTMLElement && to instanceof Node && el.contains(to)) return
  dropTarget.value = null
}

/** Position of the new-tab menu inside the panel, or `null` when it is shut. */
const shellMenu = ref<{ x: number; y: number } | null>(null)

function toggleShellMenu(event: MouseEvent) {
  if (shellMenu.value || !(props.shells && props.shells.length > 1)) {
    shellMenu.value = null
    // One shell, or a second click: the button keeps its plain meaning.
    if (!shellMenu.value && !(props.shells && props.shells.length > 1)) emit('create')
    return
  }
  const button = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const host = root.value?.getBoundingClientRect()
  shellMenu.value = {
    x: Math.max(4, Math.round(button.left - (host?.left ?? 0)) - 150),
    y: Math.round(button.bottom - (host?.top ?? 0) + 4),
  }
}

function openShell(id?: string) {
  shellMenu.value = null
  emit('create', id)
}

function endDrag() {
  dragKey.value = null
  dragGroup.value = null
  dropTarget.value = null
}

// ── the folder level ─────────────────────────────────────────────────────────

/**
 * The sidebar's sections: one per folder (in folder order, empty ones
 * included so a fresh folder is visible and rename-able), plus the ungrouped
 * terminal groups as the tail section (`group: null`).
 */
const sections = computed(() => {
  const byGroup = new Map<string | null, typeof groups.value>()
  for (const row of groups.value) {
    const key = row.tab.kind === 'shell' ? row.tab.groupKey ?? null : null
    const list = byGroup.get(key) ?? []
    list.push(row)
    byGroup.set(key, list)
  }
  return [
    ...(props.groupList ?? []).map((group) => ({
      group: group as ConsoleGroup | null,
      rows: byGroup.get(group.key) ?? [],
    })),
    { group: null as ConsoleGroup | null, rows: byGroup.get(null) ?? [] },
  ].filter((section) => section.group !== null || section.rows.length > 0)
})

/** Folder collapse — hides its terminal groups, keyed apart from tab keys. */
const foldedGroups = ref<Record<string, boolean>>({})

function toggleGroupFold(key: string) {
  foldedGroups.value = { ...foldedGroups.value, [key]: !foldedGroups.value[key] }
}

/** Folder rename, the same double-click contract the terminal groups have. */
const renamingGroup = ref<string | null>(null)
const groupDraft = ref('')
const groupRenameInput = ref<HTMLInputElement | null>(null)

function bindGroupRenameInput(el: unknown) {
  // Store only. A function ref runs on EVERY re-render of the element — a
  // `select()` in here re-selects the whole draft after each keystroke, and
  // the next key then replaces it: the name could never grow past one letter.
  groupRenameInput.value = el instanceof HTMLInputElement ? el : null
}

function startGroupRename(key: string) {
  const group = props.groupList?.find((g) => g.key === key)
  if (!group) return
  renamingGroup.value = key
  groupDraft.value = group.title
  void nextTick(() => {
    groupRenameInput.value?.focus()
    groupRenameInput.value?.select()
  })
}

function commitGroupRename() {
  const key = renamingGroup.value
  renamingGroup.value = null
  if (key && groupDraft.value.trim()) emit('renameGroup', { key, title: groupDraft.value })
}

function onGroupRenameKey(event: KeyboardEvent) {
  if (event.key === 'Enter') commitGroupRename()
  if (event.key === 'Escape') renamingGroup.value = null
}
</script>

<template>
  <div ref="root" class="cgr" @dblclick.self="canCreate !== false && emit('create')">
    <div class="cgr-bar">
      <span class="cgr-bar-ico" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path v-for="d in SEARCH_ICON" :key="d" :d="d" />
        </svg>
      </span>
      <input
        v-model="filter"
        class="cgr-search"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Search tabs..."
        aria-label="Search tabs and panes"
        @keydown.escape.prevent.stop="filter = ''"
      />
      <button
        class="cgr-bar-btn"
        aria-label="Display settings"
        :aria-expanded="overlay?.kind === 'display'"
        title="Display settings"
        @click="toggleDisplay"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path v-for="d in SLIDERS_ICON" :key="d" :d="d" />
        </svg>
      </button>
      <button
        class="cgr-bar-btn"
        :disabled="canCreate === false"
        aria-label="New tab"
        :title="keys?.newTab ? `New tab (${keys.newTab})` : 'New tab'"
        aria-haspopup="menu"
        @click="toggleShellMenu"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path v-for="d in CREATE_ICON" :key="d" :d="d" />
        </svg>
      </button>

      <div
        v-if="shellMenu"
        class="cgr-pop"
        role="menu"
        :style="{ left: `${shellMenu.x}px`, top: `${shellMenu.y}px` }"
        @click.stop
      >
        <button
          v-for="shell in shells"
          :key="shell.id"
          data-item
          class="cgr-pop-item"
          role="menuitem"
          @click="openShell(shell.id)"
        >
          <span class="cgr-pop-label">{{ shell.label }}</span>
          <span v-if="shell.isDefault" class="cgr-pop-key">default</span>
        </button>
      </div>
    </div>

    <p v-if="!tabs.length" class="cgr-note">No session yet. Double-click here to open one.</p>
    <p v-else-if="!groups.length" class="cgr-note">Nothing matches “{{ filter }}”.</p>

    <template v-for="section in sections" :key="section.group?.key ?? 'ungrouped'">
      <!-- One section = one folder (or the ungrouped tail). The WHOLE area is
           the drop target for filing a terminal group here — a target the size
           of a heading strip is a target people miss. -->
      <div
        class="cgr-section"
        :class="{ 'is-drop': dropSection === (section.group?.key ?? null) }"
        @dragover="onSectionDragOver($event, section.group?.key ?? null)"
        @drop="onSectionDrop($event, section.group?.key ?? null)"
        @dragleave="onTargetDragLeave"
      >
      <!-- The folder heading (level 1). Draggable to reorder the folders. -->
      <div
        v-if="section.group"
        class="cgr-folder"
        :class="{ 'is-dragging': dragGroup === section.group.key, 'is-reorder': dropFolder === section.group.key }"
        :draggable="!filter.trim() && renamingGroup !== section.group.key"
        @dragstart="onFolderDragStart($event, section.group.key)"
        @dragover="onFolderDragOver($event, section.group.key)"
        @drop="onFolderDrop($event, section.group.key)"
        @dragend="endDrag"
        @dragleave="onTargetDragLeave"
      >
        <input
          v-if="renamingGroup === section.group.key"
          :ref="bindGroupRenameInput"
          v-model="groupDraft"
          class="cgr-rename"
          type="text"
          spellcheck="false"
          aria-label="Group name"
          @keydown="onGroupRenameKey"
          @blur="commitGroupRename"
        />
        <template v-else>
          <button
            class="cgr-folder-twist"
            :aria-expanded="!foldedGroups[section.group.key]"
            :aria-label="foldedGroups[section.group.key] ? `Expand ${section.group.title}` : `Collapse ${section.group.title}`"
            @click="toggleGroupFold(section.group.key)"
          >
            <svg
              class="cgr-twist-svg"
              :class="{ 'is-open': !foldedGroups[section.group.key] }"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
            >
              <path v-for="d in CHEVRON_ICON" :key="d" :d="d" />
            </svg>
          </button>
          <button
            class="cgr-folder-name"
            :title="`${section.group.title} — double-click to rename`"
            @dblclick.stop="startGroupRename(section.group.key)"
            @click="toggleGroupFold(section.group.key)"
            @contextmenu.prevent="openFrom($event, { kind: 'group', groupKey: section.group.key })"
          >
            {{ section.group.title }}
            <span class="cgr-count">{{ section.rows.length }}</span>
          </button>
          <span class="cgr-tools">
            <button
              class="cgr-tool"
              :aria-label="`Group menu for ${section.group.title}`"
              title="Group menu"
              @click="openFrom($event, { kind: 'group', groupKey: section.group.key })"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path v-for="d in KEBAB_ICON" :key="d" :d="d" />
              </svg>
            </button>
          </span>
        </template>
      </div>

      <p
        v-if="section.group && !section.rows.length && !foldedGroups[section.group.key]"
        class="cgr-note is-folder-empty"
      >Drag a terminal group here.</p>

      <template v-for="group in section.rows" :key="group.tab.key">
      <div
        v-if="!section.group || !foldedGroups[section.group.key]"
        class="cgr-group"
        :class="{
          'is-dragging': dragKey === group.tab.key,
          'is-drop': dropKey === group.tab.key,
          'in-folder': !!section.group,
        }"
        :style="tintStyle(group.tab.key)"
      >
      <input
        v-if="renaming === group.tab.key"
        :ref="bindRenameInput"
        v-model="draft"
        class="cgr-rename"
        type="text"
        spellcheck="false"
        aria-label="Tab name"
        @keydown="onRenameKey"
        @blur="commitRename"
      />
      <div
        v-else
        class="cgr-row cgr-row-head"
        :class="{ 'is-active': group.tab.key === activeTabKey, 'has-tint': !!tint(group.tab.key) }"
        :draggable="!filter.trim()"
        @dragstart="onDragStart($event, group.tab.key)"
        @dragover="onDragOver($event, group.tab.key)"
        @drop="onDrop($event, group.tab.key)"
        @dragend="endDrag"
      >
        <button
          class="cgr-twist"
          :aria-label="collapsed[group.tab.key] ? `Expand ${group.name}` : `Collapse ${group.name}`"
          :aria-expanded="!collapsed[group.tab.key]"
          @click="toggleCollapse(group.tab.key)"
        >
          <svg
            class="cgr-twist-svg"
            :class="{ 'is-open': !collapsed[group.tab.key] }"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
          >
            <path v-for="d in CHEVRON_ICON" :key="d" :d="d" />
          </svg>
        </button>

        <button
          class="cgr-head"
          :title="`${group.name} — double-click to rename`"
          @click="onHeadClick(group.tab.key)"
          @dblclick.stop="startRename(group.tab.key)"
          @mousedown.middle.prevent
          @auxclick="onAux($event, () => emit('closeTab', group.tab.key))"
          @contextmenu.prevent="openFrom($event, { kind: 'tab', tabKey: group.tab.key })"
          @keydown="onHeadKey($event, group.tab.key)"
        >
          <span class="cgr-head-name">{{ group.name }}</span>
          <span
            v-if="display.view === 'tabs' && groupBadge(group.panes)"
            class="cgr-badge is-inline"
            :class="`is-${groupBadge(group.panes)?.tone}`"
            aria-hidden="true"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in (groupBadge(group.panes)?.paths ?? [])" :key="d" :d="d" />
            </svg>
          </span>
          <span v-if="display.view === 'tabs'" class="cgr-count">{{ group.panes.length }}</span>
          <span v-if="groupUnread(group.panes) && group.tab.key !== activeTabKey" class="cgr-dot" role="img" aria-label="Unread output" />
        </button>

        <span class="cgr-tools">
          <button
            class="cgr-tool"
            :aria-label="`Tab menu for ${group.name}`"
            title="Tab menu"
            @click="openFrom($event, { kind: 'tab', tabKey: group.tab.key })"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in KEBAB_ICON" :key="d" :d="d" />
            </svg>
          </button>
          <button
            class="cgr-tool is-close"
            :aria-label="`Close ${group.name}`"
            :title="keys?.closeTab ? `Close tab (${keys.closeTab})` : 'Close tab'"
            @click="emit('closeTab', group.tab.key)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in CLOSE_ICON" :key="d" :d="d" />
            </svg>
          </button>
        </span>
      </div>

      <div
        v-for="slot in panesOf(group)"
        :key="slot.key"
        class="cgr-row cgr-row-pane"
        :class="{
          'is-active': group.tab.key === activeTabKey && slot.key === group.tab.activePaneKey,
          'is-exited': slot.exited,
        }"
      >
        <button
          class="cgr-pane"
          @click="emit('focusPane', slot.key)"
          @mousedown.middle.prevent
          @auxclick="onAux($event, () => emit('closePane', slot.key))"
          @contextmenu.prevent="openFrom($event, { kind: 'pane', tabKey: group.tab.key, paneKey: slot.key })"
          @keydown="onPaneKey($event, group.tab.key, slot.key)"
        >
          <span class="cgr-mark" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in group.tab.kind === 'suite' ? SUITE_ICON : PANE_ICON" :key="d" :d="d" />
            </svg>
            <span class="cgr-badge" :class="`is-${paneBadge(slot).tone}`" :title="paneBadge(slot).hint">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
                <path v-for="d in paneBadge(slot).paths" :key="d" :d="d" />
              </svg>
            </span>
          </span>

          <span class="cgr-text">
            <span class="cgr-title" :title="paneTitle(slot)">{{ paneTitle(slot) }}</span>
            <!-- `dir="rtl"` clips the front of the path instead of the end: the tail
                 is what tells two checkouts apart. The `&lrm;` keeps the separators
                 from being reordered by the bidi algorithm. -->
            <span
              class="cgr-sub"
              :dir="display.subtitle === 'path' ? 'rtl' : 'ltr'"
              :title="paneSubtitle(slot)"
            >&lrm;{{ paneSubtitle(slot) }}</span>
            <span v-if="display.density === 'expanded'" class="cgr-detail">{{ paneDetail(slot) }}</span>
          </span>

          <span
            v-if="isUnread(slot.key) && slot.key !== group.tab.activePaneKey"
            class="cgr-dot"
            role="img"
            aria-label="Unread output"
          />
        </button>

        <span class="cgr-tools">
          <button
            class="cgr-tool"
            :aria-label="`Pane menu for ${paneTitle(slot)}`"
            title="Pane menu"
            @click="openFrom($event, { kind: 'pane', tabKey: group.tab.key, paneKey: slot.key })"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in KEBAB_ICON" :key="d" :d="d" />
            </svg>
          </button>
          <button
            class="cgr-tool is-close"
            :aria-label="`Close ${paneTitle(slot)}`"
            :title="keys?.closePane ? `Close pane (${keys.closePane})` : 'Close pane'"
            @click="emit('closePane', slot.key)"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path v-for="d in CLOSE_ICON" :key="d" :d="d" />
            </svg>
          </button>
        </span>
      </div>
      </div>
      </template>
      </div>
    </template>

    <p v-if="hiddenByFilter > 0" class="cgr-note">
      {{ hiddenByFilter }} group(s) hidden by the search.
    </p>

    <!-- The new-group zone: exists only while a terminal group is in flight.
         Dropping here founds a fresh named folder around it. -->
    <div
      v-if="dragKey"
      class="cgr-newgroup"
      :class="{ 'is-armed': dropNew }"
      @dragover="onNewGroupDragOver"
      @drop="onNewGroupDrop"
      @dragleave="onTargetDragLeave"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M3.5 6.5A1.5 1.5 0 0 1 5 5h4l2 2.5h8A1.5 1.5 0 0 1 20.5 9v8.5A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z" />
        <path d="M12 10.5v5M9.5 13h5" />
      </svg>
      New group
    </div>

    <!-- Takes the leftover height so the "double-click nothing to open a tab"
         target is the whole empty panel, not the four pixels under the last row.
         During a pane drag it doubles as the new-group target, so a drop on any
         empty space below the list means "its own group" too. -->
    <div
      class="cgr-void"
      @dblclick="canCreate !== false && emit('create')"
      @dragover="onNewGroupDragOver"
      @drop="onNewGroupDrop"
    />

    <!-- ------------------------------------------------------------ overlays -- -->
    <div
      v-if="overlay"
      ref="overlayEl"
      class="cgr-pop"
      :style="{ left: `${overlayAt.x}px`, top: `${overlayAt.y}px` }"
      role="menu"
      @keydown="onOverlayKey"
    >
      <template v-if="overlay.kind === 'tab'">
        <button data-item class="cgr-pop-item" role="menuitem" @click="run(() => startRename(menuTabKey))">
          <span class="cgr-pop-label">Rename tab</span><span class="cgr-pop-key">F2</span>
        </button>
        <button
          v-if="menuTabIndex > 0"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => moveTab(menuTabKey, -1))"
        >
          <span class="cgr-pop-label">Move tab up</span><span class="cgr-pop-key">Alt+Up</span>
        </button>
        <button
          v-if="menuTabIndex >= 0 && menuTabIndex < tabs.length - 1"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => moveTab(menuTabKey, 1))"
        >
          <span class="cgr-pop-label">Move tab down</span><span class="cgr-pop-key">Alt+Down</span>
        </button>

        <div class="cgr-pop-sep" role="separator" />

        <!-- Filing (the folder level). Kept flat: the folder list is short. -->
        <button
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => emit('createGroupWith', menuTabKey))"
        >
          <span class="cgr-pop-label">New group from this</span>
        </button>
        <button
          v-for="folder in (groupList ?? []).filter((g) => g.key !== menuTabGroupKey)"
          :key="folder.key"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => emit('moveTabToGroup', { tabKey: menuTabKey, groupKey: folder.key }))"
        >
          <span class="cgr-pop-label">Move to “{{ folder.title }}”</span>
        </button>
        <button
          v-if="menuTabGroupKey"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => emit('moveTabToGroup', { tabKey: menuTabKey, groupKey: null }))"
        >
          <span class="cgr-pop-label">Remove from group</span>
        </button>

        <div class="cgr-pop-sep" role="separator" />

        <button data-item class="cgr-pop-item" role="menuitem" @click="run(() => emit('closeTab', menuTabKey))">
          <span class="cgr-pop-label">Close tab</span><span class="cgr-pop-key">{{ keys?.closeTab }}</span>
        </button>
        <button
          v-if="tabs.length > 1"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => closeOthers(menuTabKey))"
        >
          <span class="cgr-pop-label">Close other tabs</span>
        </button>
        <button
          v-if="menuTabIndex >= 0 && menuTabIndex < tabs.length - 1"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => closeBelow(menuTabKey))"
        >
          <span class="cgr-pop-label">Close tabs below</span>
        </button>

        <template v-if="tabColors">
          <div class="cgr-pop-sep" role="separator" />
          <div class="cgr-swatches" role="group" :aria-label="`Colour for ${menuTabName}`">
            <button
              data-item
              class="cgr-swatch is-none"
              :class="{ 'is-on': menuTabColor === null }"
              aria-label="No colour"
              title="No colour"
              @click="run(() => emit('setTabColor', { key: menuTabKey, color: null }))"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <path v-for="d in NO_COLOUR_ICON" :key="d" :d="d" />
              </svg>
            </button>
            <button
              v-for="swatch in SWATCHES"
              :key="swatch.index"
              data-item
              class="cgr-swatch"
              :class="{ 'is-on': menuTabColor === swatch.index }"
              :style="{ '--cgr-swatch': ansi.colour(swatch.index) }"
              :aria-label="swatch.name"
              :title="swatch.name"
              @click="run(() => emit('setTabColor', { key: menuTabKey, color: swatch.index }))"
            />
          </div>
        </template>
      </template>

      <template v-else-if="overlay.kind === 'group'">
        <button
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => startGroupRename(menuGroupKey))"
        >
          <span class="cgr-pop-label">Rename group</span>
        </button>
        <div class="cgr-pop-sep" role="separator" />
        <button
          data-item class="cgr-pop-item" role="menuitem"
          :title="`Dissolve “${menuGroupTitle}” — its terminal groups stay, ungrouped`"
          @click="run(() => emit('deleteGroup', menuGroupKey))"
        >
          <span class="cgr-pop-label">Delete group</span>
        </button>
      </template>

      <template v-else-if="overlay.kind === 'pane'">
        <button
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => emit('closePane', menuPaneKey))"
        >
          <span class="cgr-pop-label">Close pane</span><span class="cgr-pop-key">{{ keys?.closePane }}</span>
        </button>
        <button
          v-if="menuPaneCount > 1"
          data-item class="cgr-pop-item" role="menuitem"
          @click="run(() => closeOtherPanes(menuTabKey, menuPaneKey))"
        >
          <span class="cgr-pop-label">Close other panes</span>
        </button>
      </template>

      <template v-else>
        <p class="cgr-pop-head">View as</p>
        <div class="cgr-seg" role="group" aria-label="View as">
          <button
            v-for="option in (['panes', 'tabs'] as const)"
            :key="option"
            data-item
            class="cgr-seg-btn"
            :class="{ 'is-on': display.view === option }"
            :aria-pressed="display.view === option"
            @click="setDisplay('view', option)"
          >{{ option === 'panes' ? 'Panes' : 'Tabs' }}</button>
        </div>

        <p class="cgr-pop-head">Density</p>
        <div class="cgr-seg" role="group" aria-label="Density">
          <button
            v-for="option in (['compact', 'expanded'] as const)"
            :key="option"
            data-item
            class="cgr-seg-btn"
            :class="{ 'is-on': display.density === option }"
            :aria-pressed="display.density === option"
            @click="setDisplay('density', option)"
          >{{ option === 'compact' ? 'Compact' : 'Expanded' }}</button>
        </div>

        <p class="cgr-pop-head">Pane title</p>
        <div class="cgr-seg" role="group" aria-label="Pane title">
          <button
            v-for="option in (['command', 'directory'] as const)"
            :key="option"
            data-item
            class="cgr-seg-btn"
            :class="{ 'is-on': display.title === option }"
            :aria-pressed="display.title === option"
            @click="setDisplay('title', option)"
          >{{ option === 'command' ? 'Command' : 'Directory' }}</button>
        </div>

        <p class="cgr-pop-head">Subtitle</p>
        <div class="cgr-seg" role="group" aria-label="Subtitle">
          <button
            v-for="option in (['path', 'shell'] as const)"
            :key="option"
            data-item
            class="cgr-seg-btn"
            :class="{ 'is-on': display.subtitle === option }"
            :aria-pressed="display.subtitle === option"
            @click="setDisplay('subtitle', option)"
          >{{ option === 'path' ? 'Path' : 'Shell' }}</button>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.cgr {
  position: relative;
  /* Rows re-lay themselves out against the sidebar's own width, which the user
     drags down to 120px. A media query would be answering the wrong question. */
  container-type: inline-size;
  container-name: cgr;
  display: flex;
  flex-direction: column;
  min-height: 100%;
  padding-bottom: 6px;
}

/* --------------------------------------------------------------------- bar -- */
.cgr-bar {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px 8px;
  background: var(--qss-bg-panel, var(--qss-bg));
}

.cgr-bar-ico {
  position: absolute;
  left: 16px;
  display: grid;
  place-items: center;
  width: 14px;
  height: 14px;
  color: var(--qss-text-muted);
  pointer-events: none;
}
.cgr-bar-ico svg {
  width: 14px;
  height: 14px;
}

.cgr-search {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 6px 0 26px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-input, var(--qss-bg-raised));
  color: var(--qss-text);
  font-family: var(--qss-font-sans);
  font-size: 12px;
  outline: none;
}
.cgr-search::placeholder {
  color: var(--qss-text-muted);
}
.cgr-search:focus {
  border-color: var(--qss-accent);
}
.cgr-search:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}

.cgr-bar-btn {
  flex: none;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary);
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-bar-btn:hover:not(:disabled),
.cgr-bar-btn[aria-expanded='true'] {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.cgr-bar-btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.cgr-bar-btn svg {
  width: 14px;
  height: 14px;
}

.cgr-note {
  padding: 6px 12px;
  font-size: 12px;
  color: var(--qss-text-muted);
}

/* The empty-panel double-click target. */
.cgr-void {
  flex: 1;
  min-height: 20px;
}

/* ------------------------------------------------------------------ groups -- */
.cgr-group {
  margin-bottom: 8px;
}
.cgr-group.is-dragging {
  opacity: 0.5;
}
.cgr-group.is-drop .cgr-row-head {
  box-shadow: inset 0 2px 0 0 var(--qss-accent);
}

/* ── the folder level ── */

/* A section is a folder plus its terminal groups (or the ungrouped tail) —
   and, mid-drag, the drop area for filing a terminal group here. */
.cgr-section {
  border-radius: 6px;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    box-shadow var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-section.is-drop {
  background: color-mix(in srgb, var(--qss-accent) 7%, transparent);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--qss-accent) 45%, transparent);
}

.cgr-folder {
  position: relative;
  display: flex;
  align-items: center;
  gap: 2px;
  width: calc(100% - 16px);
  margin: 10px 8px 2px;
  padding: 2px 2px 2px 0;
  border-radius: 4px;
}
.cgr-folder.is-dragging {
  opacity: 0.45;
}
.cgr-folder.is-reorder {
  box-shadow: inset 0 2px 0 0 var(--qss-accent);
}
.cgr-folder-twist {
  flex: none;
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  border: 0;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  padding: 0;
}
.cgr-folder-twist svg { width: 12px; height: 12px; }
.cgr-folder-twist:focus-visible,
.cgr-folder-name:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}
.cgr-folder-name {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  border: 0;
  background: transparent;
  padding: 2px 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
  font: 600 10.5px/1.2 var(--qss-font-sans);
  letter-spacing: 0.07em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.cgr-folder:hover .cgr-folder-name { color: var(--qss-text-secondary); }
.cgr-folder .cgr-tools { position: static; opacity: 0; }
.cgr-folder:hover .cgr-tools,
.cgr-folder:focus-within .cgr-tools { opacity: 1; }

/* Terminal groups inside a folder step in, so the level reads at a glance. */
.cgr-group.in-folder {
  margin-left: 10px;
  width: calc(100% - 10px);
}

.cgr-note.is-folder-empty {
  margin: 0 8px 4px 26px;
  font-size: 10.5px;
}

/* The founding zone. Appears only mid-drag, styled like the folder heading it
   is about to become rather than a construction-site dashed box. */
.cgr-newgroup {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin: 8px;
  padding: 8px;
  border: 1px dashed var(--qss-border-subtle);
  border-radius: 6px;
  color: var(--qss-text-muted);
  font: 600 10.5px/1.2 var(--qss-font-sans);
  letter-spacing: 0.07em;
  text-transform: uppercase;
  transition: border-color var(--qss-dur-instant) var(--qss-ease-out),
    background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-newgroup svg {
  width: 14px;
  height: 14px;
}
.cgr-newgroup.is-armed {
  border-color: color-mix(in srgb, var(--qss-accent) 60%, transparent);
  background: color-mix(in srgb, var(--qss-accent) 8%, transparent);
  color: var(--qss-text);
}

/* One row = one main button plus its hover tools. The tools float over the
   right edge instead of sitting in the flow, so the title keeps the row's full
   width at every sidebar size and nothing reflows when the pointer arrives. */
/* At rest the rows are one slab divided by hairlines; only the row under the
   pointer or under the selection detaches into a rounded card. That is the
   reference's treatment, and the reason it works is that a list of forty rounded
   cards has forty edges competing for the eye, while a slab has one. */
.cgr-row {
  position: relative;
  display: flex;
  align-items: center;
  width: calc(100% - 16px);
  margin: 0 8px;
  border: 1px solid transparent;
  border-bottom-color: var(--qss-border-subtle);
  border-radius: 0;
  /* Hover fade (§3.4) — rows light up, they do not snap. */
  transition: background var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-row:hover,
.cgr-row:focus-within,
.cgr-row.is-active {
  border-color: transparent;
  border-radius: 4px;
}
/* The last row of a group closes the slab. */
.cgr-group > .cgr-row:last-child {
  border-bottom-color: transparent;
}

.cgr-row-head {
  border-bottom-color: transparent;
}
.cgr-row-head:hover {
  background: var(--qss-bg-hover);
}
/* Keyboard focus lights the row the way hover does, so the fade behind the
   revealed controls has the same ground under it either way. */
.cgr-row:focus-within {
  background: var(--qss-bg-hover);
}

/* Every control in here is a real button, so every control in here gets a real
   focus ring — the list is the only way to reach a session, and the keyboard
   has to be able to see where it is. */
.cgr-head:focus-visible,
.cgr-pane:focus-visible,
.cgr-tool:focus-visible,
.cgr-twist:focus-visible,
.cgr-bar-btn:focus-visible,
.cgr-seg-btn:focus-visible,
.cgr-swatch:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}

.cgr-head,
.cgr-pane,
.cgr-rename {
  min-width: 0;
  border: none;
  background: transparent;
  font-family: var(--qss-font-sans);
  text-align: left;
  cursor: pointer;
}

/* The heading is a label, not a button-looking thing: it is muted and small, and
   only the rows underneath it are meant to look clickable. */
.cgr-head {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 4px 3px 0;
  color: var(--qss-text-muted);
  font-family: var(--qss-font-mono);
  font-size: 11px;
  border-radius: 4px;
}
.cgr-row-head:hover .cgr-head {
  color: var(--qss-text-secondary);
}
.cgr-row-head.is-active .cgr-head {
  color: var(--qss-text);
}
.cgr-row-head.is-active.has-tint .cgr-head {
  color: var(--cgr-tint);
}
.cgr-row-head.has-tint {
  box-shadow: inset 2px 0 0 0 var(--cgr-tint);
}

.cgr-head-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cgr-count {
  flex: none;
  padding: 0 4px;
  border-radius: 999px;
  background: var(--qss-bg-raised);
  font-size: 10px;
  line-height: 14px;
}

.cgr-twist {
  flex: none;
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.cgr-twist:hover {
  color: var(--qss-text);
}
.cgr-twist-svg {
  width: 14px;
  height: 14px;
  /* dur-fast: the collapse chevron is structural motion, not a hover fade —
     same speed as the block header's chevron (§3.4). */
  transition: transform var(--qss-dur-fast) var(--qss-ease-out);
}
.cgr-twist-svg.is-open {
  transform: rotate(90deg);
}

.cgr-rename {
  flex: 1;
  width: calc(100% - 16px);
  margin: 0 8px 2px;
  padding: 3px 6px;
  border: 1px solid var(--qss-accent);
  border-radius: 4px;
  background: var(--qss-bg-input, var(--qss-bg-raised));
  color: var(--qss-text);
  font-family: var(--qss-font-mono);
  font-size: 11px;
  outline: none;
}

/* ------------------------------------------------------------------- panes -- */
.cgr-pane {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 5px 6px;
  border-radius: 4px;
  color: var(--qss-text-secondary);
}
.cgr-row-pane:hover {
  background: var(--qss-bg-hover);
}
.cgr-row-pane.is-active {
  background: var(--qss-bg-card);
  box-shadow: inset 2px 0 0 0 var(--cgr-tint, var(--qss-accent));
}
.cgr-row-pane.is-exited {
  opacity: 0.55;
}

/* Circular, as Warp draws it, and with the badge hung off its corner: the state
   belongs on the object it describes, not in a column of its own that a 120px
   sidebar cannot afford. */
.cgr-mark {
  position: relative;
  flex: none;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: 1px solid var(--qss-border);
  border-radius: 50%;
  background: var(--qss-bg-raised);
  color: var(--qss-text-muted);
}
.cgr-mark > svg {
  width: 12px;
  height: 12px;
}
.cgr-row-pane.is-active .cgr-mark {
  border-color: var(--cgr-tint, var(--qss-accent));
  color: var(--cgr-tint, var(--qss-accent));
}

.cgr-badge {
  position: absolute;
  right: -3px;
  bottom: -3px;
  display: grid;
  place-items: center;
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: var(--qss-bg);
  box-shadow: 0 0 0 1px var(--qss-bg);
}
.cgr-badge svg {
  width: 9px;
  height: 9px;
}
.cgr-badge.is-inline {
  position: static;
  width: 11px;
  height: 11px;
  background: transparent;
  box-shadow: none;
}
.cgr-badge.is-run {
  color: var(--qss-accent);
}
.cgr-badge.is-ok {
  color: var(--qss-success);
}
.cgr-badge.is-warn {
  color: var(--qss-warning);
}
.cgr-badge.is-wait {
  color: var(--qss-text-muted);
}
.cgr-badge.is-off {
  color: var(--qss-text-muted);
}

.cgr-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.cgr-title {
  overflow: hidden;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text-secondary);
}
.cgr-row-pane.is-active .cgr-title {
  color: var(--qss-text);
}

/* The subtitle sits at the title's size and face; the reference separates the
   two by colour alone, and a smaller face made the second line read as a
   footnote rather than as the other half of the row. `cgr-detail` is the third
   line of the expanded density and stays smaller on purpose. */
.cgr-sub,
.cgr-detail {
  overflow: hidden;
  font-size: 12px;
  color: var(--qss-text-muted);
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cgr-detail {
  font-family: var(--qss-font-mono);
  font-size: 10px;
}

/* Unread activity, at the row's right edge. It sits under the hover tools by
   design: while the pointer is on the row the controls are the useful thing, and
   the moment the pane is focused the dot is gone anyway. */
.cgr-dot {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--qss-accent);
}

/* ------------------------------------------------------------------- tools -- */
.cgr-tools {
  position: absolute;
  right: 2px;
  display: flex;
  align-items: center;
  gap: 1px;
  padding-left: 10px;
  /* The fade keeps a long title from running visibly under the buttons. */
  background: linear-gradient(to right, transparent, var(--qss-bg-hover) 10px);
  border-radius: 4px;
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-row:hover .cgr-tools,
.cgr-row:focus-within .cgr-tools {
  opacity: 1;
  pointer-events: auto;
}
.cgr-row-pane.is-active .cgr-tools {
  background: linear-gradient(to right, transparent, var(--qss-bg-card) 10px);
}

.cgr-tool {
  flex: none;
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-tool svg {
  width: 14px;
  height: 14px;
}
.cgr-tool:hover {
  background: var(--qss-bg-raised);
  color: var(--qss-text);
}
.cgr-tool.is-close:hover {
  color: var(--qss-error);
}

/* ---------------------------------------------------------------- overlays -- */
.cgr-pop {
  position: absolute;
  z-index: 30;
  min-width: 0;
  /* Capped to the list rather than to a comfortable reading width: at the
     sidebar's 120px minimum a fixed-width menu would hang half of itself over
     the terminal, and a menu outside its panel is a menu you cannot use. */
  max-width: calc(100% - 12px);
  width: max-content;
  padding: 4px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-overlay);
  box-shadow: var(--qss-shadow-lg);
  font-family: var(--qss-font-sans);
  /* The menu rise (§3.4) — opened by a click, never at first paint. */
  animation: cgr-rise var(--qss-dur-fast) var(--qss-ease-out);
}

.cgr-pop-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-pop-item:hover,
.cgr-pop-item:focus-visible {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
/* The module ring (§3.5) instead of the outline:none this had — arrow-key
   navigation through the menu has to be visible. */
.cgr-pop-item:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: -1px;
}
.cgr-pop-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.cgr-pop-key {
  flex: none;
  color: var(--qss-text-muted);
  font-family: var(--qss-font-mono);
  font-size: 10px;
}
.cgr-pop-sep {
  height: 1px;
  margin: 4px 2px;
  background: var(--qss-border);
}
.cgr-pop-head {
  padding: 5px 8px 2px;
  color: var(--qss-text-muted);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.cgr-seg {
  display: flex;
  gap: 2px;
  padding: 0 4px 2px;
}
.cgr-seg-btn {
  flex: 1;
  min-width: 0;
  padding: 4px 6px;
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 11px;
  white-space: nowrap;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.cgr-seg-btn:hover {
  background: var(--qss-bg-hover);
}
.cgr-seg-btn.is-on {
  border-color: var(--qss-accent);
  background: var(--qss-accent-soft);
  color: var(--qss-text);
}

.cgr-swatches {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 4px;
}
.cgr-swatch {
  flex: none;
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 50%;
  background: var(--cgr-swatch, transparent);
  color: var(--qss-text-muted);
  cursor: pointer;
}
.cgr-swatch.is-none {
  border-color: var(--qss-border);
}
.cgr-swatch svg {
  width: 11px;
  height: 11px;
}
.cgr-swatch:hover,
.cgr-swatch.is-on {
  box-shadow: 0 0 0 2px var(--qss-bg-overlay), 0 0 0 3px var(--qss-accent);
}

/* ------------------------------------------------------------------ narrow -- */
/* At the sidebar's minimum a row has about fifty pixels of text left. Everything
   optional comes off — the margins, the mark's size, the third line — so what
   survives is the one thing that identifies the pane. */
@container cgr (max-width: 168px) {
  .cgr-row,
  .cgr-rename {
    width: calc(100% - 8px);
    margin-left: 4px;
    margin-right: 4px;
  }
  .cgr-pane {
    gap: 5px;
    padding: 5px 4px;
  }
  .cgr-mark {
    width: 18px;
    height: 18px;
  }
  .cgr-mark > svg {
    width: 10px;
    height: 10px;
  }
  .cgr-badge {
    width: 9px;
    height: 9px;
    right: -2px;
    bottom: -2px;
  }
  .cgr-badge svg {
    width: 8px;
    height: 8px;
  }
  .cgr-detail {
    display: none;
  }
  .cgr-bar {
    padding: 6px 4px 8px;
  }
  .cgr-bar-ico {
    left: 12px;
  }
}

/* The menu rise (§3.4): 4px up plus fade, transform/opacity only. */
@keyframes cgr-rise {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}

@media (prefers-reduced-motion: reduce) {
  .cgr-tools,
  .cgr-twist-svg {
    transition: none;
  }
  .cgr-pop {
    animation: none;
  }
}
</style>
