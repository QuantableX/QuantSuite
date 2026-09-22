<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { useWorkspacesStore } from '../../../stores/workspaces'
import type { FileNode, GitFileStatus, SearchMatch } from '../../../shared/types'
import { onClickOutside } from '@vueuse/core'

/**
 * This component is SHARED: QuantCanvas hosts it in its own sidebar,
 * QuantConsole hosts it as its left panel, QuantCode as its explorer.
 *
 * `cdEnabled` is one host-specific difference: with it on, a directory's
 * context menu offers "Change Directory", emitted as `cd` for the host to run
 * in its focused terminal. The canvas host does not pass it and is unchanged.
 *
 * `host` is the other (2026-08-31, user): WHAT the explorer shows — which
 * workspace, or the General view — is remembered PER HOST MODULE, so
 * QuantCode, QuantCanvas and QuantConsole each keep their own selection and
 * switching modules never carries one module's view into another.
 */
const props = withDefaults(
  defineProps<{
    cdEnabled?: boolean
    /** Path to highlight as open. The host knows which tab is in front. */
    selectedPath?: string | null
    /** Bump to force a silent re-walk of the tree. */
    refreshKey?: number
    /** Which module hosts this instance — namespaces the remembered view. */
    host?: string
  }>(),
  { cdEnabled: false, selectedPath: null, refreshKey: 0, host: 'canvas' }
)

const emit = defineEmits<{
  /** "Change Directory" on a folder — the host `cd`s its focused terminal. */
  (e: 'cd', path: string): void
  /**
   * A file was activated. The explorer has already read it — the host only
   * decides where it goes, which is what lets QuantCanvas, QuantConsole and
   * QuantCode share one explorer and keep their own tabs.
   */
  (e: 'open', file: { path: string; content: string; kind: 'text' | 'image'; line?: number }): void
  /** A file was deleted from disk, so the host can drop any tab showing it. */
  (e: 'deleted', path: string): void
  /**
   * The host's view changed: a workspace was picked or the General view
   * toggled. The MIDDLE AREA follows this (user, 2026-08-31) — QuantCode
   * swaps its session, QuantCanvas its board — so General gets a content
   * context of its own instead of keeping the previous workspace's.
   */
  (
    e: 'select',
    selection: {
      /** The effective view: an explicit General pin, or nothing open. */
      general: boolean
      /** The explicit General pin alone (never the no-workspace fallback). */
      pinned: boolean
      workspaceId: string | null
      path: string | null
    }
  ): void
}>()

const workspacesStore = useWorkspacesStore()

// ── Workspace switcher ──
//
// A workspace IS a folder (docs/PLAN-WORKSPACES.md, 2026-08-26), so this menu
// lists WORKSPACES, not the roots of one. It is the only workspace selector in
// QuantCode — the suite titlebar carries none, because workspaces bind this app
// and nothing else. The dashboard still owns the full list; this is the switch.
const wsDropdownOpen = ref(false)
const wsDropdownRef = ref<HTMLElement | null>(null)

onClickOutside(wsDropdownRef, () => {
  wsDropdownOpen.value = false
})

/** The folder's parent, shown small above the name. */
function parentPathOf(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean)
  if (parts.length <= 1) return ''
  return parts.slice(0, -1).join('/') + ' /'
}

/** The folder's own name — what the workspace is called by default. */
function folderNameOf(path: string): string {
  return path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? path
}

// ── Per-host view selection (2026-08-31) ──
//
// WHAT this explorer shows — a workspace, or the virtual "General" view with
// every registered workspace as a root — is this HOST's own, persisted per
// module (user decision: QuantCode, QuantCanvas and QuantConsole must never
// inherit each other's view). The suite-global `core / workspace.active`
// still exists for module internals and follows the last explicit workspace
// pick in any host; General never touches it (a fake id there would cascade
// into QuantCode's session restore and the canvas state swap). A workspace
// is still a folder; General is a view over the registry, not a workspace
// (PLAN-WORKSPACES §8).
const GENERAL_WS = '__general__'

interface HostSelection {
  general: boolean
  workspaceId: string | null
}

function selectionKey(): string {
  return `qc-explorer-sel:${props.host}`
}

function readSelection(): HostSelection {
  try {
    const raw = JSON.parse(localStorage.getItem(selectionKey()) ?? 'null')
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

const selection = ref<HostSelection>(readSelection())

function persistSelection() {
  try {
    localStorage.setItem(selectionKey(), JSON.stringify(selection.value))
  } catch {
    /* private mode */
  }
}

/**
 * The workspace THIS host shows: its own pinned choice when it has one the
 * registry still knows, the suite's active workspace otherwise.
 */
const currentWorkspace = computed(() => {
  const pinned = selection.value.workspaceId
  if (pinned) {
    const hit =
      workspacesStore.sorted.find((w) => w.id === pinned) ??
      (workspacesStore.activeWorkspace?.id === pinned ? workspacesStore.activeWorkspace : undefined)
    if (hit) return hit
  }
  return workspacesStore.activeWorkspace
})

/**
 * General: the explicit pin — and the no-workspace state. With nothing open
 * (no pin the registry still knows, no suite-active workspace) the view is
 * General rather than an empty explorer above a board no window can be
 * created on (2026-09-02: a dangling `workspace.active` reads as none now,
 * which is exactly this state). Not persisted as a pin, so the next workspace
 * pick anywhere — the dashboard included — is followed.
 */
const generalMode = computed(
  () => selection.value.general || (workspacesStore.settled && !currentWorkspace.value)
)

/** The General tree's roots: the registry, plus any synthetic entries
 *  (active or pinned) the registry does not carry. */
const generalRoots = computed(() => {
  const list = [...workspacesStore.sorted]
  for (const extra of [workspacesStore.activeWorkspace, currentWorkspace.value]) {
    if (extra && !list.some((w) => w.id === extra.id)) list.push(extra)
  }
  return list
})

const activeWsParentPath = computed(() => {
  if (generalMode.value) return 'every workspace /'
  const ws = currentWorkspace.value
  return ws ? parentPathOf(ws.path) : ''
})

const activeWsFolderName = computed(() => {
  if (generalMode.value) return 'General'
  const ws = currentWorkspace.value
  if (!ws) return 'No workspace'
  return folderNameOf(ws.path) || ws.name
})

async function selectWorkspace(id: string) {
  wsDropdownOpen.value = false
  const changedLocally = selection.value.general || selection.value.workspaceId !== id
  selection.value = { general: false, workspaceId: id }
  persistSelection()
  if (id !== workspacesStore.activeWorkspaceId) {
    // The suite-global active follows the LAST explicit pick in any host —
    // sessions and canvas state key off it. Our currentWorkspace watcher
    // reloads the tree; other hosts keep their own pins.
    await workspacesStore.setActiveWorkspace(id)
  } else if (changedLocally) {
    void loadFileTree()
  }
}

function selectGeneral() {
  wsDropdownOpen.value = false
  if (selection.value.general) return
  selection.value = { ...selection.value, general: true }
  persistSelection()
  void loadFileTree()
}

/** Folder dialog → open it as a workspace. The one way a new one is made.
 *  The host that opened it also shows it. */
async function openWorkspaceFolder() {
  wsDropdownOpen.value = false
  await workspacesStore.openFolder()
  const id = workspacesStore.activeWorkspaceId
  if (id) {
    selection.value = { general: false, workspaceId: id }
    persistSelection()
    void loadFileTree()
  }
}

const fileTree = ref<FileNode[]>([])

/**
 * The search box searches FILE CONTENTS, the way VS Code's does — not file
 * names (user, 2026-08-26). Names are Ctrl+P in the module header; this is the
 * other half. While a query is running, the panel shows hits grouped by file
 * instead of the tree.
 */
const searchQuery = ref('')
const searchResults = ref<SearchMatch[]>([])
const searching = ref(false)
const searchError = ref<string | null>(null)
const searchTruncated = ref(false)
const collapsedFiles = ref<Set<string>>(new Set())

const filterOpen = ref(false)

/** VS Code's filter panel, verbatim: two globs and three switches. */
const filters = reactive({
  include: '',
  exclude: '',
  caseSensitive: false,
  wholeWord: false,
  regex: false,
})

const filtersActive = computed(
  () =>
    !!filters.include.trim() ||
    !!filters.exclude.trim() ||
    filters.caseSensitive ||
    filters.wholeWord ||
    filters.regex
)

function resetFilters() {
  filters.include = ''
  filters.exclude = ''
  filters.caseSensitive = false
  filters.wholeWord = false
  filters.regex = false
}

/** Escape closes the filter modal — the same key the settings modal uses. */
function onFilterKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && filterOpen.value) {
    e.preventDefault()
    filterOpen.value = false
  }
}

