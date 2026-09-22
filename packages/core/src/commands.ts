/**
 * Typed wrappers around the Tauri commands. No component writes raw `invoke`
 * (ARCHITECTURE.md §2) - module namespaces are added here as their phases land.
 *
 * The plugin is named `qs`, not `core`: Tauri reserves `core` for its own
 * built-in commands and refuses to register a plugin under that name.
 */

import { invoke } from '@tauri-apps/api/core'
import type {
  AgentCapability,
  AgentDecision,
  ConsoleAttachResult,
  ConsoleSession,
  ConsoleShell,
  Entity,
  FileEntry,
  GitCommitEntry,
  GitCommitFile,
  GitStatusResult,
  TimelineEntry,
  Link,
  PendingAgentCall,
  ProcessInfo,
  SuitePaths,
} from './types'

export const qs = {
  core: {
    // -- settings --
    getSettings: (scope: string) => invoke<Record<string, unknown>>('plugin:qs|get_settings', { scope }),
    getSetting: <T = unknown>(scope: string, key: string) =>
      invoke<T | null>('plugin:qs|get_setting', { scope, key }),
    setSetting: (scope: string, key: string, value: unknown) =>
      invoke<void>('plugin:qs|set_setting', { scope, key, value }),

    // -- entities --
    upsertEntity: (entity: Entity) => invoke<void>('plugin:qs|upsert_entity', { entity }),
    deleteEntity: (id: string) => invoke<void>('plugin:qs|delete_entity', { id }),
    listEntities: (opts: { module?: string; kind?: string; limit?: number } = {}) =>
      invoke<Entity[]>('plugin:qs|list_entities', opts),
    /** Same filters as `listEntities`, without loading the rows — for counters. */
    countEntities: (opts: { module?: string; kind?: string } = {}) =>
      invoke<number>('plugin:qs|count_entities', opts),
    searchEntities: (query: string, limit = 30) =>
      invoke<Entity[]>('plugin:qs|search_entities', { query, limit }),
    linkEntities: (link: Link) => invoke<void>('plugin:qs|link_entities', { link }),
    unlinkEntities: (link: Link) => invoke<void>('plugin:qs|unlink_entities', { link }),
    linkedEntities: (id: string, incoming = false) =>
      invoke<Entity[]>('plugin:qs|linked_entities', { id, incoming }),

    // -- processes --
    /**
     * Every process a module has announced, running or not (process center, E4).
     *
     * The only core command here on purpose: qs-core keeps a register, not a
     * supervisor. Start, stop and the log tail belong to the owning module, and
     * each entry names the `plugin:<module>|<command>` to invoke for them.
     */
    processList: () => invoke<ProcessInfo[]>('plugin:qs|process_list'),

    // -- agent layer (the MCP bridge's policy, inspectable) --
    agentTools: () => invoke<AgentCapability[]>('plugin:qs|agent_tools'),
    agentToolDecision: (tool: string, mode?: 'strict' | 'relaxed') =>
      invoke<AgentDecision>('plugin:qs|agent_tool_decision', { tool, mode }),
    /** Calls waiting on the shell — queue recovery after a reload (E4). */
    agentPendingCalls: () => invoke<PendingAgentCall[]>('plugin:qs|agent_pending_calls'),
    /** Atomically reserve a call before dispatch/rejection from any window. */
    agentCallClaim: (callId: string) => invoke<boolean>('plugin:qs|agent_call_claim', { callId }),
    /** Report a dispatched call's outcome back to the waiting agent. */
    agentCallComplete: (callId: string, result: string | null, error: string | null) =>
      invoke<void>('plugin:qs|agent_call_complete', { callId, result, error }),

    // -- window / lifecycle --
    /**
     * Clip the window to a circle of `diameter` logical px, or `null` for the
     * rectangle. CSS `border-radius` rounds only what is painted — the corners
     * stay part of the window and keep swallowing clicks.
     */
    setCircularWindow: (diameter: number | null) =>
      invoke<void>('plugin:qs|set_circular_window', { diameter }),
    windowShow: () => invoke<void>('plugin:qs|window_show'),
    /** Another independent view of this same running suite and its data. */
    windowNew: () => invoke<string>('plugin:qs|window_new'),
    windowHide: () => invoke<void>('plugin:qs|window_hide'),
    windowToggle: () => invoke<void>('plugin:qs|window_toggle'),
    /** The only real exit - runs the ordered teardown first. */
    quit: () => invoke<void>('plugin:qs|quit'),

    // -- autostart --
    /**
     * Whether the OS starts the suite with the session. Read from the OS entry
     * itself (the Run key on Windows), so a removal made outside the app shows
     * up here rather than being masked by a stale setting.
     */
    autostartEnabled: () => invoke<boolean>('plugin:qs|autostart_enabled'),
    /** An autostart launch stays in the tray with the window hidden. */
    setAutostart: (enabled: boolean) => invoke<void>('plugin:qs|set_autostart', { enabled }),

    // -- introspection --
    appVersion: () => invoke<string>('plugin:qs|app_version'),
    paths: () => invoke<SuitePaths>('plugin:qs|suite_paths'),
  },

  /**
   * QuantConsole — terminal sessions (docs/PLAN-CONSOLE.md, phase P0).
   *
   * Output does not come back through these calls: the plugin emits
   * `console-output` (`{ id, data }`, empty `data` = EOF) to the *one* webview
   * that opened the session, and `console-session` for state changes. Listen
   * with `useTauriEvent` and filter by id.
   */
  console: {
    /**
     * Open a session. `owner` says which surface it belongs to — `'console'` for
     * a tab, `'canvas'` for a canvas window — and is what keeps the console's tab
     * bar from adopting shells opened somewhere else (P4.5).
     */
    openSession: (opts: {
      cwd: string
      shell?: string | null
      cols?: number
      rows?: number
      owner?: 'console' | 'canvas'
    }) =>
      invoke<string>('plugin:console|open_session', {
        request: {
          cwd: opts.cwd,
          shell: opts.shell ?? null,
          cols: opts.cols ?? null,
          rows: opts.rows ?? null,
          owner: opts.owner ?? 'console',
        },
      }),
    write: (id: string, data: string) => invoke<void>('plugin:console|write_session', { id, data }),
    resize: (id: string, cols: number, rows: number) =>
      invoke<void>('plugin:console|resize_session', { id, cols, rows }),
    close: (id: string) => invoke<void>('plugin:console|close_session', { id }),
    /** Leaving the module: keep the shell, buffer its output. */
    detach: (id: string) => invoke<void>('plugin:console|detach_session', { id }),
    /**
     * Coming back: resume streaming. What the shell printed while nobody was
     * listening comes back verbatim in `raw` — xterm replays it and the screen
     * is whole again.
     */
    attach: (id: string) => invoke<ConsoleAttachResult>('plugin:console|attach_session', { id }),
    alive: (id: string) => invoke<boolean>('plugin:console|session_alive', { id }),
    /** Live sessions; pass an owner to see only one surface's. */
    listSessions: (owner?: 'console' | 'canvas') =>
      invoke<ConsoleSession[]>('plugin:console|list_sessions', { owner: owner ?? null }),
    /** Only shells that exist on this machine, default first-marked. */
    listShells: () => invoke<ConsoleShell[]>('plugin:console|list_shells'),

    /**
     * Write a pasted image to disk and get back the path to type.
     *
     * A PTY carries no attachments — what a program like `claude` takes is a
     * path, so a pasted screenshot has to become a file first. `data` is the
     * image base64-encoded without a data-URL prefix; `extension` comes from the
     * clipboard's MIME type and is sanitised on the other side.
     */
    savePastedImage: (data: string, extension: string) =>
      invoke<string>('plugin:console|save_pasted_image', { data, extension }),
  },

  /**
   * Files and git, for any module with a tools panel.
   *
   * These are `plugin:canvas|…` and that is deliberate, not a leak. They are
   * suite-wide primitives — a gitignore-aware directory walk, a read, a write, a
   * `git2` status and diff — that happen to have been written in the canvas
   * crate first. The alternative was a second copy of a walker and a diff engine
   * in the console crate, which is the kind of duplication that drifts: one gets
   * a bug fix, the other does not, and the two file trees start disagreeing about
   * what `.gitignore` means.
   *
   * The dependency is on the *command*, which is a stable named surface, and it
   * is declared here in one place instead of spread across components. If these
   * ever move to `qs-core`, this namespace is the only thing that changes.
   */
  files: {
    /**
     * The directory as a tree, honouring `.gitignore` when asked.
     *
     * `gitignore: true` is what keeps `node_modules` and `target` out — without
     * it a walk of a real project returns a hundred thousand entries and the
     * panel that renders them stops responding.
     */
    readDirTree: (path: string, gitignore = true) =>
      invoke<FileEntry[]>('plugin:canvas|read_dir_tree', { path, gitignore }),
    readFile: (path: string) => invoke<string>('plugin:canvas|read_file', { path }),
    writeFile: (path: string, content: string) =>
      invoke<void>('plugin:canvas|write_file', { path, content }),

    /** Branch, ahead/behind, and one entry per changed file. */
    gitStatus: (repoPath: string) =>
      invoke<GitStatusResult>('plugin:canvas|git_status', { repoPath }),
    /**
     * The repository's diff as unified text.
     *
     * `mode` is `'all'`, `'staged'` or `'unstaged'`. Whole-repository, not
     * per-file: that is what the command offers, and splitting the result by its
     * `diff --git` headers is cheaper than one call per changed file.
     */
    gitDiff: (repoPath: string, mode: 'all' | 'staged' | 'unstaged' = 'all') =>
      invoke<string>('plugin:canvas|git_diff', { repoPath, mode }),

    /** The last `limit` commits reachable from HEAD, newest first. */
    gitLog: (repoPath: string, limit = 30) =>
      invoke<GitCommitEntry[]>('plugin:canvas|git_log', { repoPath, limit }),
    /** The files one commit touched — its tree against the first parent's. */
    gitCommitFiles: (repoPath: string, oid: string) =>
      invoke<GitCommitFile[]>('plugin:canvas|git_commit_files', { repoPath, oid }),

    /**
     * Local save history (QuantCode's Timeline). A snapshot per save, deduped
     * against the latest and capped per file — git answers "since the last
     * commit", this answers "since twenty minutes ago".
     */
    timelineSnapshot: (path: string, content: string) =>
      invoke<boolean>('plugin:canvas|timeline_snapshot', { path, content }),
    timelineList: (path: string) =>
      invoke<TimelineEntry[]>('plugin:canvas|timeline_list', { path }),
    timelineRead: (path: string, id: string) =>
      invoke<string>('plugin:canvas|timeline_read', { path, id }),
  },

  // Further module namespaces are added here as each phase lands, e.g.:
  // systems: {
  //   listSystems: () => invoke<SystemMeta[]>('plugin:systems|list_systems'),
  // },
}
