/**
 * The branch toolbar's state (card 10): what checkout each thread works in,
 * switching branches in a Local thread, Local/Worktree before the first
 * message, and the guarded cleanup when a worktree thread settles. Card 12
 * adds the git actions — commit, push, pull, merge into the base, discard,
 * pull requests — and the threads' linked pull requests. Git itself runs
 * in the crate (`git.rs`, `gitops.rs`, QuantMCP's worktree code).
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import type { BranchInfo, CleanupCheck, GitDone, GitGenerated, GitInfo, GitStatus, RepoInfo, SessionMeta, ThreadLink } from '#pilot/types'

export type GitResult = { threadId: string; ok: boolean; text: string; url?: string }

export const useGitStore = defineStore('pilot/git', {
  state: () => ({
    info: {} as Record<string, GitInfo>,
    error: {} as Record<string, string>,
    /** A settled worktree thread waiting for the user's cleanup choice. */
    cleanup: null as { threadId: string; title: string; check: CleanupCheck } | null,
    // ── Git actions (card 12) ──
    status: {} as Record<string, GitStatus>,
    statusError: {} as Record<string, string>,
    /** The running action per thread ("Committing…"). */
    busy: {} as Record<string, string>,
    /** The last action's outcome, shown as a toast. */
    result: null as GitResult | null,
    /** Pull requests per thread. */
    links: {} as Record<string, ThreadLink[]>,
  }),

  actions: {
    async refresh(id: string): Promise<GitInfo | null> {
      try {
        const info = await pilot<GitInfo>('plugin:pilot|pilot_git_info', { id })
        if (info) this.info[id] = info
        delete this.error[id]
        return info
      } catch (e) {
        this.error[id] = String(e)
        return null
      }
    },

    async branches(id: string): Promise<BranchInfo[]> {
      try {
        return (await pilot<BranchInfo[]>('plugin:pilot|pilot_git_branches', { id })) ?? []
      } catch {
        return []
      }
    },

    /** Switch or create a branch in a Local thread's checkout; the error explains a refusal. */
    async checkout(id: string, branch: string, create: boolean, base?: string): Promise<string | null> {
      try {
        await pilot<RepoInfo>('plugin:pilot|pilot_git_checkout', { id, branch, create, base: base ?? null })
        await this.refresh(id)
        return null
      } catch (e) {
        return String(e)
      }
    },

    /** Local / Worktree, base and branch name before the first message. */
    async setWorkspace(id: string, patch: { mode: 'local' | 'worktree'; baseBranch?: string; branch?: string; worktreePath?: string }): Promise<string | null> {
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_thread_set_workspace', {
          id,
          mode: patch.mode,
          baseBranch: patch.baseBranch ?? null,
          branch: patch.branch ?? null,
          worktreePath: patch.worktreePath ?? null,
        })
        if (meta) usePilotStore().upsertSession(meta)
        await this.refresh(id)
        return null
      } catch (e) {
        return String(e)
      }
    },

    /** Ctrl+Shift+L: work in the worktree the newest worktree thread of this project used. */
    async reusePrevious(id: string): Promise<string | null> {
      try {
        const prev = await pilot<SessionMeta | null>('plugin:pilot|pilot_worktree_previous', { id })
        if (!prev?.worktreePath) return 'No earlier worktree in this project.'
        return this.setWorkspace(id, { mode: 'worktree', worktreePath: prev.worktreePath })
      } catch (e) {
        return String(e)
      }
    },

    async recreate(id: string): Promise<string | null> {
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_worktree_recreate', { id })
        if (meta) usePilotStore().upsertSession(meta)
        await this.refresh(id)
        return null
      } catch (e) {
        return String(e)
      }
    },

    /** After a worktree thread settled: ask what to clean up, with safe defaults. */
    async offerCleanup(meta: SessionMeta) {
      if (!meta.worktreePath) return
      try {
        const check = await pilot<CleanupCheck>('plugin:pilot|pilot_worktree_check', { id: meta.id })
        if (check?.exists) this.cleanup = { threadId: meta.id, title: meta.title || 'New thread', check }
      } catch {
        /* no repository any more: nothing to clean */
      }
    },

    // ── Git actions (card 12) ──

    async loadStatus(id: string): Promise<GitStatus | null> {
      try {
        const st = await pilot<GitStatus>('plugin:pilot|pilot_git_status', { id })
        if (st) this.status[id] = st
        delete this.statusError[id]
        return st
      } catch (e) {
        delete this.status[id]
        this.statusError[id] = String(e)
        return null
      }
    },

    /** A commit message (`commit`) or a pull request's title and description (`pr`). */
    async generate(id: string, kind: 'commit' | 'pr', opts: { files?: string[]; base?: string } = {}): Promise<GitGenerated | string> {
      try {
        const g = await pilot<GitGenerated>('plugin:pilot|pilot_git_generate', { id, kind, files: opts.files ?? null, base: opts.base ?? null })
        return g ?? 'Nothing came back.'
      } catch (e) {
        return String(e)
      }
    },

    /** Run one action with a busy label; the outcome lands in `result`, the status refreshes. */
    async run<T>(id: string, label: string, call: () => Promise<T>, describe: (r: T) => GitResult | null): Promise<boolean> {
      if (this.busy[id]) return false
      this.busy[id] = label
      try {
        const r = await call()
        this.result = describe(r)
        return true
      } catch (e) {
        this.result = { threadId: id, ok: false, text: String(e) }
        return false
      } finally {
        delete this.busy[id]
        void this.loadStatus(id)
        void this.refresh(id)
      }
    },

    done(id: string, d: GitDone | null): GitResult {
      return d?.error ? { threadId: id, ok: false, text: `${d.message} ${d.error}` } : { threadId: id, ok: true, text: d?.message ?? 'Done.' }
    },

    commit(id: string, message: string, files: string[] | null, push: boolean) {
      return this.run(id, push ? 'Committing & pushing…' : 'Committing…', () => pilot<GitDone>('plugin:pilot|pilot_git_commit', { id, message, files, push }), (d) => this.done(id, d))
    },
    push(id: string) {
      return this.run(id, 'Pushing…', () => pilot<GitDone>('plugin:pilot|pilot_git_push', { id }), (d) => this.done(id, d))
    },
    pull(id: string) {
      return this.run(id, 'Pulling…', () => pilot<GitDone>('plugin:pilot|pilot_git_pull', { id }), (d) => this.done(id, d))
    },
    merge(id: string) {
      return this.run(id, 'Merging…', () => pilot<GitDone>('plugin:pilot|pilot_git_merge', { id }), (d) => this.done(id, d))
    },
    discardAll(id: string) {
      return this.run(id, 'Discarding…', () => pilot('plugin:pilot|pilot_git_discard_all', { id }), () => ({ threadId: id, ok: true, text: 'All changes discarded.' }))
    },
    createPr(id: string, req: { base: string; title: string; body: string; draft: boolean }) {
      return this.run(
        id,
        'Creating pull request…',
        () => pilot<ThreadLink>('plugin:pilot|pilot_pr_create', { id, ...req }),
        (l) => {
          if (l) this.links[id] = [...(this.links[id] ?? []).filter((x) => x.url !== l.url), l]
          return { threadId: id, ok: true, text: l ? `Pull request #${l.number} created.` : 'Pull request created.', url: l?.url }
        }
      )
    },

    async loadLinks() {
      try {
        const all = (await pilot<ThreadLink[]>('plugin:pilot|pilot_links', { id: null })) ?? []
        const by: Record<string, ThreadLink[]> = {}
        for (const l of all) (by[l.threadId] ??= []).push(l)
        this.links = by
      } catch {
        /* the links stay as they were */
      }
    },

    /** A thread's pull requests from GitHub: open ones at most every five minutes (the crate decides), all when forced. */
    async refreshPrs(id: string, force = false) {
      if (!force && !(this.links[id] ?? []).some((l) => l.state === 'open' || l.state === 'draft')) return
      try {
        const links = await pilot<ThreadLink[]>('plugin:pilot|pilot_pr_refresh', { id, force })
        if (links) this.links[id] = links
      } catch {
        /* offline or signed out: the chip keeps its last state */
      }
    },

    async removeWorktree(id: string, deleteBranch: boolean, unmergedOk: boolean): Promise<string | null> {
      try {
        await pilot('plugin:pilot|pilot_worktree_remove', { id, deleteBranch, unmergedOk })
        await this.refresh(id)
        return null
      } catch (e) {
        return String(e)
      }
    },
  },
})