onMounted(() => document.addEventListener('keydown', onFilterKeydown))
onUnmounted(() => document.removeEventListener('keydown', onFilterKeydown))

const loading = ref(false)
const searchFocused = ref(false)
const rootExpanded = ref(true)

/**
 * Tree cache, per workspace, at module scope so it survives unmounts.
 *
 * In the suite this component unmounts on every module switch; without the
 * cache each switch back into QuantCode re-walked the entire workspace
 * folder (`read_dir_tree` + gitignore parsing) before anything rendered —
 * the "loads forever" on entry. Now the cached tree (with its expansion
 * state, held by reference) paints instantly and a silent refresh brings it
 * up to date behind it.
 */
const treeCache = new Map<string, FileNode[]>()

// Which file the host has in front. A prop, not a peek into another
// module's store — three hosts, three different tab models.
const selectedPath = computed(() => props.selectedPath)

// Workspace header info
const workspaceName = computed(() => {
  const ws = currentWorkspace.value
  if (!ws) return ''
  return folderNameOf(ws.path) || ws.name
})


// Context menu
const contextMenu = ref({ visible: false, x: 0, y: 0, node: null as FileNode | null })
const contextMenuRef = ref<HTMLElement | null>(null)

// Delete confirmation modal
const confirmDeleteVisible = ref(false)
const pendingDeleteNode = ref<FileNode | null>(null)

// Drag and drop state
const dragSourcePath = ref<string | null>(null)
const dropTargetPath = ref<string | null>(null)

onClickOutside(contextMenuRef, () => {
  contextMenu.value.visible = false
})

function collectExpandedPaths(nodes: FileNode[]): Set<string> {
  const paths = new Set<string>()
  for (const node of nodes) {
    if (node.isDirectory && node.expanded) {
      paths.add(node.path)
    }
    if (node.children) {
      for (const p of collectExpandedPaths(node.children)) {
        paths.add(p)
      }
    }
  }
  return paths
}

function restoreExpandedPaths(nodes: FileNode[], expanded: Set<string>): void {
  for (const node of nodes) {
    if (node.isDirectory && expanded.has(node.path)) {
      node.expanded = true
    }
    if (node.children) {
      restoreExpandedPaths(node.children, expanded)
    }
  }
}

async function loadFileTree(silent = false) {
  if (generalMode.value) return loadGeneralTree(silent)
  const workspace = currentWorkspace.value
  if (!workspace) return

  // Remember which folders are expanded
  const expandedPaths = collectExpandedPaths(fileTree.value)

  if (!silent) loading.value = true
  try {
    // One workspace, one folder, one walk — the tree renders flat
    // (docs/PLAN-WORKSPACES.md).
    const tree = await invoke<FileNode[]>('plugin:canvas|read_dir_tree', {
      path: workspace.path,
      gitignore: true,
    }).catch(() => [] as FileNode[])

    restoreExpandedPaths(tree, expandedPaths)
    fileTree.value = tree
    treeCache.set(workspace.id, tree)
  } catch {
    if (!silent) fileTree.value = []
  } finally {
    if (!silent) loading.value = false
  }
}

/**
 * The General tree: one synthetic root node per registered workspace (the
 * pattern showRootContextMenu already uses), each carrying that folder's
 * full `read_dir_tree` walk. Paths stay absolute throughout, which is what
 * lets every host open across workspaces unchanged.
 */
async function loadGeneralTree(silent = false) {
  const roots = generalRoots.value
  const expandedPaths = collectExpandedPaths(fileTree.value)
  if (!silent) loading.value = true
  try {
    const trees = await Promise.all(
      roots.map(async (ws) => {
        const children = await invoke<FileNode[]>('plugin:canvas|read_dir_tree', {
          path: ws.path,
          gitignore: true,
        }).catch(() => [] as FileNode[])
        return {
          name: folderNameOf(ws.path) || ws.name,
          path: ws.path,
          isDirectory: true,
          children,
          expanded: expandedPaths.has(ws.path) || roots.length === 1,
        } as FileNode
      })
    )
    for (const root of trees) {
      if (root.children) restoreExpandedPaths(root.children, expandedPaths)
    }
    fileTree.value = trees
    treeCache.set(GENERAL_WS, trees)
  } finally {
    if (!silent) loading.value = false
  }
}

/**
 * Run the query. Debounced — every keystroke otherwise walks the workspace —
 * and capped by the backend, which says so when it truncates.
 */
const SEARCH_LIMIT = 500
let searchTimer: ReturnType<typeof setTimeout> | null = null

async function runSearch() {
  const query = searchQuery.value
  // `search_files` is rooted at one folder; General mode fans out over every
  // root and shares the cap so the result list stays bounded.
  const roots = generalMode.value
    ? generalRoots.value.map((w) => w.path)
    : [currentWorkspace.value?.path].filter((p): p is string => !!p)
  if (!query.trim() || !roots.length) {
    searchResults.value = []
    searchError.value = null
    searchTruncated.value = false
    return
  }

  searching.value = true
  searchError.value = null
  try {
    const all: SearchMatch[] = []
    for (const root of roots) {
      if (all.length >= SEARCH_LIMIT) break
      const hits = await invoke<SearchMatch[]>('plugin:canvas|search_files', {
        path: root,
        pattern: query,
        options: {
          include: filters.include,
          exclude: filters.exclude,
          caseSensitive: filters.caseSensitive,
          wholeWord: filters.wholeWord,
          regex: filters.regex,
          limit: SEARCH_LIMIT - all.length,
        },
      })
      all.push(...hits)
    }
    searchResults.value = all
    searchTruncated.value = all.length >= SEARCH_LIMIT
  } catch (e) {
    // A half-typed regex is the common case; the backend explains itself.
    searchError.value = e instanceof Error ? e.message : String(e)
    searchResults.value = []
  } finally {
    searching.value = false
  }
}

function scheduleSearch() {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(runSearch, 220)
}

watch(searchQuery, scheduleSearch)
watch(filters, () => {
  if (searchQuery.value.trim()) scheduleSearch()
})

function clearSearch() {
  searchQuery.value = ''
  searchResults.value = []
  searchError.value = null
  searchTruncated.value = false
}

function relativeDir(path: string): string {
  const p = path.replace(/\\/g, '/')
  // Longest matching root wins — in General mode a hit can come from any
  // workspace, and its directory reads best as "<workspace>/…".
  const roots = generalMode.value
    ? generalRoots.value.map((w) => w.path.replace(/\\/g, '/'))
    : [currentWorkspace.value?.path.replace(/\\/g, '/') ?? '']
  const root = roots
    .filter((r) => r && p.toLowerCase().startsWith(r.toLowerCase()))
    .sort((a, b) => b.length - a.length)[0]
  const rel = root ? p.slice(root.length + 1) : p
  const parts = rel.split('/')
  parts.pop()
  if (generalMode.value && root) parts.unshift(folderNameOf(root))
  return parts.join('/')
}

