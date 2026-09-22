<script setup lang="ts">
/**
 * QuantConsole — the module page, plain-terminal edition (2026-08-20 rollback).
 *
 * Tabs and splits of plain xterm panes; the multiplexer's state lives in the
 * store, and each leaf of a tab's split tree renders one `ConsolePane`.
 *
 * **Every tab stays mounted**, hidden with `v-show`. Unmounting a tab would
 * throw away the xterm instance a program is drawing into. The panes are
 * rendered **flat, keyed by session slot** and positioned by the split tree's
 * computed rectangles: moving a component between instances makes Vue destroy
 * and rebuild it, and a rebuilt pane loses its terminal.
 */
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch, type CSSProperties } from 'vue'
import { bus, getActiveWorkspace, qs, useShortcuts, type ConsoleShell, type ShortcutBinding } from '@quantsuite/core'
import {
  useConsoleStore,
  type ConsoleLayout,
  type ConsoleTab,
  type PaneRect,
  type PaneSlot,
} from '../../stores/console'
import { CONSOLE_ACTIONS, chordsFor, parseBinding, prettyBinding } from '../../keymap'

const store = useConsoleStore()
const route = useRoute()

const shells = ref<ConsoleShell[]>([])
/** cwd for the *next* session; running ones keep whatever they were opened in. */
const cwd = ref('.')
/** Which QuantSuite workspace the session memory is filed under. */
const workspacePath = ref<string | null>(null)
/** Set once a restore (or the decision not to) has happened. */
const memoryReady = ref(false)
/**
 * Both panels of the module chrome: the left sidebar is the window groups and
 * is always open (it is this module's navigation); there is no right panel in
 * the plain edition.
 */
const sidebarWidth = ref(220)
const rightWidth = ref(220)
/** Both panels are toggled from the header wedges, the way every QuantSpace
 *  module does it (docs/PLAN-QUANTSPACE.md §4d). */
const explorerVisible = ref(true)
const groupsVisible = ref(true)
/** The footer dock — the SAME bar every QuantSpace module carries (CodeDock:
 *  Problems / Terminal / Search). `footerVisible` is its open flag; the tab
 *  in front is this module's own choice. */
const footerVisible = ref(false)
const footerTab = ref<'problems' | 'terminal' | 'search'>('terminal')
/** The module is on screen — the dock terminal must not draw while hidden. */
const moduleOnScreen = ref(true)
onActivated(() => (moduleOnScreen.value = true))
onDeactivated(() => (moduleOnScreen.value = false))

function onFooterPanel(panel: 'problems' | 'terminal' | 'search' | null) {
  if (panel) footerTab.value = panel
  else footerVisible.value = false
}
/** The header's file search — armed only while this module is on screen. */
const searchRef = ref<{ arm: () => void; disarm: () => void } | null>(null)
const toast = ref<string | null>(null)
let toastTimer: ReturnType<typeof setTimeout> | null = null
let offWorkspace: (() => void) | undefined
let offSetting: (() => void) | undefined
let saveTimer: ReturnType<typeof setTimeout> | null = null

/** `console / shell.default` — the shell a new session opens. */
const shellPreference = ref<string | null>(null)

/**
 * Which shell a new tab gets: the configured one when it still exists,
 * otherwise the platform default. A preference naming a shell that was
 * uninstalled falls through rather than failing.
 */
const defaultShell = computed(() => {
  const preferred = shellPreference.value
    ? shells.value.find((shell) => shell.id === shellPreference.value)
    : null
  return preferred ?? shells.value.find((shell) => shell.isDefault) ?? shells.value[0] ?? null
})

