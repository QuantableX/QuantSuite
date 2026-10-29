/**
 * What the git actions control offers (card 12; T3 Code's
 * GitActionsControl.logic): one primary action for the checkout's state and
 * the full menu with reasons for what is unavailable. A remote-less
 * repository only gets local actions; pull requests need a GitHub remote
 * and a signed-in `gh`.
 */
import type { GitStatus, ThreadLink } from '#pilot/types'

export type GitActionId = 'commit' | 'push' | 'pull' | 'pr' | 'merge' | 'discard' | 'refresh'

export interface GitQuick {
  id: GitActionId | null
  label: string
  /** Shown on hover; for a disabled quick action, why. */
  hint: string
}

export interface GitMenuEntry {
  id: GitActionId
  label: string
  description?: string
  disabled?: boolean
  reason?: string
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`

export function openPr(links: ThreadLink[] | undefined): ThreadLink | null {
  return (links ?? []).filter((l) => l.kind === 'pr' && (l.state === 'open' || l.state === 'draft')).at(-1) ?? null
}

function canPr(st: GitStatus, links?: ThreadLink[]): string | null {
  if (!st.github) return 'Needs a GitHub remote'
  if (st.prBlocked) return st.prBlocked
  if (openPr(links)) return 'This thread already has an open pull request'
  if (!st.branch || st.branch === st.base) return `Work on a branch other than ${st.base || 'the base'}`
  if (st.files.length) return 'Commit or discard the changed files first'
  if (st.aheadOfBase === 0) return `No commits beyond ${st.base}`
  if (st.behind > 0) return 'Pull first — the upstream has new commits'
  return null
}

function canPush(st: GitStatus): string | null {
  if (!st.remote) return 'This repository has no remote'
  if (!st.branch) return 'A detached HEAD has no branch to push'
  if (st.behind > 0) return 'Pull first — the upstream has new commits'
  if (st.upstream && st.ahead === 0) return 'Nothing to push'
  if (!st.upstream && st.aheadOfBase === 0 && st.branch === st.base) return 'Nothing to push'
  return null
}

function canMerge(st: GitStatus): string | null {
  if (!st.worktree) return 'Only a worktree thread merges into its base'
  if (st.files.length) return 'Commit or discard the changed files first'
  if (st.aheadOfBase === 0) return `No commits beyond ${st.base}`
  return null
}

/** The primary button: the next sensible step. */
export function quickAction(st: GitStatus | null, links?: ThreadLink[], busy?: string | null): GitQuick {
  if (busy) return { id: null, label: busy, hint: 'A git action is running.' }
  if (!st) return { id: null, label: 'Commit', hint: 'Git status is unavailable.' }
  if (st.files.length) {
    return { id: 'commit', label: st.remote ? 'Commit & push' : 'Commit', hint: `${plural(st.files.length, 'changed file')} — review and commit` }
  }
  if (st.behind > 0 && st.ahead > 0) return { id: null, label: 'Diverged', hint: 'The branch and its upstream have diverged — merge or rebase first.' }
  if (st.behind > 0) return { id: 'pull', label: 'Pull', hint: `${plural(st.behind, 'new commit')} on ${st.upstream}` }
  if (!canPr(st, links)) return { id: 'pr', label: st.upstream && st.ahead === 0 ? 'Create PR' : 'Push & create PR', hint: `Open a pull request into ${st.base}` }
  if (!canMerge(st)) return { id: 'merge', label: `Merge into ${st.base}`, hint: `${plural(st.aheadOfBase, 'commit')} land in ${st.base} as one merge commit` }
  if (!canPush(st)) return { id: 'push', label: 'Push', hint: st.upstream ? `${plural(st.ahead, 'commit')} to ${st.upstream}` : `Publish ${st.branch} to ${st.remote}` }
  return { id: null, label: 'Up to date', hint: 'Nothing to commit, push or merge.' }
}

/** Every action, with why one is unavailable. */
export function menuEntries(st: GitStatus | null, links?: ThreadLink[]): GitMenuEntry[] {
  if (!st) return [{ id: 'refresh', label: 'Refresh' }]
  const n = st.files.length
  const out: GitMenuEntry[] = [
    {
      id: 'commit',
      label: 'Commit…',
      description: n ? `${plural(n, 'changed file')}` : undefined,
      disabled: n === 0,
      reason: 'Nothing to commit',
    },
  ]
  if (st.remote) {
    const push = canPush(st)
    out.push({ id: 'push', label: st.upstream ? 'Push' : 'Publish branch', description: st.upstream ? `to ${st.upstream}` : `to ${st.remote}`, disabled: !!push, reason: push ?? undefined })
    out.push({ id: 'pull', label: 'Pull', description: st.upstream ? `fast-forward from ${st.upstream}` : undefined, disabled: !st.upstream, reason: 'The branch has no upstream yet' })
  }
  if (st.github) {
    const pr = canPr(st, links)
    out.push({ id: 'pr', label: 'Create pull request…', description: `into ${st.base} on ${st.github}`, disabled: !!pr, reason: pr ?? undefined })
  }
  if (st.worktree) {
    const merge = canMerge(st)
    out.push({ id: 'merge', label: `Merge into ${st.base}…`, description: 'one --no-ff merge commit in the project checkout', disabled: !!merge, reason: merge ?? undefined })
  }
  out.push({ id: 'discard', label: 'Discard all changes…', description: n ? `${plural(n, 'file')} back to the last commit` : undefined, disabled: n === 0, reason: 'Nothing to discard' })
  out.push({ id: 'refresh', label: 'Refresh' })
  return out
}