/** Hits grouped by file, in the order the walk found them. */
const groupedResults = computed(() =>
  [...searchResults.value.reduce((groups, hit) => {
    const list = groups.get(hit.path)
    if (list) list.push(hit)
    else groups.set(hit.path, [hit])
    return groups
  }, new Map<string, SearchMatch[]>())].map(([path, hits]) => ({
    path,
    name: path.replace(/\\/g, '/').split('/').pop() ?? path,
    dir: relativeDir(path),
    hits,
  }))
)

function toggleGroup(path: string) {
  const next = new Set(collapsedFiles.value)
  if (next.has(path)) next.delete(path)
  else next.add(path)
  collapsedFiles.value = next
}

/** A hit's line, split around the match so the middle can be highlighted. */
function splitHit(hit: SearchMatch): { before: string; match: string; after: string } {
  const before = hit.line.slice(0, hit.matchStart)
  return {
    // Long leading context pushes the match off-screen; trim it, keep a marker.
    before: before.length > 40 ? '…' + before.slice(-40) : before,
    match: hit.line.slice(hit.matchStart, hit.matchEnd),
    after: hit.line.slice(hit.matchEnd),
  }
}

/** Open the file a hit is in, at its line. */
async function openHit(hit: SearchMatch) {
  try {
    const content = await invoke<string>('plugin:canvas|read_file', { path: hit.path })
    emit('open', { path: hit.path, content, kind: 'text', line: hit.lineNumber })
  } catch {
    emit('open', { path: hit.path, content: '', kind: 'text', line: hit.lineNumber })
  }
}

// Host asked for a re-walk (a file was written outside the tree).
watch(() => props.refreshKey, () => {
  loadFileTree()
})

function toggleExpand(node: FileNode) {
  if (node.isDirectory) {
    node.expanded = !node.expanded
  }
}

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif'])

function isImageFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  return IMAGE_EXTS.has(ext)
}

async function openFile(node: FileNode) {
  if (node.isDirectory) {
    toggleExpand(node)
    return
  }

  const fullPath = getFullPath(node)

  // Images come back base64 and go up as a data URL — every host renders them
  // in an <img>, none of them wants the bytes.
  if (isImageFile(node.name)) {
    try {
      const result = await invoke<{ base64Data: string; mimeType: string; sizeBytes: number }>(
        'plugin:canvas|read_file_binary',
        { path: fullPath }
      )
      emit('open', {
        path: fullPath,
        content: `data:${result.mimeType};base64,${result.base64Data}`,
        kind: 'image',
      })
    } catch {
      emit('open', { path: fullPath, content: '', kind: 'image' })
    }
    return
  }

  try {
    const content = await invoke<string>('plugin:canvas|read_file', { path: node.path })
    emit('open', { path: node.path, content, kind: 'text' })
    return
  } catch {
    // Fall through — a relative path from demo data needs the workspace prefix.
  }

  try {
    // A relative fallback against the ACTIVE workspace would be wrong in
    // General mode, where the row can belong to any root — and there every
    // path is absolute anyway.
    const workspace = currentWorkspace.value
    if (workspace && !generalMode.value) {
      const content = await invoke<string>('plugin:canvas|read_file', {
        path: workspace.path + '/' + node.path,
      })
      emit('open', { path: node.path, content, kind: 'text' })
      return
    }
  } catch {
    // Neither path resolved; say so in the buffer rather than opening nothing.
  }

  emit('open', { path: node.path, content: `// Could not read ${node.name}\n`, kind: 'text' })
}

async function onNewFileInFolder(node: FileNode) {
  const name = prompt('File name:')
  if (!name) return
  const dir = node.isDirectory ? node.path : getParentDir(node.path)
  try {
    await invoke('plugin:canvas|create_file', { path: dir + '/' + name, isDirectory: false })
    loadFileTree()
  } catch { /* ignore */ }
}

function onContextMenu(e: MouseEvent, node: FileNode) {
  e.preventDefault()
  e.stopPropagation()
  contextMenu.value = { visible: true, x: e.clientX, y: e.clientY, node }
}

function normalizePath(p: string): string {
  return p.replace(/\\/g, '/')
}

const explorerTreeRef = ref<HTMLElement | null>(null)
let pointerStartX = 0
let pointerStartY = 0
let pointerDidMove = false
let pointerMoveHandler: ((e: PointerEvent) => void) | null = null
let pointerUpHandler: (() => void) | null = null

function findDropFolderAtPoint(x: number, y: number, draggingPath: string): string | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null
  if (!el) return null
  // Check if hovering a folder row directly
  const folderRow = el.closest?.('[data-is-dir]') as HTMLElement | null
  if (folderRow) {
    const path = folderRow.getAttribute('data-path')
    if (path && path !== draggingPath) return path
  }
  // If hovering a file row, use its parent folder (the .tree-children ancestor's sibling folder row)
  const anyRow = el.closest?.('[data-path]') as HTMLElement | null
  if (anyRow && !anyRow.hasAttribute('data-is-dir')) {
    // Walk up to .tree-children, then its parent wrapper has the folder row
    const treeChildren = anyRow.closest?.('.tree-children') as HTMLElement | null
    if (treeChildren) {
      const parentFolder = treeChildren.previousElementSibling?.closest?.('[data-is-dir]') as HTMLElement | null
        || treeChildren.parentElement?.querySelector?.(':scope > [data-is-dir]') as HTMLElement | null
      if (parentFolder) {
        const path = parentFolder.getAttribute('data-path')
        if (path && path !== draggingPath) return path
      }
    }
    // Fallback: file at root level — use workspace. Ambiguous with N roots,
    // so General mode only drops onto explicit folder rows.
    const ws = currentWorkspace.value
    if (ws && !generalMode.value) return ws.path
  }
  // Check if hovering empty tree area
  if (el.closest?.('.explorer-tree')) {
    const ws = currentWorkspace.value
    if (ws && !generalMode.value) return ws.path
  }
  return null
}

function onRowPointerDown(e: PointerEvent) {
  if (e.button !== 0) return
  const target = e.target as HTMLElement
  if (target.closest('button, input')) return
  const row = target.closest?.('[data-path]') as HTMLElement | null
  if (!row) return
  const path = row.getAttribute('data-path')
  if (!path) return

  e.preventDefault()
  pointerStartX = e.clientX
  pointerStartY = e.clientY
  pointerDidMove = false

  pointerMoveHandler = (me: PointerEvent) => {
    const dx = me.clientX - pointerStartX
    const dy = me.clientY - pointerStartY
    if (!pointerDidMove && Math.abs(dx) + Math.abs(dy) < 4) return
    pointerDidMove = true
    dragSourcePath.value = path
    row.style.opacity = '0.35'
    document.body.classList.add('qc-file-dragging')

    const folder = findDropFolderAtPoint(me.clientX, me.clientY, path)
    if (folder) {
      const srcNorm = normalizePath(path)
      const folderNorm = normalizePath(folder)
      if (folderNorm === srcNorm || folderNorm === normalizePath(getParentDir(path)) || folderNorm.startsWith(srcNorm + '/')) {
        dropTargetPath.value = null
      } else {
        dropTargetPath.value = folder
      }
    } else {
      dropTargetPath.value = null
    }
  }

  pointerUpHandler = async () => {
    document.removeEventListener('pointermove', pointerMoveHandler!, true)
    document.removeEventListener('pointerup', pointerUpHandler!, true)
    document.body.classList.remove('qc-file-dragging')
    row.style.opacity = ''

    if (pointerDidMove && dragSourcePath.value && dropTargetPath.value) {
      const srcNorm = normalizePath(dragSourcePath.value)
      const tgtNorm = normalizePath(dropTargetPath.value)
      const fileName = srcNorm.split('/').pop()
      if (fileName && srcNorm !== tgtNorm) {
        const newPath = tgtNorm + '/' + fileName
        try {
          await invoke('plugin:canvas|rename_file', { oldPath: dragSourcePath.value, newPath })
          loadFileTree()
        } catch (err) {
          console.error('Failed to move file:', err)
        }
      }
    }

    dragSourcePath.value = null
    dropTargetPath.value = null
  }

  document.addEventListener('pointermove', pointerMoveHandler, true)
  document.addEventListener('pointerup', pointerUpHandler, true)
}

