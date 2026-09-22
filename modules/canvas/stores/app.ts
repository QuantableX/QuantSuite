import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { v4 as uuidv4 } from 'uuid'
import type { EditorTab, NavHistoryEntry, ShellType } from '../shared/types'

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif'])

function detectLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? ''
  if (IMAGE_EXTS.has(ext)) return 'image'
  const languageMap: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescriptreact',
    js: 'javascript',
    jsx: 'javascriptreact',
    vue: 'vue',
    html: 'html',
    css: 'css',
    scss: 'scss',
    less: 'less',
    json: 'json',
    md: 'markdown',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    rs: 'rust',
    py: 'python',
    go: 'go',
    sh: 'shell',
    bash: 'shell',
    zsh: 'shell',
    sql: 'sql',
    graphql: 'graphql',
    xml: 'xml',
    svg: 'xml',
    pine: 'javascript',
  }
  return languageMap[ext] ?? 'plaintext'
}

function extractFileName(filePath: string): string {
  return filePath.split(/[/\\]/).pop() ?? filePath
}

export type Theme = 'dark' | 'light'
// Canonical definition lives in shared/types (canvas windows persist it);
// re-exported here so existing `from 'stores/app'` imports keep working.
export type { ShellType }
export type SidebarInitialWidth = 'default' | 'wide'

export const SHELL_OPTIONS: { value: ShellType; label: string; path: string }[] = [
  { value: 'powershell', label: 'PowerShell', path: 'powershell.exe' },
  { value: 'cmd', label: 'Command Prompt', path: 'cmd.exe' },
  { value: 'bash', label: 'Bash', path: 'bash' },
  { value: 'wsl', label: 'WSL', path: 'wsl.exe' },
  { value: 'git-bash', label: 'Git Bash', path: 'C:\\Program Files\\Git\\bin\\bash.exe' },
]