/** Whether the Tauri bridge is here at all (browser dev has none). */
function hasTauriBridge(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Shells for browser development, where nothing can be detected. */
const DEMO_SHELLS: ConsoleShell[] = [
  { id: 'demo', label: 'Demo shell', path: '', isDefault: true },
]

/** A pane is visible when its tab is on screen and zoom is not hiding it. */
function paneVisible(tabKey: string, paneKey: string): boolean {
  const tab = store.tabs.find((t) => t.key === tabKey)
  if (!tab || store.activeTabKey !== tabKey) return false
  return !tab.zoomedPaneKey || tab.zoomedPaneKey === paneKey
}

function paneOnStage(tab: ConsoleTab, paneKey: string): boolean {
  if (tab.key !== store.activeTabKey) return false
  return !tab.zoomedPaneKey || tab.zoomedPaneKey === paneKey
}

/**
 * Where a tile is drawn. A zoomed pane fills the tab. `right`/`bottom` are
 * explicit: an absolutely positioned box with all four edges plus a size is
 * over-constrained, and browsers resolve that in the writing direction.
 */
function tileStyle(tab: ConsoleTab, rect: PaneRect): CSSProperties {
  const zoomed = tab.zoomedPaneKey === rect.paneKey
  const box = zoomed ? { x: 0, xPx: 0, y: 0, yPx: 0, w: 1, wPx: 0, h: 1, hPx: 0 } : rect
  const calc = (fraction: number, px: number) =>
    `calc(${(fraction * 100).toFixed(4)}% + ${px.toFixed(2)}px)`
  return {
    left: calc(box.x, box.xPx),
    top: calc(box.y, box.yPx),
    width: calc(box.w, box.wPx),
    height: calc(box.h, box.hPx),
    right: 'auto',
    bottom: 'auto',
  }
}

/**
 * Panes per tab, for the group tree. Resolved here and passed as a prop: a
 * child rendered in a `QSidebar` slot is only updated when its **props**
 * change — see the note at the top of `Groups.vue`.
 */
const panesByTab = computed<Record<string, PaneSlot[]>>(() =>
  Object.fromEntries(store.tabs.map((tab) => [tab.key, store.tabPanes(tab)]))
)

/** Pane rectangles per tab, computed once per layout change. */
const rectsByTab = computed<Record<string, PaneRect[]>>(() =>
  Object.fromEntries(store.tabs.map((tab) => [tab.key, store.paneRects(tab)]))
)

/**
 * Every pane of every shell tab, as ONE flat list keyed by pane key.
 *
 * One list — not one per tab — because moving a pane between groups (the
 * sidebar's drag & drop) must not move its component between `v-for` parents:
 * Vue answers that by destroying and rebuilding it, and a rebuilt pane loses
 * the xterm a program is drawing into. In a single keyed list a move is a
 * style change. Off-stage tiles are hidden with `display: none` instead.
 */
const allTiles = computed(() =>
  store.tabs
    .filter((tab) => tab.kind === 'shell' && tab.root)
    .flatMap((tab) => (rectsByTab.value[tab.key] ?? []).map((rect) => ({ tab, rect })))
)

// ── pane handles ─────────────────────────────────────────────────────────────

interface PaneHandle {
  pushSize: () => void
  prefill: (text: string) => void
  run: (text: string) => void
  focusTerm: () => void
}
const paneHandles = new Map<string, PaneHandle>()

function setPaneHandle(paneKey: string, instance: unknown) {
  if (instance && typeof (instance as PaneHandle).pushSize === 'function') {
    paneHandles.set(paneKey, instance as PaneHandle)
  } else {
    paneHandles.delete(paneKey)
  }
}

/** The handle of the pane the keyboard belongs to. */
function activeHandle(): PaneHandle | undefined {
  const key = store.activeTab?.activePaneKey
  return key ? paneHandles.get(key) : undefined
}

// Focus follows the store: a pane activated by keyboard gets the caret.
watch(
  () => [store.activeTabKey, store.activeTab?.activePaneKey] as const,
  () => {
    activeHandle()?.focusTerm()
    void Promise.resolve().then(() => {
      for (const handle of paneHandles.values()) handle.pushSize()
    })
  }
)

// ── tabs and splits ──────────────────────────────────────────────────────────

/**
 * The explorer's view for THIS host: a new tab opens in the folder the tree
 * shows. The explorer's pin and the suite pointer can differ (each host keeps
 * its own pin; the pointer can be unset from outside), and the tree in front
 * of the user is the one that counts — docs/PLAN-WORKSPACES.md §9. General is
 * not a folder: the cwd stays what it was.
 */
function onExplorerSelect(s: { general: boolean; path: string | null }) {
  if (s.general || !s.path) return
  cwd.value = s.path
  workspacePath.value = s.path
}

function newTab(shellId?: string) {
  const shell = shellId ? shells.value.find((s) => s.id === shellId) ?? defaultShell.value : defaultShell.value
  if (!shell) return
  store.createTab(shell.path ? shell : null, cwd.value)
  rememberLayout()
}

function splitPane(direction: 'right' | 'down' | 'left' | 'up') {
  const dir = direction === 'right' || direction === 'left' ? 'row' : 'col'
  const before = direction === 'left' || direction === 'up'
  store.splitActive(dir, before)
  rememberLayout()
}

function zoomPane() {
  store.toggleZoom()
}

/**
 * A file activated in the explorer. The console has no editor of its own, so
 * it announces the file and whichever editor module is warm picks it up — the
 * same `core.file.open` the shell palette emits. Until 2026-08-26 the explorer
 * wrote straight into QuantCanvas' store, which only worked because Pinia is
 * shared; the boundary is honest now (ARCHITECTURE.md §4).
 */
function openInCanvas(file: { path: string }) {
  void bus.emit('core.file.open', { path: file.path })
}

/** Same route for the header's search: announce it, let an editor take it. */
function openFromSearch(path: string) {
  void bus.emit('core.file.open', { path })
}

/**
 * Back/forward step through the open tabs.
 *
 * The console has no navigation history to walk — a terminal is a place, not a
 * page — so the pair keeps its position and shape but moves along the tab strip,
 * which is the nearest thing to "where I was before" here.
 */
const tabIndex = computed(() => store.tabs.findIndex((t) => t.key === store.activeTabKey))
const canGoBack = computed(() => tabIndex.value > 0)
const canGoForward = computed(() => tabIndex.value >= 0 && tabIndex.value < store.tabs.length - 1)

function stepTab(delta: number) {
  const next = store.tabs[tabIndex.value + delta]
  if (next) store.activeTabKey = next.key
}

/**
 * The explorer's "Change Directory": `cd` the focused pane into the clicked
 * folder. Runs, deliberately — it is a teleporter, not a prefill. cmd needs
 * `/d` to cross drives; PowerShell's `cd` handles that on its own. The slot's
 * cwd follows, so the pane bar says where the shell now is.
 */
function cdTo(path: string) {
  const tab = store.activeTab
  if (!tab || tab.kind !== 'shell') {
    notice('Open a shell tab first')
    return
  }
  const paneKey = tab.activePaneKey
  const handle = paneHandles.get(paneKey)
  const slot = store.panes[paneKey]
  if (!handle || !slot || slot.exited) {
    notice('No live pane to cd in')
    return
  }
  const isCmd = (slot.shellPath ?? '').toLowerCase().includes('cmd')
  handle.run(isCmd ? `cd /d "${path}"` : `cd "${path}"`)
  store.reportPaneState(paneKey, { cwd: path })
}

// ── the keymap ───────────────────────────────────────────────────────────────

/**
 * Chord labels for menus, from the one keymap table — never hardcoded next to
 * an action (that is how two lists drift apart).
 */
const keyLabels = computed<Record<string, string>>(() =>
  Object.fromEntries(
    CONSOLE_ACTIONS.map((action) => [action.id, prettyBinding(bindingFor(action.id))])
  )
)

/** Stored rebinds, applied over the shipped defaults. */
const keymapOverrides = ref<Record<string, string>>({})

function bindingFor(actionId: string): string {
  const action = CONSOLE_ACTIONS.find((a) => a.id === actionId)
  const override = keymapOverrides.value[actionId]
  if (typeof override === 'string' && parseBinding(override)) return override
  return action?.defaultBinding ?? ''
}

/** Chords the panes must let bubble past xterm (tab/group switching). */
const terminalOverrides = computed<string[]>(() =>
  CONSOLE_ACTIONS.filter((action) => action.overridesTerminal).flatMap((action) =>
    chordsFor(action, bindingFor(action.id))
  )
)

/** The action handlers, one per keymap id. */
const actionHandlers: Record<string, (e: KeyboardEvent) => void> = {
  newTab: (e) => {
    e.preventDefault()
    newTab()
  },
  closeTab: (e) => {
    e.preventDefault()
    if (store.activeTabKey) void store.closeTab(store.activeTabKey)
  },
  reopenTab: (e) => {
    e.preventDefault()
    if (!store.reopenClosedTab()) notice('Nothing to reopen')
  },
  prevTab: (e) => {
    e.preventDefault()
    store.stepTab(-1)
  },
  nextTab: (e) => {
    e.preventDefault()
    store.stepTab(1)
  },
  moveTabLeft: (e) => {
    e.preventDefault()
    store.moveActiveTab(-1)
  },
  moveTabRight: (e) => {
    e.preventDefault()
    store.moveActiveTab(1)
  },
  ...Object.fromEntries(
    Array.from({ length: 8 }, (_, i) => [
      `selectTab${i + 1}`,
      (e: KeyboardEvent) => {
        e.preventDefault()
        store.selectTabAt(i + 1)
      },
    ])
  ),
  selectLastTab: (e) => {
    e.preventDefault()
    store.selectTabAt(0)
  },
  splitRight: (e) => {
    e.preventDefault()
    splitPane('right')
  },
  splitDown: (e) => {
    e.preventDefault()
    splitPane('down')
  },
  closePane: (e) => {
    e.preventDefault()
    const key = store.activeTab?.activePaneKey
    if (key) void store.closePane(key)
  },
  zoomPane: (e) => {
    e.preventDefault()
    zoomPane()
  },
  prevPane: (e) => {
    e.preventDefault()
    store.focusAdjacentPane(-1)
  },
  nextPane: (e) => {
    e.preventDefault()
    store.focusAdjacentPane(1)
  },
  focusPaneLeft: (e) => {
    e.preventDefault()
    store.focusDirection('left')
  },
  focusPaneRight: (e) => {
    e.preventDefault()
    store.focusDirection('right')
  },
  focusPaneUp: (e) => {
    e.preventDefault()
    store.focusDirection('up')
  },
  focusPaneDown: (e) => {
    e.preventDefault()
    store.focusDirection('down')
  },
}

const shortcuts = computed<ShortcutBinding[]>(() => {
  const out: ShortcutBinding[] = []
  for (const action of CONSOLE_ACTIONS) {
    const handler = actionHandlers[action.id]
    if (!handler) continue
    for (const chord of chordsFor(action, bindingFor(action.id))) {
      const parsed = parseBinding(chord)
      if (parsed) out.push({ ...parsed, handler })
    }
  }
  return out
})

// `useShortcuts` captures the array once and iterates it per keydown — so the
// array's *identity* is kept stable and its contents follow the computed, and
// a rebind reaches the listener without re-registering anything.
const liveBindings: ShortcutBinding[] = []
watch(
  shortcuts,
  (next) => {
    liveBindings.length = 0
    liveBindings.push(...next)
  },
  { immediate: true }
)
useShortcuts(liveBindings)

// ── session memory ───────────────────────────────────────────────────────────

const LAYOUTS_KEY = 'session.layouts'
const MAX_WORKSPACES = 12

/**
 * Persist the layout for this workspace, debounced. The entry is re-inserted
 * rather than updated in place: `Object` keeps insertion order for string
 * keys, so the workspace touched last is last, and trimming from the front
 * drops the one nobody has opened in longest.
 */
function rememberLayout() {
  const key = workspacePath.value
  if (!key || !memoryReady.value) return
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    const layout = store.snapshot()
    void qs.core
      .getSetting<Record<string, ConsoleLayout>>('console', LAYOUTS_KEY)
      .then((all) => {
        const next: Record<string, ConsoleLayout> = { ...(all ?? {}) }
        delete next[key]
        next[key] = layout
        const keys = Object.keys(next)
        while (keys.length > MAX_WORKSPACES) {
          const oldest = keys.shift()
          if (oldest) delete next[oldest]
        }
        return qs.core.setSetting('console', LAYOUTS_KEY, next)
      })
      .catch(() => {
        // browser development, or a settings store that is not writable — the
        // console works either way, it simply forgets.
      })
  }, 500)
}

