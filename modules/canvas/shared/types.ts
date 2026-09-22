// QuantCode Shared Types

// ============= Workspace Types =============

// A workspace IS a folder, and its type lives in `@quantsuite/core`
// (`Workspace`) — one definition for the whole suite. The module-local
// `WorkspaceInfo`/`WorkspacesConfig` pair and its `folders[]` multi-root field
// were deleted on 2026-08-26 (docs/PLAN-WORKSPACES.md); so was this module's
// own `workspaces.json`. Import `Workspace` from `@quantsuite/core` instead.

// ============= Navigation History =============

export interface NavHistoryEntry {
  workspaceId: string | null
  tabId: string | null
}

// ============= Canvas Types =============

export interface CanvasTransform {
  x: number
  y: number
  scale: number
}

export interface WindowPosition {
  x: number
  y: number
  width: number
  height: number
}

export type WindowType = 'terminal' | 'diff' | 'spec' | 'file' | 'browser'

/** Shell flavour a terminal window runs. Re-exported from `stores/app`. */
export type ShellType = 'powershell' | 'cmd' | 'bash' | 'wsl' | 'git-bash'

export type WindowStatus = 'idle' | 'thinking' | 'live' | 'error' | 'minimized'

export type FilePreviewKind = 'image' | 'code' | 'markdown' | 'text' | 'binary'

export interface FileConfig {
  filePath: string
  previewKind: FilePreviewKind
  language?: string
  content?: string
  base64Data?: string
  mimeType?: string
}

export type SearchEngine = 'google' | 'duckduckgo' | 'bing' | 'brave'

export interface BrowserTab {
  id: string
  url: string
  title: string
  isLoading: boolean
}

export interface BrowserHistoryEntry {
  url: string
  title: string
  visitedAt: string
  favicon?: string
}

export interface BrowserBookmark {
  id: string
  url: string
  title: string
  folder?: string
  createdAt: string
}

export interface BrowserConfig {
  url: string
  tabs?: BrowserTab[]
  activeTabId?: string
  searchEngine?: SearchEngine
}

export interface CanvasWindow {
  id: string
  type: WindowType
  title: string
  position: WindowPosition
  status: WindowStatus
  minimized: boolean
  zIndex: number
  terminalId?: string
  /** Chosen once per terminal window; absent means "still needs to be picked". */
  shellType?: ShellType
  fileConfig?: FileConfig
  browserConfig?: BrowserConfig
}

export interface CanvasState {
  workspaceId: string
  transform: CanvasTransform
  windows: CanvasWindow[]
  nextZIndex: number
}

// ============= Spec Types =============

export type SpecStatus = 'open' | 'verify' | 'done'

export interface SpecFrontmatter {
  title: string
  status: SpecStatus
  priority?: 'low' | 'medium' | 'high' | 'critical'
  assignedTo?: string
  verifyMode?: boolean
  linkedFiles?: string[]
  tags?: string[]
  createdAt: string
  updatedAt: string
}

export interface SpecFile {
  path: string
  frontmatter: SpecFrontmatter
  content: string
  rawContent: string
}

// ============= File Explorer Types =============

export interface FileNode {
  name: string
  path: string
  isDirectory: boolean
  children?: FileNode[]
  expanded?: boolean
  gitStatus?: GitFileStatus
}

export type GitFileStatus = 'modified' | 'untracked' | 'staged' | 'deleted' | 'renamed' | 'clean'

// ============= Git Types =============

export interface GitStatus {
  branch: string
  files: GitStatusFile[]
  ahead: number
  behind: number
}

export interface GitStatusFile {
  path: string
  status: GitFileStatus
  staged: boolean
}

// ============= Diff Types =============

export interface DiffHunk {
  id: string
  filePath: string
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  oldContent: string
  newContent: string
  accepted?: boolean
}

export interface FileDiff {
  filePath: string
  hunks: DiffHunk[]
  isNew: boolean
  isDeleted: boolean
}

// ============= App State Types =============

export interface AppState {
  fileExplorerVisible: boolean
  editorVisible: boolean
  activeEditorTabs: EditorTab[]
  activeTabId: string | null
}

export interface EditorTab {
  id: string
  filePath: string
  fileName: string
  content: string
  savedContent: string
  isDirty: boolean
  cursorPosition: { line: number; column: number }
  language: string
}

// ============= Terminal Types =============

export interface TerminalSession {
  id: string
  windowId: string
  pid?: number
  cwd: string
  active: boolean
}

/** One line the workspace search matched. Mirrors the Rust `SearchMatch`. */
export interface SearchMatch {
  path: string
  lineNumber: number
  /** The whole line, trimmed to 400 chars by the backend. */
  line: string
  /** Byte offsets of the hit inside `line`, for highlighting. */
  matchStart: number
  matchEnd: number
}
