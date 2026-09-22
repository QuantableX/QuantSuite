<script setup lang="ts">
/**
 * QuantCode — the editor module of QuantSpace (docs/PLAN-QUANTSPACE.md).
 *
 * A VS Code-shaped workbench wearing the suite's chrome: `QModuleHeader` on
 * top, the shared file explorer on the left, editor groups in the middle, a
 * dock below. Every heavy part is borrowed rather than rebuilt — the explorer
 * from QuantCanvas, Monaco from `QCodeEditor`, the terminal from
 * `ConsolePane`, files and git from `qs.files`. This module owns the
 * workbench, not the tools.
 */
import { bus, getActiveWorkspace, listWorkspaces, onWorkspacesChanged, samePath } from '@quantsuite/core'
import { useEditorStore } from '#code-root/stores/editor'
import { useBookmarksStore } from '#code-root/stores/bookmarks'

const store = useEditorStore()
const bookmarksStore = useBookmarksStore()

/** The suite is dark by decision (packages/core `useTheme`). */
const THEME = 'dark' as const

/** Explorer width: 220 to start, so it lines up with the header's logo cap,
 *  then whatever the user drags it to (QSidebar persists it). */
const explorerWidth = ref(220)
const searchRef = ref<{ arm: () => void; disarm: () => void } | null>(null)

/** Group refs, so the problems panel and search can jump to a line. */
const groupRefs = ref<Record<string, { reveal: (line: number, column?: number) => void } | null>>({})

/** The module is on screen — the terminal pane must not draw while hidden. */
const isActive = ref(false)

// ── Workspace ──
//
// The middle area follows the EXPLORER's per-host view (user, 2026-08-31):
// the session key is the picked workspace's path, or the General sentinel —
// General has a tab set of its own and never shows the previous workspace's.

/** What this module's explorer last showed — the same persisted selection
 *  the explorer itself restores (`qc-explorer-sel:code`, a shared contract,
 *  read here so a hidden explorer still yields the right initial session). */
function persistedExplorerSelection(): { general: boolean; workspaceId: string | null } {
  try {
    const raw = JSON.parse(localStorage.getItem('qc-explorer-sel:code') ?? 'null')
    if (raw && typeof raw === 'object') {
      return {
        general: !!raw.general,
        workspaceId: typeof raw.workspaceId === 'string' ? raw.workspaceId : null,
      }
    }
  } catch {
    /* fresh profile */
  }
  return { general: false, workspaceId: null }
}

/** Swap the session (and bookmarks) to the selected view, once. */
async function applySelection(selection: { general: boolean; path: string | null }) {
  const key = selection.general ? store.GENERAL_SESSION : selection.path
  const current = store.workspacePath
  if (key === current) return
  if (key && current && samePath(key, current)) return
  await store.restoreSession(key)
  await bookmarksStore.load(key)
}

async function syncWorkspace() {
  const selection = persistedExplorerSelection()
  if (selection.general) {
    await applySelection({ general: true, path: null })
    return
  }
  let path = (await getActiveWorkspace())?.path ?? null
  if (selection.workspaceId) {
    const pinned = (await listWorkspaces()).find((w) => w.id === selection.workspaceId)
    if (pinned) path = pinned.path
  }
  // Nothing open anywhere: General is the no-workspace session — the same
  // view the explorer falls back to (2026-09-02).
  await applySelection(path ? { general: false, path } : { general: true, path: null })
}

// ── Opening files ──

async function openFromExplorer(file: {
  path: string
  content: string
  kind: 'text' | 'image'
  /** Set when the explorer's content search sent us here. */
  line?: number
}) {
  await store.openFile(file.path, { content: file.content, kind: file.kind })
  if (file.line) {
    await nextTick()
    groupRefs.value[store.activeGroupId]?.reveal(file.line)
  }
}

/**
 * Jump to a place in a file — the problems panel and search results both land
 * here. The file is opened if it is not already, then revealed once the group
 * has rendered it.
 */
async function reveal(target: { path: string; line: number }) {
  await store.openFile(target.path)
  await nextTick()
  groupRefs.value[store.activeGroupId]?.reveal(target.line)
}

// ── Shortcuts ──

