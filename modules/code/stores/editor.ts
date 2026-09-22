/**
 * QuantCode's workbench state: editor groups, their tabs, diagnostics, panels.
 *
 * The store owns the buffers. Monaco holds a model per path and echoes what it
 * is given (see `QCodeEditor`), which is what lets the same file be open in two
 * groups without either of them fighting over the text.
 *
 * Files and git come from `qs.files` — the suite-wide primitives — so this
 * module needs no crate of its own (docs/PLAN-QUANTSPACE.md).
 */
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { bus, qs, samePath } from '@quantsuite/core'
import type { EditorMarker } from '@quantsuite/ui'
import type { CodeSession, CodeTab, EditorGroup, PanelTab, TabKind } from '../shared/types'

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif'])

/**
 * Monaco reads the language off the file extension for anything opened with a
 * `file:` URI, so this map only has to cover what it cannot guess and what the
 * tab strip wants to label. Unknown extensions fall through to plaintext.
 */
const LANGUAGES: Record<string, string> = {
  ts: 'typescript', mts: 'typescript', cts: 'typescript',
  tsx: 'typescript', js: 'javascript', mjs: 'javascript', cjs: 'javascript',
  jsx: 'javascript', vue: 'html', html: 'html', htm: 'html',
  css: 'css', scss: 'scss', less: 'less', json: 'json', jsonc: 'json',
  md: 'markdown', markdown: 'markdown', yaml: 'yaml', yml: 'yaml',
  toml: 'ini', ini: 'ini', rs: 'rust', py: 'python', go: 'go',
  java: 'java', c: 'c', h: 'c', cpp: 'cpp', hpp: 'cpp', cs: 'csharp',
  rb: 'ruby', php: 'php', swift: 'swift', kt: 'kotlin', lua: 'lua',
  sh: 'shell', bash: 'shell', zsh: 'shell', ps1: 'powershell',
  sql: 'sql', graphql: 'graphql', gql: 'graphql', xml: 'xml', svg: 'xml',
  dockerfile: 'dockerfile',
}

export function languageFor(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  if (IMAGE_EXTS.has(ext)) return 'image'
  return LANGUAGES[ext] ?? 'plaintext'
}

export function kindFor(path: string): TabKind {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  return IMAGE_EXTS.has(ext) ? 'image' : 'text'
}

function fileNameOf(path: string): string {
  return path.split(/[/\\]/).pop() ?? path
}

let counter = 0
const nextId = (prefix: string) => `${prefix}-${++counter}`

function emptyGroup(): EditorGroup {
  return { id: nextId('group'), tabs: [], activeTabId: null }
}

