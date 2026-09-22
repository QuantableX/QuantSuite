/**
 * QuantConsole — tabs, splits and pane slots for plain xterm terminals.
 *
 * Plain-terminal edition (2026-08-20 rollback): a pane is a PTY session in an
 * xterm, nothing more. The store owns the multiplexer — the tab list, each
 * tab's split tree, and one slot per pane — plus the per-workspace session
 * memory and the adoption of sessions that survived a webview reload.
 *
 * Two id spaces on purpose: `PaneNode.id` addresses a position in a tree,
 * `PaneSlot.key` a session slot. A split rebuilds the tree without touching a
 * running shell.
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { qs, type ConsoleSession, type ConsoleShell } from '@quantsuite/core'
import type { PaneNode, PaneSplit, SplitDirection } from '../types/multiplex'

export interface PaneSlot {
  key: string
  /** The live session, once the pane reported it opened. */
  sessionId: string | null
  shellPath: string | null
  shellLabel: string
  cwd: string
  exited: boolean
}

/**
 * A group — the sidebar's top level: a named folder of terminal groups. Purely
 * organisational; it owns no sessions and no tree.
 */
export interface ConsoleGroup {
  key: string
  title: string
}

export interface ConsoleTab {
  key: string
  /** Renamed by the user; `null` means "derive it from the pane". */
  title: string | null
  /** The group this terminal group is filed under; `null` = ungrouped. */
  groupKey: string | null
  /**
   * `'shell'` is a tab of PTY panes. `'suite'` is the read-only view of the
   * suite's own processes — it has no tree, because there is nothing to split
   * and nothing to type into.
   */
  kind: 'shell' | 'suite'
  root: PaneNode | null
  activePaneKey: string
  /** One leaf fills the tab; `null` is the normal layout. */
  zoomedPaneKey: string | null
}

export interface ConsoleTabView {
  key: string
  title: string
}

// ── tree helpers ─────────────────────────────────────────────────────────────
//
// Pure and returning new nodes rather than mutating: a tree edit that half
// succeeds is much harder to see than one that replaces the root, and Pinia
// re-renders on the assignment either way.

function leafKeys(node: PaneNode): string[] {
  return node.kind === 'leaf' ? [node.paneKey] : node.children.flatMap(leafKeys)
}

function countLeaves(node: PaneNode): number {
  return node.kind === 'leaf' ? 1 : node.children.reduce((n, child) => n + countLeaves(child), 0)
}

/**
 * Split the leaf showing `paneKey` in two.
 *
 * `before` is what makes "split left" and "split up" different from their
 * mirrors: the axis is the same, only the side the new pane lands on changes.
 */
function splitAt(
  node: PaneNode,
  paneKey: string,
  dir: SplitDirection,
  newPaneKey: string,
  nodeId: () => string,
  before = false
): PaneNode {
  if (node.kind === 'leaf') {
    if (node.paneKey !== paneKey) return node
    const fresh: PaneNode = { kind: 'leaf', id: nodeId(), paneKey: newPaneKey }
    return {
      kind: 'split',
      id: nodeId(),
      dir,
      children: before ? [fresh, node] : [node, fresh],
      sizes: [0.5, 0.5],
    }
  }
  return {
    ...node,
    children: node.children.map((child) =>
      splitAt(child, paneKey, dir, newPaneKey, nodeId, before)
    ),
  }
}

/**
 * Remove the leaf showing `paneKey`. A split left with one child collapses into
 * that child — without this, closing panes leaves a tower of one-child splits
 * that each still draw a divider.
 */
function removeLeaf(node: PaneNode, paneKey: string): PaneNode | null {
  if (node.kind === 'leaf') return node.paneKey === paneKey ? null : node

  // Survivors are collected with their original share in one pass. Re-deriving
  // "which children survived" by comparing the rebuilt nodes cannot work — they
  // are new objects — and that comparison silently kept every size at 1/n.
  const survivors: { child: PaneNode; size: number }[] = []
  node.children.forEach((child, index) => {
    const next = removeLeaf(child, paneKey)
    if (next) {
      survivors.push({ child: next, size: node.sizes[index] ?? 1 / node.children.length })
    }
  })

  if (!survivors.length) return null
  const only = survivors[0]
  if (survivors.length === 1 && only) return only.child

  // Redistribute the departed share proportionally: the survivors keep their
  // relative widths instead of snapping back to equal.
  const total = survivors.reduce((sum, entry) => sum + entry.size, 0) || 1
  return {
    ...node,
    children: survivors.map((entry) => entry.child),
    sizes: survivors.map((entry) => entry.size / total),
  }
}