function resetDragState() {
  if (pointerMoveHandler) document.removeEventListener('pointermove', pointerMoveHandler, true)
  if (pointerUpHandler) document.removeEventListener('pointerup', pointerUpHandler, true)
  document.body.classList.remove('qc-file-dragging')
  dragSourcePath.value = null
  dropTargetPath.value = null
}

function onTreeContextMenu(e: MouseEvent) {
  // Only trigger on empty space, not on nodes
  if (e.target !== e.currentTarget) return
  e.preventDefault()
  showRootContextMenu(e)
}

function onWorkspaceHeaderContextMenu(e: MouseEvent) {
  showRootContextMenu(e)
}

function showRootContextMenu(e: MouseEvent) {
  // General mode has no single root; its per-workspace root rows are normal
  // folder nodes and get the regular context menu.
  if (generalMode.value) return
  const workspace = currentWorkspace.value
  if (!workspace) return
  contextMenu.value = {
    visible: true,
    x: e.clientX,
    y: e.clientY,
    node: { name: workspaceName.value, path: workspace.path, isDirectory: true, children: [] } as FileNode,
  }
}

function getParentDir(filePath: string): string {
  // Handle both forward and backslash separators
  const lastSep = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'))
  return lastSep > 0 ? filePath.substring(0, lastSep) : filePath
}

async function contextAction(action: string) {
  const node = contextMenu.value.node
  contextMenu.value.visible = false
  if (!node) return

  switch (action) {
    case 'new-file': {
      const name = prompt('File name:')
      if (!name) return
      const dir = node.isDirectory ? node.path : getParentDir(node.path)
      try {
        await invoke('plugin:canvas|create_file', { path: dir + '/' + name, isDirectory: false })
        loadFileTree()
      } catch { /* ignore */ }
      break
    }
    case 'new-folder': {
      const name = prompt('Folder name:')
      if (!name) return
      const dir = node.isDirectory ? node.path : getParentDir(node.path)
      try {
        await invoke('plugin:canvas|create_file', { path: dir + '/' + name, isDirectory: true })
        loadFileTree()
      } catch { /* ignore */ }
      break
    }
    case 'rename': {
      const newName = prompt('New name:', node.name)
      if (!newName || newName === node.name) return
      try {
        const parentDir = getParentDir(node.path)
        await invoke('plugin:canvas|rename_file', {
          oldPath: node.path,
          newPath: parentDir + '/' + newName,
        })
        loadFileTree()
      } catch { /* ignore */ }
      break
    }
    case 'delete': {
      pendingDeleteNode.value = node
      confirmDeleteVisible.value = true
      break
    }
    case 'copy-path': {
      try {
        await navigator.clipboard.writeText(node.path)
      } catch { /* ignore */ }
      break
    }
    case 'cd': {
      emit('cd', node.path)
      break
    }
    case 'open-on-canvas': {
      if (!node.isDirectory) {
        openFileOnCanvas(node)
      }
      break
    }
  }
}

async function handleDelete(confirmed: boolean) {
  confirmDeleteVisible.value = false
  if (!confirmed || !pendingDeleteNode.value) {
    pendingDeleteNode.value = null
    return
  }
  const node = pendingDeleteNode.value
  pendingDeleteNode.value = null
  try {
    await invoke('plugin:canvas|delete_file', { path: node.path })
    loadFileTree()
    emit('deleted', node.path)
  } catch { /* ignore */ }
}

function openFileOnCanvas(node: FileNode) {
  // Emit a custom event that InfinityCanvas listens for
  const event = new CustomEvent('qc-open-file-on-canvas', {
    detail: { filePath: getFullPath(node) },
  })
  window.dispatchEvent(event)
}

function getFullPath(node: FileNode): string {
  // If the path is already absolute, use it directly
  if (node.path.includes(':') || node.path.startsWith('/')) {
    return node.path
  }
  // Otherwise, prepend workspace folder — never in General mode, where the
  // node can belong to any root and paths are absolute by construction.
  const workspace = currentWorkspace.value
  if (workspace && !generalMode.value) {
    return workspace.path + '/' + node.path
  }
  return node.path
}

const gitStatusStyle: Record<GitFileStatus, { letter: string; color: string }> = {
  modified: { letter: 'M', color: 'text-amber-400' },
  untracked: { letter: 'U', color: 'text-green-400' },
  staged: { letter: 'S', color: 'text-blue-400' },
  deleted: { letter: 'D', color: 'text-red-400' },
  renamed: { letter: 'R', color: 'text-purple-400' },
  clean: { letter: '', color: '' },
}

function emitSelect() {
  emit('select', {
    general: generalMode.value,
    // The explicit pin, apart from the no-workspace fallback: a host that
    // keeps its own General flag (the canvas board) must not latch onto the
    // fallback, or a workspace picked while this explorer is hidden would
    // leave that host on General.
    pinned: selection.value.general,
    workspaceId: generalMode.value ? null : currentWorkspace.value?.id ?? null,
    path: generalMode.value ? null : currentWorkspace.value?.path ?? null,
  })
}

// Fires when the registry loads, when this host's pin resolves, and — for
// an unpinned host — when the suite-global active changes. A pinned host
// never sees a change here unless its own selection moved: that is the
// whole point of per-host selection.
watch(() => currentWorkspace.value?.id, () => {
  if (!generalMode.value) loadFileTree()
})

// The host's middle area follows every effective view change, including the
// initial resolution once the registry has loaded.
watch([generalMode, () => currentWorkspace.value?.id], () => emitSelect())

// The General roots include the suite-active workspace; keep them fresh.
watch(() => workspacesStore.activeWorkspaceId, () => {
  if (generalMode.value) void loadFileTree(true)
})

// A workspace added or removed changes what the General tree IS.
watch(() => workspacesStore.workspaces.length, () => {
  treeCache.delete(GENERAL_WS)
  if (generalMode.value) void loadFileTree()
})

onMounted(() => {
  // The store reads the registry itself — whichever module hosts this explorer
  // first (canvas or console) finds it filled. Until 2026-08-26 the console
  // reached across the module boundary to do this.
  void workspacesStore.ensureLoaded()

  // Cached tree first — instant paint on re-entry — then refresh silently.
  const cacheKey = generalMode.value ? GENERAL_WS : currentWorkspace.value?.id
  const cached = cacheKey ? treeCache.get(cacheKey) : undefined
  if (cached) {
    fileTree.value = cached
    void loadFileTree(true)
  } else {
    void loadFileTree()
  }
  explorerTreeRef.value?.addEventListener('pointerdown', onRowPointerDown)

  // A restored General view has no currentWorkspace change to ride on —
  // announce it so the host's middle area switches to the General context.
  if (generalMode.value) emitSelect()
})

onUnmounted(() => {
  explorerTreeRef.value?.removeEventListener('pointerdown', onRowPointerDown)
  resetDragState()
})
</script>

