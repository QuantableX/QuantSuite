/**
 * The branch toolbar's state (card 10): what checkout each thread works in,
 * switching branches in a Local thread, Local/Worktree before the first
 * message, and the guarded cleanup when a worktree thread settles. Git
 * itself runs in the crate (`git.rs`, QuantMCP's worktree code).
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import type { BranchInfo, CleanupCheck, GitInfo, RepoInfo, SessionMeta } from '#pilot/types'

export const useGitStore = defineStore('pilot/git', {
  state: () => ({
    info: {} as Record<string, GitInfo>,
    error: {} as Record<string, string>,
    /** A settled worktree thread waiting for the user's cleanup choice. */
    cleanup: null as { threadId: string; title: string; check: CleanupCheck } | null,
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