function withSizes(node: PaneNode, id: string, sizes: number[]): PaneNode {
  if (node.kind === 'leaf') return node
  if (node.id === id) return { ...node, sizes }
  return { ...node, children: node.children.map((child) => withSizes(child, id, sizes)) }
}

function hasNode(node: PaneNode, id: string): boolean {
  if (node.id === id) return true
  return node.kind === 'split' && node.children.some((child) => hasNode(child, id))
}

/** A split's fractions, normalized, with equal shares for anything malformed. */
function fractionsOf(node: PaneSplit): number[] {
  const count = node.children.length
  const equal = Array.from({ length: count }, () => 1 / count)
  const raw = node.sizes
  if (!Array.isArray(raw) || raw.length !== count) return equal
  let sum = 0
  for (const value of raw) {
    if (!Number.isFinite(value) || value <= 0) return equal
    sum += value
  }
  return raw.map((value) => value / sum)
}

/** Where a leaf sits, in fractions of the tab's box. */
interface LeafRect {
  paneKey: string
  x: number
  y: number
  w: number
  h: number
}

/** The divider's fixed thickness, mirrored from `SplitTree.vue`. */
const DIVIDER_PX = 4

/**
 * A leaf's box in the units CSS actually needs: a percentage of the tab plus a
 * pixel correction for the dividers between here and the root.
 */
export interface PaneRect {
  paneKey: string
  x: number
  xPx: number
  y: number
  yPx: number
  w: number
  wPx: number
  h: number
  hPx: number
}

type Box = Omit<PaneRect, 'paneKey'>

/**
 * The same walk as `leafRects`, carrying the dividers along.
 *
 * A split hands its children `basis: 0; grow: <fraction>`, so the dividers take
 * their pixels out of the box *before* the fractions divide what is left. Child
 * `i` is therefore `f_i` of `(box − (n−1) × 4px)`, offset by the `i` dividers in
 * front of it — which is exactly the arithmetic below, kept in two parts so the
 * result stays exact at any container width instead of at one measured one.
 */
function paneBoxes(node: PaneNode, box: Box, out: PaneRect[]) {
  if (node.kind === 'leaf') {
    out.push({ paneKey: node.paneKey, ...box })
    return
  }
  const fractions = fractionsOf(node)
  const gaps = (node.children.length - 1) * DIVIDER_PX
  const row = node.dir === 'row'
  const span = row ? box.w : box.h
  const spanPx = (row ? box.wPx : box.hPx) - gaps
  // One accumulator for both parts: a child's pixel width is `f * spanPx`, so
  // the pixels in front of it are the same running sum of fractions.
  let offset = 0

  node.children.forEach((child, index) => {
    const fraction = fractions[index] ?? 1 / node.children.length
    const start = (row ? box.x : box.y) + offset * span
    const startPx = (row ? box.xPx : box.yPx) + offset * spanPx + index * DIVIDER_PX
    const next: Box = row
      ? { ...box, x: start, xPx: startPx, w: fraction * span, wPx: fraction * spanPx }
      : { ...box, y: start, yPx: startPx, h: fraction * span, hPx: fraction * spanPx }
    paneBoxes(child, next, out)
    offset += fraction
  })
}

/**
 * The layout as rectangles, derived from the tree rather than measured.
 *
 * Directional pane movement has to agree with what the eye sees, and the eye
 * sees geometry — no walk of tree *order* produces it. Reading the DOM would
 * give the same answer, but only for the tab that is on screen: a background
 * tab measures 0×0, and this has to work the moment one is switched to.
 */
function leafRects(node: PaneNode, x: number, y: number, w: number, h: number, out: LeafRect[]) {
  if (node.kind === 'leaf') {
    out.push({ paneKey: node.paneKey, x, y, w, h })
    return
  }
  const fractions = fractionsOf(node)
  let offset = 0
  node.children.forEach((child, index) => {
    const share = fractions[index] ?? 0
    if (node.dir === 'row') leafRects(child, x + offset * w, y, share * w, h, out)
    else leafRects(child, x, y + offset * h, w, share * h, out)
    offset += share
  })
}