/** The arrangement as a value, so the watcher fires on a change to it only. */
const layoutSignature = computed(() =>
  JSON.stringify(
    store.tabs
      .filter((tab) => tab.kind === 'shell')
      .map((tab) => [
        tab.title,
        tab.groupKey,
        tab.root,
        store.tabPanes(tab).map((slot) => [slot.shellPath, slot.cwd]),
        tab.key === store.activeTabKey,
      ])
      .concat([store.groups.map((group) => [group.key, group.title])])
  )
)

watch(layoutSignature, rememberLayout)

/**
 * Boot order: live sessions first, the recorded layout only as the cold-start
 * fallback. A webview reload leaves every PTY alive in Rust — adopting those
 * is lossless, while restoring the layout would open fresh shells *next to*
 * the surviving ones.
 */
async function restoreMemory() {
  const live = await store.liveSessions()
  if (live.size > 0) {
    await store.adoptRunningSessions(shells.value)
    memoryReady.value = true
    return
  }
  try {
    const all = await qs.core.getSetting<Record<string, ConsoleLayout>>('console', LAYOUTS_KEY)
    const layout = workspacePath.value ? all?.[workspacePath.value] ?? null : null
    store.restoreLayout(layout ?? null, shells.value)
  } catch {
    // Nothing recorded — the empty state offers a shell.
  }
  memoryReady.value = true
}

