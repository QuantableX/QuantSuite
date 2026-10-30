/**
 * GitHub in QuantPilot (card 23), free of Vue for node tests: the prompts
 * the GitHub view's actions start threads with, and which thread already
 * holds a pull request or issue. Every prompt names `#123`, so the item's
 * content goes along as GitHub context when it is sent.
 */
import type { GhIssue, GhPull, ThreadLink } from '#pilot/types'

const quote = (s: string) => `"${s.trim()}"`

export function reviewPrompt(pr: GhPull): string {
  return [
    `Review pull request #${pr.number} ${quote(pr.title)}.`,
    `Check it out first with \`gh pr checkout ${pr.number}\`, then read its description and its diff against the base (\`gh pr diff ${pr.number}\`).`,
    'Report correctness bugs, risky changes and missing tests, the most important first, each with file:line. Do not push or comment on GitHub unless I ask.',
  ].join('\n\n')
}

export function fixChecksPrompt(pr: GhPull): string {
  const failing = pr.checks.failing.length ? `: ${pr.checks.failing.join(', ')}` : ''
  return [
    `Pull request #${pr.number} ${quote(pr.title)} has failing checks${failing}.`,
    `Check it out with \`gh pr checkout ${pr.number}\`, find out why they fail (\`gh pr checks ${pr.number}\`, \`gh run view <run-id> --log-failed\`), fix the cause and commit. Ask me before pushing.`,
  ].join('\n\n')
}

export function addressReviewPrompt(pr: GhPull): string {
  return [
    `Address the review comments on pull request #${pr.number} ${quote(pr.title)}.`,
    `Check it out with \`gh pr checkout ${pr.number}\`, read the reviews and comments (\`gh pr view ${pr.number} --comments\`, and the line comments with \`gh api repos/{owner}/{repo}/pulls/${pr.number}/comments\`), make the requested changes and commit. Ask me before pushing or replying on GitHub.`,
  ].join('\n\n')
}

export function solvePrompt(issue: GhIssue): string {
  return [
    `Solve issue #${issue.number} ${quote(issue.title)}.`,
    `Work on this thread's branch, keep the change focused, add or update tests, and commit. When it is done, open a pull request whose description says \`Closes #${issue.number}\` — ask me before pushing.`,
  ].join('\n\n')
}

/** Open in thread: a draft for the user to finish. */
export function draftAbout(kind: 'pr' | 'issue', number: number, title: string): string {
  return `${kind === 'pr' ? 'Pull request' : 'Issue'} #${number} ${quote(title)}: `
}

/** The thread (newest first) that holds this pull request or issue of a repository. */
export function threadFor(links: Record<string, ThreadLink[]>, repoUrl: string, kind: 'pr' | 'issue', number: number): string | null {
  const path = kind === 'pr' ? 'pull' : 'issues'
  const url = `${repoUrl}/${path}/${number}`
  let best: { id: string; at: number } | null = null
  for (const [threadId, list] of Object.entries(links)) {
    for (const l of list) {
      if (l.kind === kind && l.url === url && (!best || l.createdAt > best.at)) best = { id: threadId, at: l.createdAt }
    }
  }
  return best?.id ?? null
}

/** "+120 −30" for a pull request's size. */
export function sizeOf(pr: Pick<GhPull, 'additions' | 'deletions'>): string {
  return `+${pr.additions} −${pr.deletions}`
}

export const REVIEW_LABEL: Record<string, string> = {
  APPROVED: 'Approved',
  CHANGES_REQUESTED: 'Changes requested',
  REVIEW_REQUIRED: 'Review required',
}
