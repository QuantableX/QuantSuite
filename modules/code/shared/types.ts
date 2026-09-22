// QuantCode shared types (docs/PLAN-QUANTSPACE.md)

/** How a tab renders. Images get a viewer, everything else gets the editor. */
export type TabKind = 'text' | 'image'

/** One open file in one editor group. */
export interface CodeTab {
  id: string
  /** Absolute path on disk. Also the Monaco model's identity. */
  path: string
  fileName: string
  /** The buffer. The store is the source of truth; Monaco echoes it. */
  content: string
  /** What was last written to disk — what `dirty` is measured against. */
  savedContent: string
  language: string
  kind: TabKind
  /** Pinned tabs survive "close others" and open in place of a preview. */
  pinned: boolean
  /**
   * A viewer, not an editor — Timeline snapshots open this way. Read-only
   * tabs are also transient: they are skipped when the session is persisted,
   * because their pseudo-path has nothing behind it to reopen.
   */
  readOnly?: boolean
  /**
   * Markdown only: show the rendered document instead of the source. Per TAB,
   * not per path, so the same file opened in both splits can be source on one
   * side and preview on the other — the reason to have a split at all for a
   * document. Deliberately not persisted: the session stores what was open,
   * and a fresh window starts every `.md` the way `openFile` does.
   */
  preview?: boolean
}

/**
 * One editor group — a column of the split. Every group has its own tab strip
 * and its own active tab; the same file may be open in several groups, where
 * Monaco shares one model between them.
 */
export interface EditorGroup {
  id: string
  tabs: CodeTab[]
  activeTabId: string | null
}

/** Which dock panel is showing, or none. */
export type PanelTab = 'problems' | 'terminal' | 'search'

/** One hit from the project-wide search. */
export interface SearchHit {
  path: string
  line: number
  /** The matching line, trimmed for display. */
  text: string
}

/** A pinned line. `line` tracks the code while the file is open (see stores/bookmarks). */
export interface BookmarkEntry {
  id: string
  /** Absolute path on disk. */
  path: string
  /** 1-based, kept current by the Monaco decoration backing it. */
  line: number
  /** The line's text at the last sync — the panel's preview. */
  text: string
}

/** What gets persisted per workspace, so a return lands where you left. */
export interface CodeSession {
  groups: {
    tabs: { path: string; pinned: boolean }[]
    activeTabPath: string | null
  }[]
  activeGroupIndex: number
}