export const useAppStore = defineStore('canvas/app', () => {
  // ---- State ----
  const fileExplorerVisible = ref(true)
  const editorVisible = ref(true)
  const activeEditorTabs = ref<EditorTab[]>([])
  const activeTabId = ref<string | null>(null)
  const commandPaletteOpen = ref(false)
  const settingsOpen = ref(false)
  const settingsModalOpen = ref(false)
  const theme = ref<Theme>((typeof localStorage !== 'undefined' && localStorage.getItem('qc-theme') as Theme) || 'dark')
  const defaultShell = ref<ShellType>((typeof localStorage !== 'undefined' && localStorage.getItem('qc-shell') as ShellType) || 'powershell')
  const snapToGrid = ref<boolean>(typeof localStorage !== 'undefined' ? localStorage.getItem('qc-snap-grid') !== 'false' : true)
  const snapCameraToGrid = ref<boolean>(typeof localStorage !== 'undefined' ? localStorage.getItem('qc-snap-camera') !== 'false' : true)
  // 13 is the suite's base (shell.css) — canvas matching it by default is what
  // keeps the shared explorer and the fields the same size in every module.
  // A stored 15 is the OLD default (pre-2026-08-27, when this scaled <html>),
  // not a choice — drop it so those installs land on the suite base too.
  if (typeof localStorage !== 'undefined' && localStorage.getItem('qc-font-size') === '15') {
    localStorage.removeItem('qc-font-size')
  }
  const fontSize = ref<number>(typeof localStorage !== 'undefined' ? Number(localStorage.getItem('qc-font-size')) || 13 : 13)
  const canvasHints = ref<boolean>(typeof localStorage !== 'undefined' ? localStorage.getItem('qc-canvas-hints') !== 'false' : true)
  const editorMinimap = ref<boolean>(typeof localStorage !== 'undefined' ? localStorage.getItem('qc-editor-minimap') !== 'false' : true)
  const canvasMinimap = ref<boolean>(typeof localStorage !== 'undefined' ? localStorage.getItem('qc-canvas-minimap') !== 'false' : true)
  const notesBarVisible = ref(false)
  const sidebarInitialWidth = ref<SidebarInitialWidth>((typeof localStorage !== 'undefined' && localStorage.getItem('qc-sidebar-initial-width') as SidebarInitialWidth) || 'default')

  // ---- Navigation history (tracks workspace + tab) ----
  const navHistory = ref<NavHistoryEntry[]>([])
  const navHistoryIndex = ref(-1)
  const _navigating = ref(false)
  // Workspace switch function — registered by WorkspaceSwitcher on mount
  let _switchWorkspaceFn: ((id: string) => Promise<void>) | null = null

  function registerWorkspaceSwitch(fn: (id: string) => Promise<void>): void {
    _switchWorkspaceFn = fn
  }

  // External getter for active workspace ID — avoids circular store imports
  let _activeWorkspaceId: (() => string | null) = () => null

  function registerActiveWorkspaceGetter(fn: () => string | null): void {
    _activeWorkspaceId = fn
  }

  // ---- Getters ----
  const activeTab = computed<EditorTab | undefined>(() => {
    if (!activeTabId.value) return undefined
    return activeEditorTabs.value.find(t => t.id === activeTabId.value)
  })

  const canGoBack = computed(() => navHistoryIndex.value > 0)
  const canGoForward = computed(() => navHistoryIndex.value < navHistory.value.length - 1)

  // ---- Actions ----

  function toggleFileExplorer(): void {
    fileExplorerVisible.value = !fileExplorerVisible.value
  }

  function toggleEditor(): void {
    editorVisible.value = !editorVisible.value
  }

  function setTheme(t: Theme): void {
    theme.value = t
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-theme', t)
    }
    applyTheme(t)
  }

  function toggleTheme(): void {
    setTheme(theme.value === 'dark' ? 'light' : 'dark')
  }

  function toggleSettings(): void {
    settingsOpen.value = !settingsOpen.value
  }

  function openSettingsModal(): void {
    settingsModalOpen.value = true
    settingsOpen.value = false
  }

  function closeSettingsModal(): void {
    settingsModalOpen.value = false
  }

  function setDefaultShell(shell: ShellType): void {
    defaultShell.value = shell
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-shell', shell)
    }
  }

  function toggleSnapToGrid(): void {
    snapToGrid.value = !snapToGrid.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-snap-grid', String(snapToGrid.value))
    }
  }

  function toggleSnapCameraToGrid(): void {
    snapCameraToGrid.value = !snapCameraToGrid.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-snap-camera', String(snapCameraToGrid.value))
    }
  }

  function applyTheme(t: Theme): void {
    if (typeof document !== 'undefined') {
      document.documentElement.setAttribute('data-theme', t)
    }
  }

  function setFontSize(size: number): void {
    fontSize.value = size
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-font-size', String(size))
    }
    applyFontSize(size)
  }

  function toggleCanvasHints(): void {
    canvasHints.value = !canvasHints.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-canvas-hints', String(canvasHints.value))
    }
  }

  function toggleNotesBar(): void {
    notesBarVisible.value = !notesBarVisible.value
  }

  function setSidebarInitialWidth(width: SidebarInitialWidth): void {
    sidebarInitialWidth.value = width
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-sidebar-initial-width', width)
    }
  }

  function toggleEditorMinimap(): void {
    editorMinimap.value = !editorMinimap.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-editor-minimap', String(editorMinimap.value))
    }
  }

  function toggleCanvasMinimap(): void {
    canvasMinimap.value = !canvasMinimap.value
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-canvas-minimap', String(canvasMinimap.value))
    }
  }

  function applyFontSize(size: number): void {
    if (typeof document !== 'undefined') {
      // A CSS variable only `[data-module="canvas"]` consumes — NOT the html
      // font-size. Scaling the document root scaled every rem in the SUITE
      // (the shared explorer's paddings, the dock, other modules once canvas
      // had loaded) — the spacing drift the user flagged on 2026-08-27.
      document.documentElement.style.setProperty('--qc-font-size', size + 'px')
      document.documentElement.style.fontSize = ''
    }
  }

  function pushNavHistory(): void {
    if (_navigating.value) return
    const wsId = _activeWorkspaceId()
    const tabId = activeTabId.value
    const entry: NavHistoryEntry = { workspaceId: wsId, tabId }

    // Trim forward history
    let hist = navHistory.value
    if (navHistoryIndex.value < hist.length - 1) {
      hist = hist.slice(0, navHistoryIndex.value + 1)
    }
    // Avoid duplicating identical consecutive entries
    const last = hist[hist.length - 1]
    if (last && last.workspaceId === entry.workspaceId && last.tabId === entry.tabId) {
      return
    }
    navHistory.value = [...hist, entry]
    navHistoryIndex.value = navHistory.value.length - 1
  }

  async function navigateBack(): Promise<void> {
    if (!canGoBack.value) return
    _navigating.value = true
    try {
      navHistoryIndex.value--
      const entry = navHistory.value[navHistoryIndex.value]
      if (!entry) return
      // Switch workspace if needed
      const currentWs = _activeWorkspaceId()
      if (entry.workspaceId && entry.workspaceId !== currentWs && _switchWorkspaceFn) {
        await _switchWorkspaceFn(entry.workspaceId)
      }
      // Restore tab if it still exists
      if (entry.tabId) {
        const tab = activeEditorTabs.value.find(t => t.id === entry.tabId)
        if (tab) {
          activeTabId.value = entry.tabId
          if (!editorVisible.value) editorVisible.value = true
        }
      }
    } finally {
      _navigating.value = false
    }
  }

  async function navigateForward(): Promise<void> {
    if (!canGoForward.value) return
    _navigating.value = true
    try {
      navHistoryIndex.value++
      const entry = navHistory.value[navHistoryIndex.value]
      if (!entry) return
      // Switch workspace if needed
      const currentWs = _activeWorkspaceId()
      if (entry.workspaceId && entry.workspaceId !== currentWs && _switchWorkspaceFn) {
        await _switchWorkspaceFn(entry.workspaceId)
      }
      // Restore tab if it still exists
      if (entry.tabId) {
        const tab = activeEditorTabs.value.find(t => t.id === entry.tabId)
        if (tab) {
          activeTabId.value = entry.tabId
          if (!editorVisible.value) editorVisible.value = true
        }
      }
    } finally {
      _navigating.value = false
    }
  }

  function openTab(filePath: string, content: string): void {
    const existingTab = activeEditorTabs.value.find(t => t.filePath === filePath)
    if (existingTab) {
      activeTabId.value = existingTab.id
      pushNavHistory()
      return
    }

    const tab: EditorTab = {
      id: uuidv4(),
      filePath,
      fileName: extractFileName(filePath),
      content,
      savedContent: content,
      isDirty: false,
      cursorPosition: { line: 1, column: 1 },
      language: detectLanguage(filePath),
    }

    activeEditorTabs.value.push(tab)
    activeTabId.value = tab.id
    pushNavHistory()
  }

  function closeTab(tabId: string): void {
    const index = activeEditorTabs.value.findIndex(t => t.id === tabId)
    if (index === -1) return

    activeEditorTabs.value.splice(index, 1)

    if (activeTabId.value === tabId) {
      if (activeEditorTabs.value.length > 0) {
        const nextIndex = Math.min(index, activeEditorTabs.value.length - 1)
        activeTabId.value = activeEditorTabs.value[nextIndex].id
      } else {
        activeTabId.value = null
      }
    }
  }

  /**
   * PRE-EXISTING BUG, carried over unchanged in behaviour.
   *
   * `pushTabHistory` is called by `setActiveTab` but was never defined - not
   * here and not in the standalone QuantCode either (verified against the
   * source repo). At runtime it throws a ReferenceError on every editor tab
   * switch, after `activeTabId` has already been set.
   *
   * Defined as a no-op so the module type-checks. Whatever tab-history stack
   * was intended is NOT implemented - inventing it would be a behaviour change
   * the migration has no business making.
   */
  function pushTabHistory(_tabId: string): void {
    // intentionally empty - see above
  }

  function setActiveTab(tabId: string): void {
    const tab = activeEditorTabs.value.find(t => t.id === tabId)
    if (tab) {
      activeTabId.value = tabId
      pushTabHistory(tabId)
    }
  }

  function updateTabContent(tabId: string, content: string): void {
    const tab = activeEditorTabs.value.find(t => t.id === tabId)
    if (!tab) return

    tab.content = content
    tab.isDirty = content !== tab.savedContent
  }

  function markTabClean(tabId: string): void {
    const tab = activeEditorTabs.value.find(t => t.id === tabId)
    if (tab) {
      tab.savedContent = tab.content
      tab.isDirty = false
    }
  }

  // Trigger for file explorer refresh — components watch this
  const fileExplorerRefreshKey = ref(0)

  function triggerFileExplorerRefresh(): void {
    fileExplorerRefreshKey.value++
  }

  // Normalize path for comparison: forward slashes, lowercase drive letter
  function normPath(p: string): string {
    return p.replace(/\\/g, '/').replace(/^([A-Z]):/, (_, d) => d.toLowerCase() + ':')
  }

  function pathsMatch(a: string, b: string): boolean {
    const na = normPath(a)
    const nb = normPath(b)
    return na === nb || na.endsWith('/' + nb) || nb.endsWith('/' + na)
  }

  function refreshTab(filePath: string, content: string): void {
    const tab = activeEditorTabs.value.find(t => pathsMatch(t.filePath, filePath))
    if (tab) {
      tab.content = content
      tab.savedContent = content
      tab.isDirty = false
    }
  }

  // Cross-editor file sync: when any editor saves, others can react
  const lastSavedFile = ref<{ filePath: string; content: string; ts: number } | null>(null)

  function notifyFileSaved(filePath: string, content: string): void {
    lastSavedFile.value = { filePath, content, ts: Date.now() }
    // Also update the sidebar editor tab if it exists
    refreshTab(filePath, content)
  }

  // Notify that a file was deleted — close matching sidebar tabs and canvas windows
  const lastDeletedFile = ref<{ filePath: string; ts: number } | null>(null)

  function notifyFileDeleted(filePath: string): void {
    // Close matching sidebar editor tab
    const tab = activeEditorTabs.value.find(t => pathsMatch(t.filePath, filePath))
    if (tab) {
      closeTab(tab.id)
    }
    // Signal canvas windows to close (FileWindow watches this)
    lastDeletedFile.value = { filePath, ts: Date.now() }
  }

  return {
    // State
    fileExplorerVisible,
    editorVisible,
    activeEditorTabs,
    activeTabId,
    navHistory,
    navHistoryIndex,
    commandPaletteOpen,
    settingsOpen,
    settingsModalOpen,
    theme,
    defaultShell,
    snapToGrid,
    snapCameraToGrid,
    fontSize,
    canvasHints,
    editorMinimap,
    canvasMinimap,
    notesBarVisible,
    sidebarInitialWidth,
    // Getters
    activeTab,
    canGoBack,
    canGoForward,
    // Actions
    navigateBack,
    navigateForward,
    pushNavHistory,
    registerWorkspaceSwitch,
    registerActiveWorkspaceGetter,
    toggleFileExplorer,
    toggleEditor,
    setTheme,
    toggleTheme,
    toggleSettings,
    openSettingsModal,
    closeSettingsModal,
    setDefaultShell,
    toggleSnapToGrid,
    toggleSnapCameraToGrid,
    toggleCanvasHints,
    toggleEditorMinimap,
    toggleCanvasMinimap,
    toggleNotesBar,
    setSidebarInitialWidth,
    setFontSize,
    applyFontSize,
    applyTheme,
    openTab,
    closeTab,
    setActiveTab,
    updateTabContent,
    markTabClean,
    fileExplorerRefreshKey,
    triggerFileExplorerRefresh,
    refreshTab,
    lastSavedFile,
    notifyFileSaved,
    pathsMatch,
    lastDeletedFile,
    notifyFileDeleted,
  }
})