function onKeydown(e: KeyboardEvent) {
  // Alt+Left/Right walk the file history — QuantCanvas' binding for the same
  // pair of buttons.
  if (e.altKey && e.key === 'ArrowLeft') {
    e.preventDefault()
    void store.goBack()
    return
  }
  if (e.altKey && e.key === 'ArrowRight') {
    e.preventDefault()
    void store.goForward()
    return
  }

  const mod = e.ctrlKey || e.metaKey
  if (!mod) return

  const key = e.key.toLowerCase()

  // Ctrl+Alt+K — toggle a bookmark on the caret's line (the Bookmarks
  // extension's own binding). The store anchors it to the code, not the line.
  if (e.altKey && key === 'k') {
    e.preventDefault()
    const tab = store.activeTab
    if (tab && tab.kind === 'text' && !tab.readOnly) {
      bookmarksStore.toggle(tab.path, store.cursorLine)
    }
    return
  }

  // Ctrl+S / Ctrl+Shift+S — Monaco owns Ctrl+S while it has focus and emits
  // `save`; this catches the case where focus is anywhere else in the module.
  if (key === 's') {
    e.preventDefault()
    if (e.shiftKey) void store.saveAll()
    else if (store.activeTab) void store.saveTab(store.activeTab.id)
    return
  }
  // Ctrl+W — close the active tab. Not the window: the shell owns that.
  if (key === 'w' && !e.shiftKey) {
    e.preventDefault()
    if (store.activeTab) store.closeTab(store.activeGroupId, store.activeTab.id)
    return
  }
  // Ctrl+\ — split
  if (e.key === '\\') {
    e.preventDefault()
    store.splitGroup()
    return
  }
  // Ctrl+B — explorer, Ctrl+Shift+B — right panel, Ctrl+J — dock. The same
  // three bindings QuantCanvas and QuantFinance use for the same three edges.
  if (key === 'b' && e.shiftKey) {
    e.preventDefault()
    store.toggleRightPanel()
    return
  }
  if (key === 'b') {
    e.preventDefault()
    store.toggleExplorer()
    return
  }
  if (key === 'j' && !e.shiftKey) {
    e.preventDefault()
    store.togglePanel('terminal')
    return
  }
  // Ctrl+Shift+V — markdown preview ↔ source, VS Code's binding for the same
  // switch. A no-op on anything that is not markdown (the store checks).
  if (e.shiftKey && key === 'v') {
    const tab = store.activeTab
    if (tab?.language === 'markdown') {
      e.preventDefault()
      store.toggleMarkdownPreview(tab.id)
    }
    return
  }
  // Ctrl+Shift+F — project search, Ctrl+Shift+M — problems
  if (e.shiftKey && key === 'f') {
    e.preventDefault()
    store.panel = 'search'
    return
  }
  if (e.shiftKey && key === 'm') {
    e.preventDefault()
    store.panel = 'problems'
    return
  }
  // Ctrl+Tab — next tab in the focused group
  if (e.key === 'Tab') {
    const group = store.activeGroup
    if (group.tabs.length <= 1) return
    e.preventDefault()
    const i = group.tabs.findIndex((t) => t.id === group.activeTabId)
    const next = group.tabs[(i + (e.shiftKey ? -1 + group.tabs.length : 1)) % group.tabs.length]
    if (next) store.setActiveTab(group.id, next.id)
  }
}

// ── Lifecycle ──
//
// V3 warm cache: the stage keeps this page mounted while another module is
// active, so the window listener is bound only while QuantCode is visible —
// otherwise Ctrl+B, Ctrl+J and Ctrl+W would keep firing suite-wide. Bound in
// BOTH onMounted and onActivated: pages load async and mount after the stage's
// activation flush, so onActivated alone would miss the first visit; the bound
// flag makes the overlap safe. That same late mount can land in a stage the
// user has already left, hence the guard on the first bind.
let bound = false
let offWorkspace: (() => void) | undefined
let offFileOpen: (() => void) | undefined

function bind() {
  if (bound) return
  bound = true
  isActive.value = true
  window.addEventListener('keydown', onKeydown)
  searchRef.value?.arm()
}

function unbind() {
  if (!bound) return
  bound = false
  isActive.value = false
  window.removeEventListener('keydown', onKeydown)
  searchRef.value?.disarm()
}

onMounted(async () => {
  const { inActiveKeepAliveTree } = await import('@quantsuite/core')
  if (inActiveKeepAliveTree()) bind()

  await syncWorkspace()

  // A workspace opened anywhere in the suite — reload this module's session
  // for the new folder (docs/PLAN-WORKSPACES.md: one registry, announced once).
  offWorkspace = onWorkspacesChanged(() => {
    void syncWorkspace()
  })

  // The shell palette's file results, and QuantConsole's explorer.
  offFileOpen = bus.on<{ path: string }>('core.file.open', (event) => {
    if (event.payload?.path) void store.openFile(event.payload.path)
  })
})