<template>
  <div class="explorer" :style="{ background: 'var(--qc-bg-titlebar)' }">
    <!-- Workspace selector dropdown -->
    <!-- Top row: which workspace, and a refresh for its tree. The refresh
         used to sit beside the search box; it belongs to the tree, and the
         search box no longer searches the tree (user, 2026-08-26). -->
    <div class="explorer-top">
      <div ref="wsDropdownRef" class="ws-selector">
      <button class="ws-trigger" @click="wsDropdownOpen = !wsDropdownOpen">
        <div class="ws-trigger-text">
          <span class="ws-parent-path">{{ activeWsParentPath }}</span>
          <span class="ws-folder-name">{{ activeWsFolderName }}</span>
        </div>
        <svg class="ws-caret" :class="{ 'ws-caret--open': wsDropdownOpen }" width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
          <path d="M3 4l2 2.5L7 4" stroke="currentColor" stroke-width="1.2" fill="none" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <!-- The one workspace selector in QuantCode. It lists WORKSPACES —
           each of which is a folder — not the roots of one; multi-root is gone
           (docs/PLAN-WORKSPACES.md). The suite titlebar has no workspace chip:
           workspaces bind this app and nothing else. -->
      <Transition name="ws-drop">
        <div v-if="wsDropdownOpen" class="ws-menu">
          <!-- The gigabrain view: every registered workspace as one tree.
               A view over the registry, never a workspace — no pin, no
               remove, and core/workspace.active stays untouched. -->
          <div
            class="ws-menu-item ws-menu-general"
            :class="{ 'ws-menu-item--active': generalMode }"
            role="button"
            tabindex="0"
            @click="selectGeneral"
            @keydown.enter="selectGeneral"
          >
            <span class="ws-menu-item-name">General</span>
            <span class="ws-menu-item-path">every workspace, one tree</span>
          </div>
          <div class="ws-menu-sep" />

          <div class="ws-menu-label">Workspaces</div>

          <p v-if="!workspacesStore.sorted.length" class="ws-menu-empty">
            No workspaces yet
          </p>

          <div
            v-for="w in workspacesStore.sorted"
            :key="w.id"
            class="ws-menu-item"
            :class="{ 'ws-menu-item--active': !generalMode && w.id === currentWorkspace?.id }"
            :title="w.path"
            role="button"
            tabindex="0"
            @click="selectWorkspace(w.id)"
            @keydown.enter="selectWorkspace(w.id)"
          >
            <span class="ws-menu-item-name">{{ w.name }}</span>
            <span class="ws-menu-item-path">{{ parentPathOf(w.path) }}</span>
            <button
              class="ws-menu-item-icon"
              :class="{ 'is-pinned': w.pinned }"
              :title="w.pinned ? 'Unpin' : 'Pin to top'"
              @click.stop="workspacesStore.togglePin(w.id)"
            >
              <svg width="11" height="11" viewBox="0 0 24 24" :fill="w.pinned ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12 17v5 M9 3h6l1 7 3 2H5l3-2 1-7z" />
              </svg>
            </button>
            <button
              class="ws-menu-item-remove"
              title="Remove from list (folder stays on disk)"
              @click.stop="workspacesStore.removeWorkspace(w.id)"
            >&times;</button>
          </div>

          <div class="ws-menu-sep" />
          <button class="ws-menu-item ws-menu-add" @click="openWorkspaceFolder">
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
              <line x1="6" y1="2" x2="6" y2="10" />
              <line x1="2" y1="6" x2="10" y2="6" />
            </svg>
            Open folder...
          </button>
        </div>
      </Transition>
      </div>

      <button class="explorer-icon-btn" title="Refresh file tree" @click="loadFileTree()">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="23 4 23 10 17 10" />
          <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
        </svg>
      </button>
    </div>

    <!-- Bottom row: search INSIDE files, with VS Code's filter panel behind
         the funnel. File names are Ctrl+P in the module header. -->
    <div class="explorer-search">
      <div class="explorer-search-wrapper" :class="{ 'explorer-search-wrapper--focused': searchFocused }">
        <svg class="explorer-search-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search in files"
          spellcheck="false"
          class="explorer-search-input"
          @focus="searchFocused = true"
          @blur="searchFocused = false"
          @keydown.enter="runSearch"
          @keydown.esc="clearSearch"
        />
        <span v-if="searching" class="explorer-loading-dot explorer-search-busy" />
        <button v-else-if="searchQuery" class="explorer-search-clear" title="Clear" @click="clearSearch">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>

      <button
        class="explorer-icon-btn"
        :class="{ 'is-on': filterOpen || filtersActive }"
        title="Search filters"
        @click="filterOpen = true"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 5h18l-7 8v6l-4 2v-8z" />
        </svg>
        <span v-if="filtersActive" class="explorer-filter-dot" />
      </button>
    </div>

    <!-- Search filters. A real modal, teleported to `body`: the sidebar is
         220px wide with `overflow: hidden`, so a popover anchored inside it was
         clipped on the left (user, 2026-08-26). -->
    <Teleport to="body">
      <Transition name="fm-fade">
        <div v-if="filterOpen" class="fm-overlay" @mousedown.self="filterOpen = false">
          <div class="fm-panel" role="dialog" aria-modal="true" aria-label="Search filters">
            <header class="fm-head">
              <h2 class="fm-title">Search filters</h2>
              <button class="fm-close" title="Close" @click="filterOpen = false">
                <svg width="12" height="12" viewBox="0 0 10 10" fill="none">
                  <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" />
                </svg>
              </button>
            </header>

            <div class="fm-body">
              <label class="fm-field">
                <span class="fm-label">Files to include</span>
                <input v-model="filters.include" type="text" placeholder="e.g. *.ts, src/**" spellcheck="false" />
                <span class="fm-hint">Comma-separated globs. Empty searches everything.</span>
              </label>

              <label class="fm-field">
                <span class="fm-label">Files to exclude</span>
                <input v-model="filters.exclude" type="text" placeholder="e.g. *.spec.ts, dist/**" spellcheck="false" />
                <span class="fm-hint">On top of .gitignore, which is always honoured.</span>
              </label>

              <div class="fm-field">
                <span class="fm-label">Matching</span>
                <div class="fm-switches">
                  <button
                    class="fm-switch"
                    :class="{ 'is-on': filters.caseSensitive }"
                    @click="filters.caseSensitive = !filters.caseSensitive"
                  >
                    <span class="fm-switch-mark">Aa</span>
                    <span class="fm-switch-text">Match case</span>
                  </button>
                  <button
                    class="fm-switch"
                    :class="{ 'is-on': filters.wholeWord }"
                    @click="filters.wholeWord = !filters.wholeWord"
                  >
                    <span class="fm-switch-mark">ab</span>
                    <span class="fm-switch-text">Whole word</span>
                  </button>
                  <button
                    class="fm-switch"
                    :class="{ 'is-on': filters.regex }"
                    @click="filters.regex = !filters.regex"
                  >
                    <span class="fm-switch-mark">.*</span>
                    <span class="fm-switch-text">Regular expression</span>
                  </button>
                </div>
              </div>
            </div>

            <footer class="fm-foot">
              <button class="fm-btn" :disabled="!filtersActive" @click="resetFilters">Reset</button>
              <button class="fm-btn fm-btn--primary" @click="filterOpen = false">Done</button>
            </footer>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Results, while a query is running    <!-- Results, while a query is running — the tree is behind them. -->
    <div v-if="searchQuery.trim()" class="explorer-tree scrollbar-hover">
      <p v-if="searchError" class="explorer-empty is-error">{{ searchError }}</p>
      <p v-else-if="searching && !searchResults.length" class="explorer-empty">
        <span class="explorer-loading-dot" />
        Searching...
      </p>
      <p v-else-if="!searchResults.length" class="explorer-empty">No results</p>

      <template v-else>
        <p class="results-summary">
          {{ searchResults.length }}{{ searchTruncated ? '+' : '' }} in {{ groupedResults.length }} file{{ groupedResults.length === 1 ? '' : 's' }}
        </p>

        <div v-for="group in groupedResults" :key="group.path" class="result-group">
          <button class="result-file" :title="group.path" @click="toggleGroup(group.path)">
            <span class="result-caret" :class="{ 'is-open': !collapsedFiles.has(group.path) }">
              <svg width="9" height="9" viewBox="0 0 10 10" fill="currentColor"><path d="M3 2l4 3-4 3z" /></svg>
            </span>
            <span class="result-name">{{ group.name }}</span>
            <span class="result-dir">{{ group.dir }}</span>
            <span class="result-count">{{ group.hits.length }}</span>
          </button>

          <template v-if="!collapsedFiles.has(group.path)">
            <button
              v-for="hit in group.hits"
              :key="hit.lineNumber + ':' + hit.matchStart"
              class="result-hit"
              :title="`${group.name}:${hit.lineNumber}`"
              @click="openHit(hit)"
            >
              <span class="result-line">{{ hit.lineNumber }}</span>
              <span class="result-text">{{ splitHit(hit).before }}<mark>{{ splitHit(hit).match }}</mark>{{ splitHit(hit).after }}</span>
            </button>
          </template>
        </div>
      </template>
    </div>

    <!-- File tree -->    <!-- File tree — stands down while search results are up. -->
    <div
      v-show="!searchQuery.trim()"
      ref="explorerTreeRef"
      class="explorer-tree scrollbar-hover"
      @contextmenu="onTreeContextMenu"
    >
      <!-- Root folder group with guide line. General mode renders through
           the flat branch below instead: one synthetic root per workspace. -->
      <div v-if="workspaceName && !generalMode" class="explorer-root-group">
        <!-- Workspace header row (root node) -->
        <div
          class="explorer-root-row"
          @click.stop="rootExpanded = !rootExpanded"
          @contextmenu.prevent="onWorkspaceHeaderContextMenu"
        >
          <span class="explorer-root-caret" :class="{ 'explorer-root-caret--open': rootExpanded }">
            <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
              <path d="M3 2l4 3-4 3z" />
            </svg>
          </span>
          <span class="explorer-root-name">{{ workspaceName }}</span>
        </div>

        <!-- Tree content indented under root -->
        <div v-show="rootExpanded" class="explorer-root-children">
          <div v-if="loading" class="explorer-empty">
            <span class="explorer-loading-dot" />
            Loading...
          </div>
          <template v-else-if="fileTree.length">
            <CanvasSidebarFileTreeNode
              v-for="node in fileTree"
              :key="node.path"
              :node="node"
              :depth="1"
              :git-status-style="gitStatusStyle"
              :selected-path="selectedPath"
              :drop-target-path="dropTargetPath"
              @toggle="toggleExpand"
              @open="openFile"
              @contextmenu="onContextMenu"
              @new-file="onNewFileInFolder"
            />
          </template>
        </div>
      </div>

      <!-- Fallback: no workspace -->
      <template v-else>
        <div v-if="loading" class="explorer-empty">
          <span class="explorer-loading-dot" />
          Loading...
        </div>
        <template v-else-if="fileTree.length">
          <CanvasSidebarFileTreeNode
            v-for="node in fileTree"
            :key="node.path"
            :node="node"
            :depth="0"
            :git-status-style="gitStatusStyle"
            :selected-path="selectedPath"
            :drop-target-path="dropTargetPath"
            @toggle="toggleExpand"
            @open="openFile"
            @contextmenu="onContextMenu"
            @new-file="onNewFileInFolder"
          />
        </template>
      </template>
    </div>

    <!-- Context menu -->
    <Teleport to="body">
      <Transition name="ctx">
        <div
          v-if="contextMenu.visible"
          ref="contextMenuRef"
          class="ctx-menu"
          :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
        >
          <button class="ctx-item" @click="contextAction('new-file')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9" y1="14" x2="15" y2="14"/></svg>
            New File
          </button>
          <button class="ctx-item" @click="contextAction('new-folder')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9" y1="14" x2="15" y2="14"/></svg>
            New Folder
          </button>
          <div class="ctx-sep" />
          <button class="ctx-item" @click="contextAction('rename')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
            Rename
          </button>
          <button class="ctx-item ctx-item--danger" @click="contextAction('delete')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
            Delete
          </button>
          <div class="ctx-sep" />
          <button class="ctx-item" @click="contextAction('copy-path')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            Copy Path
          </button>
          <template v-if="contextMenu.node && !contextMenu.node.isDirectory">
            <div class="ctx-sep" />
            <button class="ctx-item" @click="contextAction('open-on-canvas')">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="9" y1="21" x2="9" y2="9"/></svg>
              Open on Canvas
            </button>
          </template>
          <!-- Host-specific (see the props doc): the console's quick travel. -->
          <template v-if="props.cdEnabled && contextMenu.node?.isDirectory">
            <div class="ctx-sep" />
            <button class="ctx-item" @click="contextAction('cd')">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>
              Change Directory
            </button>
          </template>
        </div>
      </Transition>
    </Teleport>

    <!-- Delete confirmation modal -->
    <Teleport to="body">
      <Transition name="modal">
        <div
          v-if="confirmDeleteVisible"
          class="modal-overlay"
          @click.self="handleDelete(false)"
        >
          <div class="modal-content">
            <div class="modal-title">Delete {{ pendingDeleteNode?.isDirectory ? 'Folder' : 'File' }}</div>
            <p class="modal-desc">
              Are you sure you want to delete
              <span class="modal-filename">"{{ pendingDeleteNode?.name }}"</span>?
              This cannot be undone.
            </p>
            <div class="modal-actions">
              <button class="modal-btn modal-btn--cancel" @click="handleDelete(false)">
                Cancel
              </button>
              <button class="modal-btn modal-btn--delete" @click="handleDelete(true)">
                Delete
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Settings moved to the module header's gear (V3) — the sidebar foot
         row is gone suite-wide. -->
  </div>