// ── page lifecycle ───────────────────────────────────────────────────────────

function notice(text: string) {
  toast.value = text
  if (toastTimer) clearTimeout(toastTimer)
  toastTimer = setTimeout(() => {
    toast.value = null
  }, 2600)
}

onMounted(async () => {
  offWorkspace = bus.on<{ name: string; path: string }>('core.workspace.opened', (event) => {
    if (!event.payload?.path) return
    cwd.value = event.payload.path
    workspacePath.value = event.payload.path
    rememberLayout()
  })

  offSetting = bus.on<{ scope: string; key: string; value: unknown }>(
    'core.setting.changed',
    (event) => {
      if (event.payload?.scope !== 'console') return
      if (event.payload.key === 'shell.default') {
        shellPreference.value = typeof event.payload.value === 'string' ? event.payload.value : null
      }
      if (event.payload.key === 'keymap') {
        keymapOverrides.value =
          event.payload.value && typeof event.payload.value === 'object'
            ? (event.payload.value as Record<string, string>)
            : {}
      }
    }
  )

  try {
    shellPreference.value = await qs.core.getSetting<string | null>('console', 'shell.default')
    const storedKeymap = await qs.core.getSetting<Record<string, string>>('console', 'keymap')
    if (storedKeymap && typeof storedKeymap === 'object') keymapOverrides.value = storedKeymap
  } catch {
    // browser development
  }

  try {
    const found = await qs.console.listShells()
    shells.value = found.length ? found : hasTauriBridge() ? [] : DEMO_SHELLS
  } catch (e) {
    if (hasTauriBridge()) {
      store.error = `Shells could not be listed: ${e instanceof Error ? e.message : String(e)}`
    }
    shells.value = hasTauriBridge() ? [] : DEMO_SHELLS
  }

  try {
    // The shell's source of truth for the open workspace: the registry's
    // `getActiveWorkspace()`, which answers null for a pointer whose folder is
    // no longer registered (the raw setting is not read here — a dangling one
    // sent every new terminal into a folder the list did not show). This is
    // what makes a fresh terminal open in the project folder instead of
    // wherever the app process happens to run (`apps/src-tauri` under
    // `tauri dev`).
    const workspace = await getActiveWorkspace()
    if (workspace?.path) {
      workspacePath.value = workspace.path
      cwd.value = workspace.path
    }
  } catch {
    // No workspace — the memory stays off; the backend falls back to the home
    // directory rather than the process cwd.
  }

  await restoreMemory()

  if (route.query.view === 'suite') store.openSuiteTab()
  if (route.query.new === '1' && defaultShell.value) newTab()
})