onActivated(bind)
onDeactivated(unbind)

onUnmounted(() => {
  unbind()
  offWorkspace?.()
  offFileOpen?.()
})

</script>

<template>
  <!-- `qc-skin` maps the `--qc-*` tokens the shared explorer is styled with
       onto the suite palette (theme-bridge.css) — the same hook QuantConsole
       uses to host it. -->
  <div class="code-page qc-skin">
    <QModuleHeader module-id="code">
      <!-- The header anatomy QuantCanvas set: a full-height wedge against each
           cap for the side panels, the search field centred, and small round
           actions beside it for history and the bottom strip. Split lives on
           the focused group's tab strip, where the thing it splits is. -->
      <QHeaderEdge
        side="left"
        :active="store.explorerVisible"
        label="Toggle Explorer (Ctrl+B)"
        @click="store.toggleExplorer()"
      >
        <!-- The layout family: one rectangle, the toggled region marked by its
             divider — the same glyph the panel button uses, rotated to each
             edge, identical across the QuantSpace modules. -->
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="9" y1="4" x2="9" y2="20" />
        </svg>
      </QHeaderEdge>

      <QFileSearch
        ref="searchRef"
        :root="store.rootPath"
        @open="(path: string) => store.openFile(path)"
      >
        <template #left>
          <QHeaderAction :disabled="!store.canGoBack" label="Go Back (Alt+Left)" @click="store.goBack()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </QHeaderAction>
          <QHeaderAction :disabled="!store.canGoForward" label="Go Forward (Alt+Right)" @click="store.goForward()">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </QHeaderAction>
        </template>

        <template #right>
          <QHeaderAction
            :active="!!store.panel"
            label="Toggle panel (Ctrl+J)"
            @click="store.togglePanel('terminal')"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="4" width="18" height="16" rx="1.5" />
              <line x1="3" y1="14" x2="21" y2="14" />
            </svg>
          </QHeaderAction>
        </template>
      </QFileSearch>

      <QHeaderEdge
        side="right"
        :active="store.rightPanelVisible"
        label="Toggle Toolkit (Ctrl+Shift+B)"
        @click="store.toggleRightPanel()"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="15" y1="4" x2="15" y2="20" />
        </svg>
      </QHeaderEdge>
    </QModuleHeader>

    <div class="code-body">
      <!-- V3 shared shells at their DEFAULT geometry: 220px, in-flow, so the
           sidebar lines up with the header's logo cap and the right panel
           mirrors the actions cap. No width overrides — every module in the
           suite has the same edges (PLAN-V3 §3). -->
      <QSidebar
        :model-value="store.explorerVisible"
        :width="explorerWidth"
        storage-key="code.explorer"
        resizable
        @update:width="explorerWidth = $event"
      >
        <div class="explorer-host">
          <CanvasSidebarFileExplorer
            host="code"
            :selected-path="store.activeTab?.path ?? null"
            :refresh-key="store.explorerRefreshKey"
            @select="applySelection"
            @open="openFromExplorer"
            @deleted="(path: string) => { store.forgetPath(path); bookmarksStore.forgetPath(path) }"
          />
        </div>
      </QSidebar>

      <main class="code-main">
        <div class="groups">
          <CodeEditorGroup
            v-for="group in store.groups"
            :key="group.id"
            :ref="(el: any) => (groupRefs[group.id] = el)"
            :group="group"
            :theme="THEME"
            :root="store.rootPath"
          />
        </div>

        <CodeDock
          :panel="store.panel"
          :root="store.rootPath"
          :visible="isActive"
          @update:panel="store.panel = $event"
          @reveal="reveal"
        />
      </main>

      <QRightPanel :model-value="store.rightPanelVisible" storage-key="code.right">
        <CodeRightPanel
          :root="store.rootPath"
          @open="(path: string) => store.openFile(path)"
          @reveal="reveal"
        />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
.code-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  background: var(--qss-bg);
  color: var(--qss-text);
}

.code-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.explorer-host {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.code-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.groups {
  flex: 1;
  min-height: 0;
  display: flex;
}

/* ── Header centre ── */
/* ── Dock ── */
</style>