</template>

<style scoped>

.explorer {
  display: flex;
  flex-direction: column;
  height: 100%;
  font-size: 13px;
}

/* ---- Workspace selector ---- */
/* Top row: workspace trigger + refresh, side by side. */
.explorer-top {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  padding: 8px 8px 0;
}

.ws-selector {
  position: relative;
  flex: 1;
  min-width: 0;
}

.ws-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text);
  cursor: pointer;
  font-size: 11px;
  font-weight: 500;
  transition: border-color 0.15s, background-color 0.15s;
  gap: 4px;
  min-height: 28px;
}

.ws-trigger:hover {
  border-color: color-mix(in srgb, var(--qc-text) 25%, transparent);
  background: color-mix(in srgb, var(--qc-text) 4%, transparent);
}

.ws-trigger-text {
  display: flex;
  align-items: baseline;
  gap: 2px;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.ws-parent-path {
  color: var(--qc-text-muted);
  opacity: 0.4;
  font-size: 10px;
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ws-folder-name {
  flex-shrink: 0;
  font-weight: 600;
}

.ws-caret {
  flex-shrink: 0;
  color: var(--qc-text-muted);
  opacity: 0.5;
  transition: transform 0.15s;
}

.ws-caret--open {
  transform: rotate(180deg);
}

.ws-menu {
  position: absolute;
  top: calc(100% + 2px);
  left: 0;
  right: 0;
  background: var(--qc-bg-header);
  border: 1px solid var(--qc-border);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  z-index: 100;
  padding: 4px;
  max-height: 240px;
  overflow-y: auto;
}

.ws-menu-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 5px 8px;
  font-size: 11px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qc-text);
  cursor: pointer;
  transition: background 0.1s;
  text-align: left;
  gap: 4px;
}