watch(
  () => route.query.view,
  (view) => {
    if (view === 'suite') store.openSuiteTab()
  }
)

onUnmounted(() => {
  offWorkspace?.()
  offSetting?.()
  if (toastTimer) clearTimeout(toastTimer)
  if (saveTimer) clearTimeout(saveTimer)
})
</script>

<template>
  <div class="console-page" data-module="console">
    <QModuleHeader module-id="console">
      <!-- The QuantSpace header anatomy: a wedge against each cap for the side
           panels, the shared search field centred, history actions beside it.
           There is no bottom strip in the console, so the right slot carries
           the one thing a terminal always wants — a new tab. -->
      <QHeaderEdge
        side="left"
        :active="explorerVisible"
        label="Toggle Explorer (Ctrl+B)"
        @click="explorerVisible = !explorerVisible"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="9" y1="4" x2="9" y2="20" />
        </svg>
      </QHeaderEdge>

      <QFileSearch ref="searchRef" :root="workspacePath" @open="openFromSearch">
        <template #left>
          <QHeaderAction :disabled="!canGoBack" label="Previous tab" @click="stepTab(-1)">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </QHeaderAction>
          <QHeaderAction :disabled="!canGoForward" label="Next tab" @click="stepTab(1)">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </QHeaderAction>
        </template>

        <template #right>
          <!-- New terminal moved off the header (user, 2026-08-27): the groups
               panel and the newTab shortcut cover it, and this slot carries the
               suite's footer toggle — the same glyph as QuantCode/QuantCanvas. -->
          <QHeaderAction :active="footerVisible" label="Toggle footer" @click="footerVisible = !footerVisible">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="4" width="18" height="16" rx="1.5" />
              <line x1="3" y1="14" x2="21" y2="14" />
            </svg>
          </QHeaderAction>
        </template>
      </QFileSearch>

      <QHeaderEdge
        side="right"
        :active="groupsVisible"
        label="Toggle Groups (Ctrl+Shift+B)"
        @click="groupsVisible = !groupsVisible"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="15" y1="4" x2="15" y2="20" />
        </svg>
      </QHeaderEdge>
    </QModuleHeader>

    <div class="console-body">
      <!-- Left: the SAME file explorer QuantCanvas shows — one component, two
           hosts, so the two modules can never disagree about the workspace
           (the user's consistency ask, 2026-08-20). `.qc-skin` maps the
           `--qc-*` tokens it is styled with (theme-bridge.css). -->
      <QSidebar :model-value="explorerVisible" :width="sidebarWidth" storage-key="console.explorer" resizable
        @update:width="sidebarWidth = $event">
        <div class="console-explorer-host qc-skin">
          <CanvasSidebarFileExplorer host="console" cd-enabled @cd="cdTo" @open="openInCanvas" @select="onExplorerSelect" />
        </div>
      </QSidebar>

      <div class="console-main">
      <main class="console-stage">
        <div
          v-for="tab in store.tabs"
          v-show="tab.key === store.activeTabKey"
          :key="tab.key"
          class="console-tab-body"
        >
          <ConsoleSuiteSession v-if="tab.kind === 'suite'" :visible="tab.key === store.activeTabKey" />

          <!-- The split tree draws only the dividers; the panes are flat tiles
               positioned by the same geometry, so a split never remounts a
               pane (see the header comment). -->
          <ConsoleSplitTree
            v-else-if="tab.root"
            :node="tab.root"
            :zoomed-pane-key="tab.zoomedPaneKey"
            @resize="store.setSizes($event.id, $event.sizes)"
          >
            <template #leaf>
              <div class="console-leaf" />
            </template>
          </ConsoleSplitTree>

        </div>

        <!-- All panes, one flat keyed list across every group — see `allTiles`
             for why they are not inside the tab bodies above. -->
        <ConsolePane
          v-for="tile in allTiles"
          :key="tile.rect.paneKey"
          :ref="(el) => setPaneHandle(tile.rect.paneKey, el)"
          class="console-tile"
          :class="{ 'has-ring': (rectsByTab[tile.tab.key]?.length ?? 0) > 1 }"
          :style="[
            tileStyle(tile.tab, tile.rect),
            paneOnStage(tile.tab, tile.rect.paneKey) ? null : { display: 'none' },
          ]"
          :session-id="store.panes[tile.rect.paneKey]?.sessionId ?? null"
          :shell="store.panes[tile.rect.paneKey]?.shellPath ?? null"
          :cwd="store.panes[tile.rect.paneKey]?.cwd ?? '.'"
          :visible="paneVisible(tile.tab.key, tile.rect.paneKey)"
          :staged="paneOnStage(tile.tab, tile.rect.paneKey)"
          chrome
          :label="store.panes[tile.rect.paneKey]?.shellLabel"
          :active="tile.rect.paneKey === tile.tab.activePaneKey"
          :keys="keyLabels"
          :terminal-overrides="terminalOverrides"
          @opened="store.reportPaneState(tile.rect.paneKey, { sessionId: $event })"
          @exited="store.reportPaneState(tile.rect.paneKey, { exited: true })"
          @failed="store.error = $event"
          @state="store.reportPaneState(tile.rect.paneKey, $event)"
          @copied="notice('Copied')"
          @focus="store.focusPane(tile.rect.paneKey)"
          @split="splitPane($event)"
          @zoom="zoomPane()"
          @close="store.closePane(tile.rect.paneKey)"
        />

        <!-- Empty state: the shell picker. -->
        <div v-if="!store.tabs.length" class="console-empty">
          <h1>New terminal</h1>
          <p v-if="shells.length">Choose a shell.</p>
          <p v-else>No shells found on this machine.</p>
          <div class="console-shells">
            <button
              v-for="shell in shells"
              :key="shell.id"
              class="console-shell"
              @click="newTab(shell.id)"
            >
              {{ shell.label }}
              <span v-if="shell.isDefault" class="console-shell-hint">default</span>
            </button>
          </div>
        </div>

        <div v-if="toast" class="console-toast" role="status">{{ toast }}</div>
        <div v-if="store.error" class="console-error" role="alert">
          {{ store.error }}
          <button class="console-error-close" @click="store.error = null">Dismiss</button>
        </div>
      </main>

      <!-- Overlay the stage without resizing the running terminal grids. -->
      <CodeDock
        class="console-dock"
        :panel="footerVisible ? footerTab : null"
        :root="workspacePath"
        :visible="moduleOnScreen"
        @update:panel="onFooterPanel"
        @reveal="(t: { path: string; line: number }) => bus.emit('core.file.open', { path: t.path })"
      />
      </div>

      <!-- Right: the terminal groups — the list that used to be the left
           sidebar, moved so the explorer can take its place. -->
      <QRightPanel :model-value="groupsVisible" :width="rightWidth" storage-key="console.groups"
        @update:width="rightWidth = $event">
        <div class="console-groups-host">
          <div class="console-groups-scroll">
            <ConsoleGroups
              :tabs="store.tabs"
              :views="store.tabViews"
              :panes-by-tab="panesByTab"
              :active-tab-key="store.activeTabKey"
              :can-create="!!defaultShell"
              :shells="shells"
              :keys="keyLabels"
              @select-tab="store.selectTab"
              @close-tab="store.closeTab"
              @rename-tab="store.renameTab($event.key, $event.title)"
              @reorder-tab="store.reorderTabs($event.from, $event.to)"
              :group-list="store.groups"
              @focus-pane="store.focusPane"
              @close-pane="store.closePane"
              @move-tab-to-group="store.moveTabToGroup($event.tabKey, $event.groupKey)"
              @create-group-with="store.createGroupWith($event)"
              @rename-group="store.renameGroup($event.key, $event.title)"
              @delete-group="store.deleteGroup($event)"
              @reorder-group="store.reorderGroups($event.from, $event.to)"
              @create="newTab($event)"
            />
          </div>
        </div>
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
/* Everything visual lives in the module stylesheet (assets/css/main.css) — the
   page only positions its launcher inside the header's stretched centre slot. */

</style>