/** How long a closed tab can still be reopened. */
const UNDO_CLOSE_MS = 60_000

/**
 * A tab that was closed, kept just long enough to take back. The shells are
 * gone — closing a tab kills them — so this stores what it takes to open the
 * same *layout* again: the tree, and each pane's shell and directory.
 */
interface ClosedTab {
  title: string | null
  groupKey: string | null
  kind: ConsoleTab['kind']
  root: PaneNode | null
  /** The slots as they were, by the pane key the tree above refers to. */
  slots: Record<string, RecordedSlot>
  at: number
}

/** A pane as something that can be written down: its shell and its directory. */
export interface RecordedSlot {
  shellPath: string | null
  shellLabel: string
  cwd: string
}

/** One terminal group as the session memory stores it. */
export interface RecordedTab {
  title: string | null
  root: PaneNode | null
  slots: Record<string, RecordedSlot>
  /** Which folder it was filed under; optional so older layouts still load. */
  groupKey?: string | null
}

/**
 * Everything the console remembers about one QuantSuite workspace.
 *
 * Versioned because it is persisted: a shape change has to be able to say
 * "this is not mine" and start clean, rather than restore half a layout.
 * Version 2 is the plain-terminal shape (v1 carried block-era fields).
 */
export interface ConsoleLayout {
  version: 2
  tabs: RecordedTab[]
  /** Which terminal group was in front, as an index into `tabs`. */
  activeIndex: number
  /** The named folders, in order; optional so older layouts still load. */
  groups?: ConsoleGroup[]
}