export const useEditorStore = defineStore('code/editor', () => {
  // ---- State ----
  const groups = ref<EditorGroup[]>([emptyGroup()])
  const activeGroupId = ref<string>(groups.value[0]!.id)
  /** Diagnostics by path. Monaco reports them per model, the panel shows all. */
  const markersByPath = ref<Map<string, EditorMarker[]>>(new Map())
  const explorerVisible = ref(true)
  const rightPanelVisible = ref(true)
  const panel = ref<PanelTab | null>(null)
  const wordWrap = ref(false)
  const minimap = ref(true)
  /** The folder everything is relative to. Set by the page from core.db. */
  const workspacePath = ref<string | null>(null)

  /** The session key of the General view — not a folder on disk. */
  const GENERAL_SESSION = '__general__'

  /**
   * The workspace as a filesystem ROOT — for search, git and the right
   * panel. `workspacePath` doubles as the session key, and the General
   * view's key is a sentinel, not a folder: those panels get null there
   * (their no-workspace state), never a fake path.
   */
  const rootPath = computed(() => (workspacePath.value === GENERAL_SESSION ? null : workspacePath.value))
  /** Bumped to make the explorer re-walk after a write it could not see. */
  const explorerRefreshKey = ref(0)
  /** Where the caret is in the focused editor — what a bookmark toggle pins. */
  const cursorLine = ref(1)

  /**
   * The live drag, shared across every tab strip: the strip a drag STARTED in
   * owns the pointer (capture), but the tab may land on another group's strip
   * or editor body — and that strip draws the insertion line for a gesture it
   * does not own. `before` is a tab id, null meaning "end of the row".
   */
  const dragTabId = ref<string | null>(null)
  const dropTarget = ref<{ groupId: string; row: 'pinned' | 'tabs'; before: string | null } | null>(null)

  /**
   * Which files were looked at, in order — what back/forward walk through.
   *
   * Paths, not tab ids: a tab can be closed and reopened, and the history
   * should survive that the way a browser's does.
   */
  const history = ref<string[]>([])
  const historyIndex = ref(-1)
  /** Set while back/forward drives the change, so it does not record itself. */
  let navigating = false

  // ---- Getters ----
  const activeGroup = computed<EditorGroup>(
    () => groups.value.find((g) => g.id === activeGroupId.value) ?? groups.value[0]!
  )

  const activeTab = computed<CodeTab | undefined>(() => {
    const g = activeGroup.value
    return g.tabs.find((t) => t.id === g.activeTabId)
  })

  /** Every diagnostic across every open file, worst first. */
  const allMarkers = computed<EditorMarker[]>(() => {
    const order = { error: 0, warning: 1, info: 2, hint: 3 }
    return [...markersByPath.value.values()]
      .flat()
      .sort((a, b) => order[a.severity] - order[b.severity] || a.line - b.line)
  })

  const errorCount = computed(() => allMarkers.value.filter((m) => m.severity === 'error').length)
  const warningCount = computed(() => allMarkers.value.filter((m) => m.severity === 'warning').length)

  /** A tab is dirty when its buffer differs from what is on disk. */
  function isDirty(tab: CodeTab): boolean {
    return tab.content !== tab.savedContent
  }

  const dirtyCount = computed(() =>
    groups.value.reduce((n, g) => n + g.tabs.filter(isDirty).length, 0)
  )

  const canGoBack = computed(() => historyIndex.value > 0)
  const canGoForward = computed(() => historyIndex.value < history.value.length - 1)

  // ---- History ----

  /** Record a visit. Repeats of the current entry are ignored. */
  function recordVisit(path: string): void {
    if (navigating) return
    if (history.value[historyIndex.value] === path) return
    // A new visit after going back drops the forward tail, as a browser does.
    history.value = [...history.value.slice(0, historyIndex.value + 1), path]
    historyIndex.value = history.value.length - 1
  }

  /** Show a path that is already in the history — reopening it if it was closed. */
  async function goTo(index: number): Promise<void> {
    const path = history.value[index]
    if (!path) return
    navigating = true
    historyIndex.value = index
    try {
      // Prefer a group that already has it, so back/forward does not move the
      // file between splits under the user.
      for (const group of groups.value) {
        const tab = group.tabs.find((t) => samePath(t.path, path))
        if (tab) {
          group.activeTabId = tab.id
          activeGroupId.value = group.id
          return
        }
      }
      await openFile(path)
    } finally {
      navigating = false
    }
  }

  const goBack = () => (canGoBack.value ? goTo(historyIndex.value - 1) : Promise.resolve())
  const goForward = () => (canGoForward.value ? goTo(historyIndex.value + 1) : Promise.resolve())

  // ---- Tabs ----

  function groupById(id: string): EditorGroup | undefined {
    return groups.value.find((g) => g.id === id)
  }

  /**
   * Open a file in a group, or focus it if that group already has it.
   *
   * Content is passed in when the caller already read the file (the explorer
   * does), and read here otherwise — so a path from the bus or the search panel
   * opens with one call and no duplicated read.
   */
  async function openFile(
    path: string,
    opts: {
      content?: string
      groupId?: string
      kind?: TabKind
      /** A read-only viewer tab (Timeline snapshots). Skipped by the session. */
      readOnly?: boolean
      /** Display name, when the path is a pseudo-path a file name can't be cut from. */
      title?: string
      /** Language override, for a pseudo-path whose extension says nothing. */
      language?: string
    } = {}
  ): Promise<void> {
    // No explicit target: a file already open in ANY group is focused where
    // it lives — a bookmark or explorer click must not mint a second tab just
    // because the file sits in the other split (the same rule goTo applies).
    if (!opts.groupId) {
      // The active group first: open in both splits means "stay where I am".
      const inOrder = [activeGroup.value, ...groups.value.filter((g) => g !== activeGroup.value)]
      for (const g of inOrder) {
        const tab = g.tabs.find((t) => !t.readOnly && samePath(t.path, path))
        if (tab) {
          g.activeTabId = tab.id
          activeGroupId.value = g.id
          recordVisit(path)
          return
        }
      }
    }

    // An explicit groupId that no longer resolves means the caller's group is
    // gone (a session restore superseded mid-flight) — opening into "whatever
    // is active now" is never what that caller meant.
    const group = opts.groupId ? groupById(opts.groupId) : activeGroup.value
    if (!group) return

    const existing = group.tabs.find((t) => samePath(t.path, path))
    if (existing) {
      group.activeTabId = existing.id
      activeGroupId.value = group.id
      if (!existing.readOnly) recordVisit(path)
      return
    }

    let content = opts.content
    const kind = opts.kind ?? kindFor(path)

    // One buffer per path: if the file is already open in another group, that
    // buffer wins — over the disk AND over content the caller read, both of
    // which would silently revert unsaved edits the other group is holding.
    const openElsewhere = opts.readOnly
      ? undefined
      : groups.value.flatMap((g) => g.tabs).find((t) => !t.readOnly && samePath(t.path, path))
    if (openElsewhere) {
      content = openElsewhere.content
    } else if (content == null) {
      try {
        content = await qs.files.readFile(path)
      } catch {
        content = `// Could not read ${fileNameOf(path)}\n`
      }
    }

    const tab: CodeTab = {
      id: nextId('tab'),
      path,
      fileName: opts.title ?? fileNameOf(path),
      content,
      // Dirty state must carry over too: a buffer with unsaved edits opened in
      // a second group is dirty there as well, not "clean at the edited text".
      savedContent: openElsewhere ? openElsewhere.savedContent : content,
      language: opts.language ?? languageFor(path),
      kind,
      pinned: false,
      readOnly: opts.readOnly ?? false,
      // A `.md` opens rendered, the way QuantCanvas' file window opens one —
      // a document is more often opened to be read than to be edited, and the
      // toggle sits one click away in the breadcrumb bar.
      preview: (opts.language ?? languageFor(path)) === 'markdown',
    }
    group.tabs.push(tab)
    group.activeTabId = tab.id
    activeGroupId.value = group.id
    // A read-only viewer's pseudo-path has nothing to reopen — keep it out of
    // the back/forward history the same way it stays out of the session.
    if (!tab.readOnly) recordVisit(path)
    persistSession()
  }

  function setActiveTab(groupId: string, tabId: string): void {
    const group = groupById(groupId)
    if (!group) return
    // A drag may have moved the tab out of this group between the gesture and
    // the click that trails it — activating an id the group does not hold
    // would leave the group pointing at nothing.
    const tab = group.tabs.find((t) => t.id === tabId)
    if (!tab) return
    group.activeTabId = tabId
    activeGroupId.value = groupId
    if (!tab.readOnly) recordVisit(tab.path)
    persistSession()
  }

  function closeTab(groupId: string, tabId: string): void {
    const group = groupById(groupId)
    if (!group) return
    const index = group.tabs.findIndex((t) => t.id === tabId)
    if (index === -1) return

    group.tabs.splice(index, 1)
    if (group.activeTabId === tabId) {
      // The neighbour to the left, or the new first tab — VS Code's rule, and
      // the one that does not jump the eye across the strip.
      group.activeTabId = group.tabs[Math.max(0, index - 1)]?.id ?? null
    }
    // An empty group disappears, unless it is the last one standing.
    if (!group.tabs.length && groups.value.length > 1) {
      closeGroup(group.id)
      return
    }
    persistSession()
  }

  function closeOthers(groupId: string, tabId: string): void {
    const group = groupById(groupId)
    if (!group) return
    group.tabs = group.tabs.filter((t) => t.id === tabId || t.pinned)
    group.activeTabId = tabId
    persistSession()
  }

  function togglePin(groupId: string, tabId: string): void {
    const tab = groupById(groupId)?.tabs.find((t) => t.id === tabId)
    if (!tab) return
    tab.pinned = !tab.pinned
    persistSession()
  }

  /**
   * Markdown: rendered document ↔ source. A no-op on anything else, so the
   * keyboard shortcut can fire without the caller checking the language first.
   */
  function toggleMarkdownPreview(tabId: string): void {
    for (const group of groups.value) {
      const tab = group.tabs.find((t) => t.id === tabId)
      if (tab?.language === 'markdown') tab.preview = !tab.preview
    }
  }

  function updateContent(tabId: string, content: string): void {
    for (const group of groups.value) {
      const tab = group.tabs.find((t) => t.id === tabId)
      if (!tab) continue
      tab.content = content
      // The same file open in another group must not go stale behind this one.
      for (const other of groups.value) {
        for (const t of other.tabs) {
          if (t.id !== tab.id && samePath(t.path, tab.path)) t.content = content
        }
      }
      return
    }
  }

  /**
   * Move a tab: to another group, to another place in its row, or between the
   * pinned and working rows — one function, because a drag can do all three at
   * once and they have to land as a single change.
   *
   * `before` is the tab to drop in front of, or null for "the end of the row".
   * A tab id rather than an index: the rows are filtered views of one array, so
   * an index would have to be translated, and a stale one would land the tab
   * somewhere the user did not point at.
   */
  function moveTab(
    tabId: string,
    to: { groupId: string; before?: string | null; pinned?: boolean }
  ): void {
    const from = groups.value.find((g) => g.tabs.some((t) => t.id === tabId))
    const target = groupById(to.groupId)
    if (!from || !target) return

    const index = from.tabs.findIndex((t) => t.id === tabId)
    const tab = from.tabs[index]
    if (!tab) return

    // Dropping a tab onto itself is a no-op, not a move to the end.
    if (to.before === tabId) return

    // Another group already showing this file: focus it rather than making a
    // second tab for the same path.
    if (from.id !== target.id) {
      const already = target.tabs.find((t) => samePath(t.path, tab.path))
      if (already) {
        from.tabs.splice(index, 1)
        if (from.activeTabId === tabId) {
          from.activeTabId = from.tabs[Math.max(0, index - 1)]?.id ?? null
        }
        target.activeTabId = already.id
        activeGroupId.value = target.id
        if (!from.tabs.length && groups.value.length > 1) closeGroup(from.id)
        else persistSession()
        return
      }
    }

    from.tabs.splice(index, 1)
    if (from.activeTabId === tabId && from.id !== target.id) {
      from.activeTabId = from.tabs[Math.max(0, index - 1)]?.id ?? null
    }

    // Which row it lands in IS its pinned state — dragging up pins, down unpins.
    if (to.pinned !== undefined) tab.pinned = to.pinned

    const at = to.before ? target.tabs.findIndex((t) => t.id === to.before) : -1
    if (at >= 0) target.tabs.splice(at, 0, tab)
    else target.tabs.push(tab)

    target.activeTabId = tab.id
    activeGroupId.value = target.id

    if (from.id !== target.id && !from.tabs.length && groups.value.length > 1) {
      closeGroup(from.id)
    } else {
      persistSession()
    }
  }

  // ---- Groups (the split) ----

  /**
   * Split: a new group to the right, carrying a file into it.
   *
   * With no argument it carries the active tab (Ctrl+\). The tab strip's
   * context menu passes the tab that was right-clicked — which need not be
   * the active one, and splitting the ACTIVE tab on a right-click at another
   * is exactly the bug that made this a parameter (2026-08-27).
   */
  function splitGroup(from?: { groupId: string; tabId: string }): void {
    const source = (from && groupById(from.groupId)) ?? activeGroup.value
    // Through the proxy, not the raw local — see restoreSession.
    groups.value.push(emptyGroup())
    const group = groups.value[groups.value.length - 1]!

    const tab = source.tabs.find((t) => t.id === (from?.tabId ?? source.activeTabId))
    if (tab) {
      const copy: CodeTab = { ...tab, id: nextId('tab') }
      group.tabs.push(copy)
      group.activeTabId = copy.id
    }
    activeGroupId.value = group.id
    persistSession()
  }

  function closeGroup(groupId: string): void {
    if (groups.value.length <= 1) return
    const index = groups.value.findIndex((g) => g.id === groupId)
    if (index === -1) return
    groups.value.splice(index, 1)
    if (activeGroupId.value === groupId) {
      activeGroupId.value = groups.value[Math.max(0, index - 1)]!.id
    }
    persistSession()
  }

  function setActiveGroup(groupId: string): void {
    if (groupById(groupId)) activeGroupId.value = groupId
  }

  // ---- Disk ----

  /** Write a tab to disk. The one place this module touches the filesystem. */
  async function saveTab(tabId: string): Promise<void> {
    for (const group of groups.value) {
      const tab = group.tabs.find((t) => t.id === tabId)
      if (!tab || tab.kind === 'image' || tab.readOnly) continue
      try {
        await qs.files.writeFile(tab.path, tab.content)
        // Every copy of this file across the splits is clean now.
        for (const g of groups.value) {
          for (const t of g.tabs) {
            if (samePath(t.path, tab.path)) t.savedContent = tab.content
          }
        }
        void bus.emit('code.file.saved', { path: tab.path })
        // The Timeline's feed: one snapshot per save, deduped in the backend.
        void qs.files.timelineSnapshot(tab.path, tab.content).catch(() => {})
        explorerRefreshKey.value++
      } catch (e) {
        console.error('[code] save failed', e)
      }
      return
    }
  }

  async function saveAll(): Promise<void> {
    const dirty = groups.value.flatMap((g) => g.tabs.filter(isDirty))
    for (const tab of dirty) await saveTab(tab.id)
  }

  /** A file vanished from disk — drop every tab showing it. */
  function forgetPath(path: string): void {
    for (const group of groups.value) {
      const tab = group.tabs.find((t) => samePath(t.path, path))
      if (tab) closeTab(group.id, tab.id)
    }
  }

  function setCursorLine(line: number): void {
    cursorLine.value = line
  }

  // ---- Diagnostics ----

  function setMarkers(path: string, markers: EditorMarker[]): void {
    const next = new Map(markersByPath.value)
    if (markers.length) next.set(path, markers)
    else next.delete(path)
    markersByPath.value = next
  }

  // ---- Panels ----

  function togglePanel(tab: PanelTab): void {
    panel.value = panel.value === tab ? null : tab
  }

  function toggleExplorer(): void {
    explorerVisible.value = !explorerVisible.value
  }

  function toggleRightPanel(): void {
    rightPanelVisible.value = !rightPanelVisible.value
  }

  // ---- Session, per workspace ----

  const SETTINGS_SCOPE = 'code'
  const SESSION_KEY = 'session.byWorkspace'
  /** Keep the memory bounded — the same cap QuantConsole uses for layouts. */
  const MAX_WORKSPACES = 12

  let saveTimer: ReturnType<typeof setTimeout> | null = null
  let sessionReady = false
  /**
   * Which restore run is current. At startup two `syncWorkspace` calls can
   * overlap (onMounted and the registry's workspaces-changed announcement) —
   * without this, two restores interleave and the second's groups end up
   * holding the first's tabs, with the editor showing a file the tab strip
   * does not claim. Every await in `restoreSession` re-checks the generation
   * and a superseded run stops instead of writing on.
   */
  let restoreGen = 0

  function snapshot(): CodeSession {
    return {
      groups: groups.value.map((g) => ({
        // Read-only viewers are transient — their pseudo-paths cannot reopen.
        tabs: g.tabs.filter((t) => !t.readOnly).map((t) => ({ path: t.path, pinned: t.pinned })),
        activeTabPath:
          g.tabs.find((t) => t.id === g.activeTabId && !t.readOnly)?.path ?? null,
      })),
      activeGroupIndex: Math.max(0, groups.value.findIndex((g) => g.id === activeGroupId.value)),
    }
  }

  /**
   * Remember the open files for this workspace, debounced. Re-inserted rather
   * than updated in place: `Object` keeps insertion order for string keys, so
   * trimming from the front drops the workspace nobody has opened in longest.
   */
  function persistSession(): void {
    const key = workspacePath.value
    if (!key || !sessionReady) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(async () => {
      try {
        const all = (await qs.core.getSetting<Record<string, CodeSession>>(SETTINGS_SCOPE, SESSION_KEY)) ?? {}
        delete all[key]
        all[key] = snapshot()
        const keys = Object.keys(all)
        for (const stale of keys.slice(0, Math.max(0, keys.length - MAX_WORKSPACES))) delete all[stale]
        await qs.core.setSetting(SETTINGS_SCOPE, SESSION_KEY, all)
      } catch {
        // Browser development, or no backend — the session is a convenience.
      }
    }, 600)
  }

  /** Reopen what was open here last time. Silent when there is nothing. */
  async function restoreSession(path: string | null): Promise<void> {
    const gen = ++restoreGen
    sessionReady = false
    groups.value = [emptyGroup()]
    activeGroupId.value = groups.value[0]!.id
    markersByPath.value = new Map()
    history.value = []
    historyIndex.value = -1
    workspacePath.value = path

    if (!path) {
      sessionReady = true
      return
    }

    try {
      const all = await qs.core.getSetting<Record<string, CodeSession>>(SETTINGS_SCOPE, SESSION_KEY)
      if (gen !== restoreGen) return
      const session = all?.[path]
      if (session?.groups?.length) {
        groups.value = []
        for (const saved of session.groups) {
          // Re-read through the array: `push` stores the raw object, and every
          // later write (activeTabId below!) must go through the reactive
          // proxy — a raw write updates the data but re-renders NOTHING, which
          // left the editor on one file while the strip claimed another
          // (startup mismatch bug, 2026-08-27).
          groups.value.push(emptyGroup())
          const group = groups.value[groups.value.length - 1]!
          for (const entry of saved.tabs) {
            await openFile(entry.path, { groupId: group.id })
            if (gen !== restoreGen) return
            const tab = group.tabs[group.tabs.length - 1]
            if (tab) tab.pinned = entry.pinned
          }
          const active = group.tabs.find((t) => samePath(t.path, saved.activeTabPath))
          group.activeTabId = active?.id ?? group.tabs[0]?.id ?? null
        }
        if (!groups.value.length) groups.value = [emptyGroup()]
        activeGroupId.value =
          groups.value[Math.min(session.activeGroupIndex, groups.value.length - 1)]!.id
      }
    } catch {
      // Nothing recorded, or no backend — start empty.
    }
    if (gen === restoreGen) sessionReady = true
  }

  return {
    // State
    groups,
    activeGroupId,
    rootPath,
    GENERAL_SESSION,
    markersByPath,
    explorerVisible,
    rightPanelVisible,
    explorerRefreshKey,
    cursorLine,
    dragTabId,
    dropTarget,
    panel,
    wordWrap,
    minimap,
    workspacePath,
    // Getters
    activeGroup,
    activeTab,
    allMarkers,
    errorCount,
    warningCount,
    dirtyCount,
    isDirty,
    canGoBack,
    canGoForward,
    // History
    goBack,
    goForward,
    // Tabs
    openFile,
    setActiveTab,
    closeTab,
    closeOthers,
    togglePin,
    toggleMarkdownPreview,
    updateContent,
    moveTab,
    // Groups
    splitGroup,
    closeGroup,
    setActiveGroup,
    // Disk
    saveTab,
    saveAll,
    forgetPath,
    // Diagnostics
    setMarkers,
    setCursorLine,
    // Panels
    togglePanel,
    toggleExplorer,
    toggleRightPanel,
    // Session
    restoreSession,
  }
})