/* The gigabrain entry reads as a mode, not another folder. */
.ws-menu-general .ws-menu-item-name {
  font-weight: 600;
}

.ws-menu-item:hover {
  background: var(--qc-bg-surface);
}

.ws-menu-item--active {
  font-weight: 700;
}

.ws-menu-item--active::before {
  content: '';
  display: inline-block;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--qc-text);
  margin-right: 4px;
  flex-shrink: 0;
}

.ws-menu-item-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ws-menu-item-remove {
  display: none;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--qc-text-muted);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  flex-shrink: 0;
  transition: color 0.1s;
}

.ws-menu-item:hover .ws-menu-item-remove {
  display: flex;
}

.ws-menu-item-remove:hover {
  color: #ef4444;
}

.ws-menu-sep {
  height: 1px;
  margin: 3px 4px;
  background: var(--qc-border);
}

.ws-menu-label {
  padding: 4px 10px 2px;
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--qc-text-dim);
}

.ws-menu-empty {
  padding: 6px 10px 8px;
  font-size: 11px;
  color: var(--qc-text-muted);
}

.ws-menu-item-path {
  flex-shrink: 0;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  color: var(--qc-text-muted);
  opacity: 0.6;
}

.ws-menu-item-icon {
  display: none;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--qc-text-muted);
  cursor: pointer;
  flex-shrink: 0;
  transition: color 0.1s;
}

.ws-menu-item:hover .ws-menu-item-icon,
.ws-menu-item-icon.is-pinned {
  display: flex;
}

.ws-menu-item-icon:hover,
.ws-menu-item-icon.is-pinned {
  color: var(--qc-text);
}

.ws-menu-add {
  color: var(--qc-text-muted);
  gap: 6px;
}

.ws-menu-add:hover {
  color: var(--qc-text);
}

/* Workspace dropdown transition */
.ws-drop-enter-active,
.ws-drop-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.ws-drop-enter-from,
.ws-drop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

/* ---- Search bar ---- */
/* One icon button for the header: refresh on top, filter below. */
.explorer-icon-btn {
  position: relative;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text-muted);
  cursor: pointer;
  transition: color 0.12s ease, background-color 0.12s ease, border-color 0.12s ease;
}
.explorer-icon-btn:hover {
  color: var(--qc-text);
  background: var(--qc-bg-surface);
}
.explorer-icon-btn.is-on {
  color: var(--qc-text);
  border-color: var(--qc-border);
  background: var(--qc-bg-surface);
}

/* A filter is set but the panel is closed — say so without opening it. */
.explorer-filter-dot {
  position: absolute;
  top: 3px;
  right: 3px;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--qc-accent, currentColor);
}

.explorer-filter {
  position: relative;
  flex-shrink: 0;
}

.explorer-search-busy {
  position: absolute;
  right: 8px;
}

/* ── Search filters, as a modal ──
   Teleported to `body` and fixed to the viewport. Anchored inside the sidebar
   it was clipped: 220px wide with `overflow: hidden` (user, 2026-08-26).
   Geometry follows QSettingsModal so the two read as the same kind of surface. */
.fm-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: grid;
  place-items: center;
  background: rgb(11 11 15 / 0.6);
}

.fm-panel {
  display: flex;
  flex-direction: column;
  width: 420px;
  max-width: calc(100% - 48px);
  max-height: 80%;
  border: 1px solid var(--qc-border);
  border-radius: 12px;
  overflow: hidden;
  background: var(--qc-bg);
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.4);
}

.fm-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  padding: 14px 16px;
  border-bottom: 1px solid var(--qc-border);
  background: var(--qc-bg-header);
}

.fm-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--qc-text);
}

.fm-close {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text-muted);
  cursor: pointer;
}
.fm-close:hover {
  background: var(--qc-bg-surface);
  color: var(--qc-text);
}

.fm-body {
  padding: 16px;
  overflow-y: auto;
}

.fm-field {
  display: block;
}
.fm-field + .fm-field {
  margin-top: 16px;
}

.fm-label {
  display: block;
  margin-bottom: 5px;
  font-size: 11px;
  font-weight: 600;
  color: var(--qc-text);
}

.fm-hint {
  display: block;
  margin-top: 5px;
  font-size: 10.5px;
  color: var(--qc-text-dim);
}

.fm-field input {
  width: 100%;
  padding: 7px 9px;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  background: var(--qc-bg-input, var(--qc-bg-surface));
  color: var(--qc-text);
  font-family: var(--qc-font-mono, monospace);
  font-size: 11.5px;
  outline: none;
}
.fm-field input:focus {
  border-color: color-mix(in srgb, var(--qc-text) 30%, transparent);
}

.fm-switches {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

/* A row, not a square: the modal has the width for the switch to say what it
   does, which the 26px popover button never did. */
.fm-switch {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text-muted);
  font-size: 11.5px;
  text-align: left;
  cursor: pointer;
  transition: color 0.12s ease, background-color 0.12s ease, border-color 0.12s ease;
}
.fm-switch:hover {
  color: var(--qc-text);
  background: var(--qc-bg-surface);
}
.fm-switch.is-on {
  color: var(--qc-text);
  background: var(--qc-bg-surface);
  border-color: color-mix(in srgb, var(--qc-text) 30%, transparent);
}

.fm-switch-mark {
  flex-shrink: 0;
  width: 26px;
  text-align: center;
  font-family: var(--qc-font-mono, monospace);
  font-size: 11px;
}

.fm-switch-text {
  flex: 1;
  min-width: 0;
}

.fm-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  flex-shrink: 0;
  padding: 12px 16px;
  border-top: 1px solid var(--qc-border);
  background: var(--qc-bg-header);
}

.fm-btn {
  padding: 6px 14px;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text-muted);
  font-size: 11.5px;
  cursor: pointer;
  transition: color 0.12s ease, background-color 0.12s ease;
}
.fm-btn:hover:not(:disabled) {
  color: var(--qc-text);
  background: var(--qc-bg-surface);
}
.fm-btn:disabled {
  opacity: 0.35;
  cursor: default;
}
.fm-btn--primary {
  color: var(--qc-text);
  background: var(--qc-bg-surface);
}

.fm-fade-enter-active,
.fm-fade-leave-active {
  transition: opacity 0.14s ease;
}
.fm-fade-enter-from,
.fm-fade-leave-to {
  opacity: 0;
}

/* ── Results ── */
.results-summary {
  padding: 6px 10px 4px;
  font-size: 10px;
  color: var(--qc-text-dim);
}

.result-group {
  margin-bottom: 2px;
}

.result-file {
  display: flex;
  align-items: baseline;
  gap: 5px;
  width: 100%;
  padding: 3px 10px;
  border: none;
  background: transparent;
  color: var(--qc-text);
  font-size: 0.78rem;
  text-align: left;
  cursor: pointer;
}
.result-file:hover {
  background: var(--qc-bg-surface);
}

.result-caret {
  flex-shrink: 0;
  color: var(--qc-text-dim);
  transition: transform 0.12s ease;
}
.result-caret.is-open {
  transform: rotate(90deg);
}

.result-name {
  flex-shrink: 0;
}

.result-dir {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  color: var(--qc-text-dim);
}

.result-count {
  flex-shrink: 0;
  min-width: 15px;
  padding: 0 4px;
  border-radius: 7px;
  background: var(--qc-bg-surface);
  font-size: 9.5px;
  line-height: 15px;
  text-align: center;
  color: var(--qc-text-muted);
}