export const useConsoleStore = defineStore('console/console', () => {
  const tabs = ref<ConsoleTab[]>([])
  /** The named folders of the sidebar's top level — see `ConsoleGroup`. */
  const groups = ref<ConsoleGroup[]>([])
  const activeTabKey = ref<string | null>(null)
  const panes = ref<Record<string, PaneSlot>>({})
  /** Surfaced by the page's bottom bar. */
  const error = ref<string | null>(null)
  /**
   * The undo-close stack, newest last. Plain state rather than a `ref`: nothing
   * renders it, and making it reactive would re-run every subscriber each time
   * a tab closes to show the same screen.
   */
  const closed: ClosedTab[] = []

  let counter = 0
  const nextId = (prefix: string) => {
    counter += 1
    return `${prefix}-${counter}`
  }

  const activeTab = computed(() => tabs.value.find((t) => t.key === activeTabKey.value) ?? null)

  /** A terminal group opened from inside a group stays in that group. */
  const inheritGroup = () =>
    activeTab.value?.kind === 'shell' ? activeTab.value.groupKey : null
  const activePane = computed(() => {
    const tab = activeTab.value
    return tab ? panes.value[tab.activePaneKey] ?? null : null
  })

  /** Every slot in a tab, in tree order. */
  const tabPanes = (tab: ConsoleTab): PaneSlot[] =>
    (tab.root ? leafKeys(tab.root) : [])
      .map((key) => panes.value[key])
      .filter((slot): slot is PaneSlot => !!slot)

  const tabViews = computed<ConsoleTabView[]>(() =>
    tabs.value.map((tab) => {
      if (tab.title) return { key: tab.key, title: tab.title }
      if (tab.kind === 'suite') return { key: tab.key, title: 'Suite processes' }
      const slots = tabPanes(tab)
      const lead = panes.value[tab.activePaneKey] ?? slots[0]
      return {
        key: tab.key,
        title: lead ? `${lead.shellLabel}${slots.length > 1 ? ` (${slots.length})` : ''}` : 'Session',
      }
    })
  )

  function addSlot(shell: ConsoleShell | null, cwd: string, sessionId: string | null = null): PaneSlot {
    const slot: PaneSlot = {
      key: nextId('pane'),
      sessionId,
      shellPath: shell?.path ?? null,
      shellLabel: shell?.label ?? 'Shell',
      cwd,
      exited: false,
    }
    panes.value = { ...panes.value, [slot.key]: slot }
    return slot
  }

  function createTab(shell: ConsoleShell | null, cwd: string, sessionId: string | null = null): ConsoleTab {
    const slot = addSlot(shell, cwd, sessionId)
    const tab: ConsoleTab = {
      key: nextId('tab'),
      title: null,
      groupKey: inheritGroup(),
      kind: 'shell',
      root: { kind: 'leaf', id: nextId('node'), paneKey: slot.key },
      activePaneKey: slot.key,
      zoomedPaneKey: null,
    }
    tabs.value = [...tabs.value, tab]
    activeTabKey.value = tab.key
    error.value = null
    return tab
  }

  /**
   * The suite's own processes as a tab. Idempotent: a second request focuses
   * the tab that exists rather than stacking identical views of one register.
   */
  function openSuiteTab(): ConsoleTab {
    const existing = tabs.value.find((t) => t.kind === 'suite')
    if (existing) {
      activeTabKey.value = existing.key
      return existing
    }
    const tab: ConsoleTab = {
      key: nextId('tab'),
      title: 'Suite processes',
      groupKey: null,
      kind: 'suite',
      root: null,
      activePaneKey: '',
      zoomedPaneKey: null,
    }
    tabs.value = [...tabs.value, tab]
    activeTabKey.value = tab.key
    return tab
  }

  /** Split the active pane. The new pane inherits the shell and the cwd — a
   * split that lands you somewhere else is a split you have to fix by hand. */
  function splitActive(dir: SplitDirection, before = false) {
    const tab = activeTab.value
    if (!tab?.root) return
    const source = panes.value[tab.activePaneKey]
    if (!source) return

    const slot = addSlot(
      source.shellPath
        ? { id: source.shellPath, label: source.shellLabel, path: source.shellPath, isDefault: false }
        : null,
      source.cwd
    )
    tab.root = splitAt(tab.root, tab.activePaneKey, dir, slot.key, () => nextId('node'), before)
    tab.activePaneKey = slot.key
    // A zoomed tab that splits would hide the pane it just created.
    tab.zoomedPaneKey = null
  }

  async function killSession(slot: PaneSlot | undefined) {
    if (!slot?.sessionId) return
    try {
      await qs.console.close(slot.sessionId)
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
    }
  }

  async function closePane(paneKey: string) {
    const tab = tabs.value.find((t) => t.root && leafKeys(t.root).includes(paneKey))
    if (!tab?.root) return
    const slot = panes.value[paneKey]

    const next = removeLeaf(tab.root, paneKey)
    if (!next) {
      await closeTab(tab.key)
      return
    }

    tab.root = next
    if (tab.activePaneKey === paneKey) tab.activePaneKey = leafKeys(next)[0] ?? tab.activePaneKey
    if (tab.zoomedPaneKey === paneKey) tab.zoomedPaneKey = null

    const { [paneKey]: _removed, ...rest } = panes.value
    panes.value = rest
    await killSession(slot)
  }

  async function closeTab(key: string) {
    const index = tabs.value.findIndex((t) => t.key === key)
    if (index === -1) return
    const tab = tabs.value[index]
    if (!tab) return

    const keys = tab.root ? leafKeys(tab.root) : []
    tabs.value = tabs.value.filter((t) => t.key !== key)
    if (activeTabKey.value === key) {
      activeTabKey.value = tabs.value[Math.min(index, tabs.value.length - 1)]?.key ?? null
    }

    const remaining = { ...panes.value }
    const doomed = keys.map((paneKey) => remaining[paneKey])
    for (const paneKey of keys) delete remaining[paneKey]
    panes.value = remaining

    const slots: Record<string, RecordedSlot> = {}
    keys.forEach((paneKey, at) => {
      const slot = doomed[at]
      if (slot) {
        slots[paneKey] = { shellPath: slot.shellPath, shellLabel: slot.shellLabel, cwd: slot.cwd }
      }
    })
    closed.push({ title: tab.title, groupKey: tab.groupKey, kind: tab.kind, root: tab.root, slots, at: Date.now() })
    // Bounded: a session of opening and closing tabs would otherwise hold every
    // slot it ever made alive for the lifetime of the window.
    if (closed.length > 16) closed.shift()

    for (const slot of doomed) await killSession(slot)
  }

  /**
   * Take back the last tab closed within the undo window.
   *
   * The window is deliberate: an undo with no expiry turns into a second
   * history, and "reopen closed tab" half an hour later would resurrect a
   * directory the user has long left.
   */
  function reopenClosedTab(): ConsoleTab | null {
    let entry = closed.pop()
    while (entry && Date.now() - entry.at > UNDO_CLOSE_MS) entry = closed.pop()
    if (!entry) return null
    if (entry.kind === 'suite') return openSuiteTab()
    return rebuildTab(entry.title, entry.root, entry.slots, entry.groupKey)
  }

  /**
   * Open one group from a description of its panes — a closed tab being taken
   * back, or a group the session memory recorded before the last shutdown.
   *
   * New slots, new node ids, same shape: the tree is rebuilt against a map from
   * the recorded pane keys, because both id spaces are per-window counters and
   * reusing them would collide with panes opened since. Sessions that survived
   * a webview reload are not this function's concern — the page adopts those
   * instead of restoring (see `adoptRunningSessions`).
   */
  function rebuildTab(
    title: string | null,
    layout: PaneNode | null,
    slots: Record<string, RecordedSlot>,
    groupKey: string | null = null
  ): ConsoleTab | null {
    // Checked before any slot is made, so a refusal leaves no orphans behind.
    if (!layout || !Object.keys(slots).length) return null

    const remap = new Map<string, string>()
    for (const [oldKey, slot] of Object.entries(slots)) {
      const shell = slot.shellPath
        ? { id: slot.shellPath, label: slot.shellLabel, path: slot.shellPath, isDefault: false }
        : null
      remap.set(oldKey, addSlot(shell, slot.cwd).key)
    }

    const rebuild = (node: PaneNode): PaneNode | null => {
      if (node.kind === 'leaf') {
        const paneKey = remap.get(node.paneKey)
        return paneKey ? { kind: 'leaf', id: nextId('node'), paneKey } : null
      }
      const children: PaneNode[] = []
      const sizes: number[] = []
      const fractions = fractionsOf(node)
      node.children.forEach((child, index) => {
        const next = rebuild(child)
        if (!next) return
        children.push(next)
        sizes.push(fractions[index] ?? 0)
      })
      if (!children.length) return null
      const first = children[0]
      if (children.length === 1 && first) return first
      const total = sizes.reduce((sum, value) => sum + value, 0) || 1
      return { ...node, id: nextId('node'), children, sizes: sizes.map((value) => value / total) }
    }

    const root = rebuild(layout)
    const paneKey = root ? leafKeys(root)[0] : undefined
    if (!root || !paneKey) return null

    const tab: ConsoleTab = {
      key: nextId('tab'),
      title,
      groupKey: groupKey ?? null,
      kind: 'shell',
      root,
      activePaneKey: paneKey,
      zoomedPaneKey: null,
    }
    tabs.value = [...tabs.value, tab]
    activeTabKey.value = tab.key
    error.value = null
    return tab
  }

  // ── session memory ─────────────────────────────────────────────────────────

  /**
   * The layout as the session memory writes it down: which groups existed, how
   * each was split, and for every pane its shell and directory. Live state is
   * deliberately absent — it describes a process that will not exist next time.
   * `suite` groups are skipped: that tab is a view of the *running* suite.
   */
  function snapshot(): ConsoleLayout {
    const shellTabs = tabs.value.filter((tab) => tab.kind === 'shell' && tab.root)
    const activeIndex = shellTabs.findIndex((tab) => tab.key === activeTabKey.value)
    return {
      version: 2,
      activeIndex: activeIndex < 0 ? 0 : activeIndex,
      tabs: shellTabs.map((tab) => {
        const slots: Record<string, RecordedSlot> = {}
        for (const key of tab.root ? leafKeys(tab.root) : []) {
          const slot = panes.value[key]
          if (!slot) continue
          slots[key] = { shellPath: slot.shellPath, shellLabel: slot.shellLabel, cwd: slot.cwd }
        }
        return { title: tab.title, root: tab.root, slots, groupKey: tab.groupKey }
      }),
      groups: groups.value.map((group) => ({ ...group })),
    }
  }

  /**
   * Reopen a recorded layout. Returns how many groups came back.
   *
   * A shell recorded under a path that no longer resolves falls back to the
   * default rather than dropping the pane: an empty group is harder to
   * understand than a group in the right directory with the wrong shell.
   */
  function restoreLayout(layout: ConsoleLayout | null, shells: ConsoleShell[]): number {
    if (!layout || layout.version !== 2 || !Array.isArray(layout.tabs)) return 0
    const available = new Set(shells.map((shell) => shell.path))
    const fallback = shells.find((shell) => shell.isDefault) ?? shells[0] ?? null

    // The folders first, keys remapped: both id spaces are per-window
    // counters, and a recorded key could collide with a group made since.
    const groupRemap = new Map<string, string>()
    for (const recorded of layout.groups ?? []) {
      if (typeof recorded?.title !== 'string' || typeof recorded?.key !== 'string') continue
      groupRemap.set(recorded.key, createGroup(recorded.title).key)
    }

    const keys: string[] = []
    for (const entry of layout.tabs) {
      const slots: Record<string, RecordedSlot> = {}
      for (const [key, slot] of Object.entries(entry.slots ?? {})) {
        const known = !!slot.shellPath && available.has(slot.shellPath)
        slots[key] = known
          ? slot
          : {
              ...slot,
              shellPath: fallback?.path ?? null,
              shellLabel: fallback?.label ?? slot.shellLabel,
            }
      }
      const tab = rebuildTab(
        entry.title ?? null,
        entry.root ?? null,
        slots,
        entry.groupKey ? groupRemap.get(entry.groupKey) ?? null : null
      )
      if (tab) keys.push(tab.key)
    }

    // Each `rebuildTab` focuses the group it opens, so without this the last
    // one would win and the group the user left in front would be behind.
    const active = keys[layout.activeIndex] ?? keys[0]
    if (active) activeTabKey.value = active
    return keys.length
  }

  function selectTab(key: string) {
    activeTabKey.value = key
  }

  /** Walk the tab list, wrapping — the far end of the strip is one key away. */
  function stepTab(step: -1 | 1) {
    if (!tabs.value.length) return
    const index = tabs.value.findIndex((t) => t.key === activeTabKey.value)
    const next = (index + step + tabs.value.length) % tabs.value.length
    const tab = tabs.value[next]
    if (tab) activeTabKey.value = tab.key
  }

  /**
   * The nth tab, 1-based; `0` is the last one. Out of range is a no-op rather
   * than a clamp: Ctrl+5 with three tabs open means "the fifth", and landing on
   * the third instead switches a tab the user did not ask for.
   */
  function selectTabAt(position: number) {
    const tab = position === 0 ? tabs.value[tabs.value.length - 1] : tabs.value[position - 1]
    if (tab) activeTabKey.value = tab.key
  }

  function renameTab(key: string, title: string) {
    const tab = tabs.value.find((t) => t.key === key)
    if (!tab) return
    const trimmed = title.trim()
    // An emptied title falls back to the derived one rather than showing blank.
    tab.title = trimmed.length ? trimmed : null
  }

  function reorderTabs(from: number, to: number) {
    const next = [...tabs.value]
    const [moved] = next.splice(from, 1)
    if (!moved) return
    next.splice(Math.max(0, Math.min(next.length, to)), 0, moved)
    tabs.value = next
  }

  /**
   * Move the active tab one place. It does **not** wrap: dragging a tab off the
   * left edge and having it appear on the right is a jump, not a move.
   */
  function moveActiveTab(step: -1 | 1) {
    const from = tabs.value.findIndex((t) => t.key === activeTabKey.value)
    if (from === -1) return
    const to = from + step
    if (to < 0 || to >= tabs.value.length) return
    reorderTabs(from, to)
  }

  function focusPane(paneKey: string) {
    const tab = tabs.value.find((t) => t.root && leafKeys(t.root).includes(paneKey))
    if (tab) {
      tab.activePaneKey = paneKey
      activeTabKey.value = tab.key
    }
  }

  // ── groups of terminal groups (the sidebar's top level) ────────────────────
  //
  // Three levels, the user's model (2026-08-20): a **group** is a named,
  // purely organisational folder; a **terminal group** (`ConsoleTab`) is one
  // split unit of terminals. Dragging a terminal group into a group files it
  // there — it stays its own split unit, nothing ever merges. Both levels are
  // renamable.

  /** Create a group; new terminal groups do NOT auto-join it. */
  function createGroup(title?: string): ConsoleGroup {
    const group: ConsoleGroup = {
      key: nextId('group'),
      title: title?.trim() || `Group ${groups.value.length + 1}`,
    }
    groups.value = [...groups.value, group]
    return group
  }

  function renameGroup(key: string, title: string) {
    const group = groups.value.find((g) => g.key === key)
    if (!group) return
    const trimmed = title.trim()
    if (trimmed) group.title = trimmed
  }

  /**
   * Dissolve a group. Its terminal groups survive as ungrouped — deleting a
   * folder must never take the shells inside with it.
   */
  function deleteGroup(key: string) {
    groups.value = groups.value.filter((g) => g.key !== key)
    for (const tab of tabs.value) {
      if (tab.groupKey === key) tab.groupKey = null
    }
  }

  /** Reorder the folders themselves — drag one heading onto another. */
  function reorderGroups(from: number, to: number) {
    const next = [...groups.value]
    const [moved] = next.splice(from, 1)
    if (!moved) return
    next.splice(Math.max(0, Math.min(next.length, to)), 0, moved)
    groups.value = next
  }

  /** File a terminal group under a group (`null` = ungrouped). */
  function moveTabToGroup(tabKey: string, groupKey: string | null) {
    const tab = tabs.value.find((t) => t.key === tabKey)
    if (!tab || tab.kind !== 'shell') return
    if (groupKey && !groups.value.some((g) => g.key === groupKey)) return
    tab.groupKey = groupKey
  }

  /** A new group founded by dropping a terminal group on the new-group zone. */
  function createGroupWith(tabKey: string): ConsoleGroup | null {
    const tab = tabs.value.find((t) => t.key === tabKey)
    if (!tab || tab.kind !== 'shell') return null
    const group = createGroup()
    tab.groupKey = group.key
    return group
  }

  /**
   * Step to the next pane **in reading order**, within this group.
   *
   * Reading order, not tree order: the tree records how the splits were made,
   * which is a history, and the history and the picture stop agreeing after
   * the second split. Sorting by row and then by column is the same rule
   * `focusDirection` already answers to.
   */
  function focusAdjacentPane(step: -1 | 1) {
    const tab = activeTab.value
    if (!tab?.root) return
    const rects: LeafRect[] = []
    leafRects(tab.root, 0, 0, 1, 1, rects)
    if (rects.length < 2) return

    // A row is a shared top edge. Compared with a tolerance because the
    // fractions come out of repeated division.
    const SAME_ROW = 1e-6
    const order = [...rects].sort((a, b) =>
      Math.abs(a.y - b.y) > SAME_ROW ? a.y - b.y : a.x - b.x
    )

    const index = order.findIndex((rect) => rect.paneKey === tab.activePaneKey)
    const next = order[(Math.max(0, index) + step + order.length) % order.length]
    if (next) tab.activePaneKey = next.paneKey
  }

  /**
   * Where every pane of a tab sits, as `calc(% + px)` along both axes.
   *
   * Published because the *page* lays the panes out from it: rendering panes
   * inside the split tree's leaf slot means a split moves each pane from one
   * component instance to another — Vue answers that by destroying and
   * rebuilding it, and a rebuilt pane loses its xterm.
   */
  function paneRects(tab: ConsoleTab): PaneRect[] {
    const rects: PaneRect[] = []
    if (tab.root) {
      paneBoxes(tab.root, { x: 0, xPx: 0, y: 0, yPx: 0, w: 1, wPx: 0, h: 1, hPx: 0 }, rects)
    }
    return rects
  }

  /**
   * Focus the pane that lies in `dir` from the focused one.
   *
   * A candidate has to be genuinely on that side (no overlap along the axis of
   * travel) and share some edge with the current pane across it; among those
   * the nearest wins, and ties go to the one that overlaps most.
   */
  function focusDirection(dir: 'left' | 'right' | 'up' | 'down') {
    const tab = activeTab.value
    if (!tab?.root) return
    const rects: LeafRect[] = []
    leafRects(tab.root, 0, 0, 1, 1, rects)
    const from = rects.find((rect) => rect.paneKey === tab.activePaneKey)
    if (!from) return

    const horizontal = dir === 'left' || dir === 'right'
    const backwards = dir === 'left' || dir === 'up'
    const along = (rect: LeafRect) => (horizontal ? rect.x : rect.y)
    const alongSize = (rect: LeafRect) => (horizontal ? rect.w : rect.h)
    const across = (rect: LeafRect) => (horizontal ? rect.y : rect.x)
    const acrossSize = (rect: LeafRect) => (horizontal ? rect.h : rect.w)
    // Fractions of the box, so "the same edge" is never exactly equal after
    // the divisions above.
    const EPSILON = 1e-6

    let best: LeafRect | null = null
    let bestScore = Infinity
    for (const rect of rects) {
      if (rect.paneKey === from.paneKey) continue
      const gap = backwards
        ? along(from) - (along(rect) + alongSize(rect))
        : along(rect) - (along(from) + alongSize(from))
      if (gap < -EPSILON) continue
      const overlap =
        Math.min(across(rect) + acrossSize(rect), across(from) + acrossSize(from)) -
        Math.max(across(rect), across(from))
      if (overlap <= EPSILON) continue
      const score = gap - overlap * 1e-3
      if (score < bestScore) {
        bestScore = score
        best = rect
      }
    }
    if (best) tab.activePaneKey = best.paneKey
  }

  function setSizes(nodeId: string, sizes: number[]) {
    // The tab that owns the node, not the active one: every tab is mounted, so
    // a divider can report from a group that is not the one on screen.
    const tab = tabs.value.find((t) => t.root && hasNode(t.root, nodeId))
    if (tab?.root) tab.root = withSizes(tab.root, nodeId, sizes)
  }

  function toggleZoom() {
    const tab = activeTab.value
    if (!tab?.root) return
    // Zooming a tab that has nothing to hide would just add a state to leave.
    if (countLeaves(tab.root) < 2) return
    tab.zoomedPaneKey = tab.zoomedPaneKey ? null : tab.activePaneKey
  }

  /** The pane reports what only it knows: the session's state. */
  function reportPaneState(
    paneKey: string,
    state: { cwd?: string; exited?: boolean; sessionId?: string }
  ) {
    const slot = panes.value[paneKey]
    if (!slot) return
    if (state.cwd) slot.cwd = state.cwd
    if (state.exited !== undefined) slot.exited = state.exited
    if (state.sessionId) slot.sessionId = state.sessionId
  }

  // ── adoption ───────────────────────────────────────────────────────────────
  //
  // What makes a webview reload or a module switch lossless: the PTYs live in
  // the plugin, so a frontend that lost its tabs can ask which sessions exist
  // and take them back — the pane re-attaches and replays what was buffered.
  //
  // Deliberately **not** a resurrection: a shell dies with the process, so
  // after an app restart there is nothing to adopt.

  /**
   * The sessions Rust still has, by id. Only the console's own: a canvas
   * window's shell lives in the same registry and is not this surface's to
   * claim.
   */
  async function liveSessions(): Promise<Map<string, ConsoleSession>> {
    try {
      const sessions = await qs.console.listSessions('console')
      return new Map(sessions.map((session) => [session.id, session]))
    } catch {
      // No bridge, or no registry yet. An empty map restores every pane as a
      // fresh shell, which is what a cold start is anyway.
      return new Map()
    }
  }

  /**
   * Sessions that belong to no pane yet, each in a group of its own. The
   * fallback, and only the fallback — the layout restore runs first, and
   * anything it claimed already has a slot.
   */
  async function adoptRunningSessions(shells: ConsoleShell[]) {
    let sessions: Awaited<ReturnType<typeof qs.console.listSessions>>
    try {
      sessions = await qs.console.listSessions('console')
    } catch {
      return 0
    }

    const known = new Set(Object.values(panes.value).map((slot) => slot.sessionId))
    let adopted = 0
    for (const session of sessions) {
      if (known.has(session.id)) continue
      const shell =
        shells.find((s) => s.path === session.shell) ??
        (session.shell
          ? {
              id: session.shell,
              label: session.shell.split(/[\\/]/).pop() ?? session.shell,
              path: session.shell,
              isDefault: false,
            }
          : null)
      const tab = createTab(shell, session.cwd, session.id)
      const slot = panes.value[tab.activePaneKey]
      if (slot) slot.exited = session.exited
      adopted += 1
    }
    return adopted
  }

  return {
    tabs,
    groups,
    createGroup,
    renameGroup,
    deleteGroup,
    reorderGroups,
    moveTabToGroup,
    createGroupWith,
    activeTabKey,
    panes,
    error,
    activeTab,
    activePane,
    tabViews,
    tabPanes,
    createTab,
    openSuiteTab,
    paneRects,
    splitActive,
    closePane,
    closeTab,
    reopenClosedTab,
    snapshot,
    restoreLayout,
    liveSessions,
    adoptRunningSessions,
    selectTab,
    stepTab,
    selectTabAt,
    renameTab,
    reorderTabs,
    moveActiveTab,
    focusPane,
    focusAdjacentPane,
    focusDirection,
    setSizes,
    toggleZoom,
    reportPaneState,
  }
})