.result-hit {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 2px 10px 2px 24px;
  border: none;
  background: transparent;
  color: var(--qc-text-muted);
  font-family: var(--qc-font-mono, monospace);
  font-size: 11px;
  text-align: left;
  cursor: pointer;
}
.result-hit:hover {
  background: var(--qc-bg-surface);
  color: var(--qc-text);
}

.result-line {
  flex-shrink: 0;
  min-width: 26px;
  text-align: right;
  color: var(--qc-text-dim);
  font-size: 10px;
}

.result-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.result-text mark {
  background: color-mix(in srgb, #f59e0b 35%, transparent);
  color: var(--qc-text);
  border-radius: 2px;
}

.explorer-empty.is-error {
  color: #f87171;
}

.explorer-search {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 8px 6px;
  flex-shrink: 0;
}

.explorer-search-wrapper {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
}

.explorer-search-icon {
  position: absolute;
  left: 7px;
  color: var(--qc-text-muted);
  opacity: 0.4;
  pointer-events: none;
  transition: opacity 0.2s;
}

.explorer-search-wrapper--focused .explorer-search-icon {
  opacity: 0.7;
}

.explorer-search-input {
  width: 100%;
  padding: 5px 8px 5px 24px;
  font-family: var(--qc-font-mono, 'SF Mono', 'Cascadia Code', 'Fira Code', monospace);
  font-size: 0.78rem;
  font-weight: 400;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  color: var(--qc-text);
  background: transparent;
  transition: border-color 0.2s, background-color 0.2s;
  outline: none;
}

.explorer-search-input:focus {
  border-color: color-mix(in srgb, var(--qc-text) 30%, transparent);
  background: color-mix(in srgb, var(--qc-text) 3%, transparent);
}

.explorer-search-input::placeholder {
  color: var(--qc-text-muted);
  opacity: 0.3;
}

.explorer-search-clear {
  position: absolute;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 3px;
  background: none;
  color: var(--qc-text-muted);
  cursor: pointer;
  opacity: 0.4;
  transition: opacity 0.15s;
}

.explorer-search-clear:hover {
  opacity: 1;
}



.explorer-refresh:active {
  transform: scale(0.92);
}

/* ---- Root folder group (workspace header + guide line) ---- */
.explorer-root-group {
  position: relative;
}

.explorer-root-group::after {
  content: '';
  position: absolute;
  left: 14px;
  top: 22px;
  bottom: 4px;
  width: 1px;
  background: var(--qc-text);
  opacity: 0.07;
  pointer-events: none;
}

.explorer-root-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0 4px 8px;
  cursor: default;
  user-select: none;
  color: var(--qc-text-muted);
  transition: color 0.2s;
  border-right: 3px solid transparent;
}

.explorer-root-row:hover {
  color: var(--qc-text);
}

.explorer-root-caret {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  flex-shrink: 0;
  opacity: 0.5;
  transition: transform 0.15s ease, opacity 0.15s;
}

.explorer-root-caret--open {
  transform: rotate(90deg);
}

.explorer-root-row:hover .explorer-root-caret {
  opacity: 0.8;
}

.explorer-root-name {
  font-family: var(--qc-font-mono, 'SF Mono', 'Cascadia Code', 'Fira Code', monospace);
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--qc-text);
  letter-spacing: 0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
  text-transform: uppercase;
}

.explorer-root-children {
  position: relative;
}

/* ---- Tree area ---- */
.explorer-tree {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 2px 0 8px;
}

.explorer-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 64px;
  font-family: var(--qc-font-mono, monospace);
  font-size: 0.75rem;
  color: var(--qc-text-muted);
  opacity: 0.4;
}

.explorer-loading-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--qc-text-muted);
  animation: pulse 1s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 0.3; }
  50% { opacity: 0.8; }
}

/* ---- Smart scrollbar (hidden until hover) ---- */
.scrollbar-hover {
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: transparent transparent;
}

.scrollbar-hover::-webkit-scrollbar {
  width: 6px;
  background-color: transparent;
}

.scrollbar-hover::-webkit-scrollbar-track {
  background-color: transparent;
}

.scrollbar-hover::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 3px;
}

.scrollbar-hover:hover {
  scrollbar-color: color-mix(in srgb, var(--qc-text) 15%, transparent) transparent;
}

.scrollbar-hover:hover::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--qc-text) 15%, transparent);
}

.scrollbar-hover:hover::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--qc-text) 25%, transparent);
}

/* ---- Context menu ---- */
.ctx-menu {
  position: fixed;
  z-index: 9999;
  min-width: 170px;
  padding: 4px;
  border-radius: 10px;
  background: var(--qc-bg-header);
  border: 1px solid var(--qc-border);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.28), 0 2px 8px rgba(0, 0, 0, 0.12);
  backdrop-filter: blur(20px);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qc-text);
  font-family: var(--qc-font-mono, monospace);
  font-size: 0.75rem;
  font-weight: 400;
  cursor: pointer;
  transition: background-color 0.12s;
  text-align: left;
}

.ctx-item:hover {
  background: color-mix(in srgb, var(--qc-text) 10%, transparent);
}

.ctx-item--danger {
  color: #ef4444;
}

.ctx-item--danger:hover {
  background: color-mix(in srgb, #ef4444 12%, transparent);
}

.ctx-item svg {
  opacity: 0.5;
  flex-shrink: 0;
}

.ctx-sep {
  height: 1px;
  margin: 3px 6px;
  background: var(--qc-border);
  opacity: 0.6;
}

/* Context menu animation */
.ctx-enter-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.ctx-leave-active {
  transition: opacity 0.08s ease, transform 0.08s ease;
}
.ctx-enter-from {
  opacity: 0;
  transform: scale(0.96) translateY(-4px);
}
.ctx-leave-to {
  opacity: 0;
  transform: scale(0.98);
}

/* ---- Delete modal ---- */
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(8px);
}

.modal-content {
  min-width: 300px;
  max-width: 380px;
  padding: 20px;
  border-radius: 14px;
  background: var(--qc-bg-header);
  border: 1px solid var(--qc-border);
  box-shadow: 0 20px 48px rgba(0, 0, 0, 0.35);
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.modal-title {
  font-family: var(--qc-font-mono, monospace);
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--qc-text);
}

.modal-desc {
  font-size: 0.75rem;
  line-height: 1.5;
  color: var(--qc-text-muted);
  margin: 0;
}

.modal-filename {
  color: #ef4444;
  font-weight: 500;
}

.modal-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 4px;
}

.modal-btn {
  font-family: var(--qc-font-mono, monospace);
  font-size: 0.72rem;
  font-weight: 500;
  padding: 6px 14px;
  border-radius: 7px;
  border: none;
  cursor: pointer;
  transition: background-color 0.15s, transform 0.1s;
}

.modal-btn:active {
  transform: scale(0.97);
}

.modal-btn--cancel {
  background: transparent;
  color: var(--qc-text-muted);
  border: 1px solid var(--qc-border);
}

.modal-btn--cancel:hover {
  background: color-mix(in srgb, var(--qc-text) 8%, transparent);
  color: var(--qc-text);
}

.modal-btn--delete {
  background: #ef4444;
  color: white;
}

.modal-btn--delete:hover {
  background: #dc2626;
}

/* Modal animation */
.modal-enter-active {
  transition: opacity 0.15s ease;
}
.modal-enter-active .modal-content {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.modal-leave-active {
  transition: opacity 0.1s ease;
}
.modal-enter-from {
  opacity: 0;
}
.modal-enter-from .modal-content {
  opacity: 0;
  transform: scale(0.95) translateY(8px);
}
.modal-leave-to {
  opacity: 0;
}
</style>
