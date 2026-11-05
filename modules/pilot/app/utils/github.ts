/**
 * GitHub in QuantPilot (card 23), free of Vue for node tests: the prompts
 * the GitHub view's actions start threads with, and which thread already
 * holds a pull request or issue. Every prompt names `#123`, so the item's
 * content goes along as GitHub context when it is sent.
 */
import type { GhIssue, GhPull, ThreadLink } from '#pilot/types'

const quote = (s: string) => `"${s.trim()}"`

export type HostId = 'github' | 'gitlab'

/** How each host says it (round 4, card 6): the view does not care which it is. */
export function hostWords(host: HostId = 'github') {
  return host === 'gitlab'
    ? { site: 'GitLab', change: 'merge request', Change: 'Merge request', Changes: 'Merge requests', sign: '!', cli: 'glab' }
    : { site: 'GitHub', change: 'pull request', Change: 'Pull request', Changes: 'Pull requests', sign: '#', cli: 'gh' }
}

export function reviewPrompt(pr: GhPull, host: HostId = 'github'): string {
  if (host === 'gitlab') {
    return [
      `Review merge request !${pr.number} ${quote(pr.title)}.`,
      `Check it out first with \`glab mr checkout ${pr.number}\`, then read its description and its diff against the target (\`glab mr view ${pr.number}\`, \`glab mr diff ${pr.number}\`).`,
      'Report correctness bugs, risky changes and missing tests, the most important first, each with file:line. Do not push or comment on GitLab unless I ask.',
    ].join('\n\n')
  }
  return [
    `Review pull request #${pr.number} ${quote(pr.title)}.`,
    `Check it out first with \`gh pr checkout ${pr.number}\`, then read its description and its diff against the base (\`gh pr diff ${pr.number}\`).`,
    'Report correctness bugs, risky changes and missing tests, the most important first, each with file:line. Do not push or comment on GitHub unless I ask.',
  ].join('\n\n')
}

export function fixChecksPrompt(pr: GhPull, host: HostId = 'github'): string {
  if (host === 'gitlab') {
    return [
      `Merge request !${pr.number} ${quote(pr.title)} has a failing pipeline.`,
      `Check it out with \`glab mr checkout ${pr.number}\`, find out why it fails (\`glab ci status\`, \`glab ci trace\`), fix the cause, run the checks you can locally, commit and push to its branch.`,
    ].join('\n\n')
  }
  const failing = pr.checks.failing.length ? `: ${pr.checks.failing.join(', ')}` : ''
  return [
    `Pull request #${pr.number} ${quote(pr.title)} has failing checks${failing}.`,
    `Check it out with \`gh pr checkout ${pr.number}\`, find out why they fail (\`gh pr checks ${pr.number}\`, \`gh run view <run-id> --log-failed\`), fix the cause and commit. Ask me before pushing.`,
  ].join('\n\n')
}

export function addressReviewPrompt(pr: GhPull, host: HostId = 'github'): string {
  if (host === 'gitlab') {
    return [
      `Address the review comments on merge request !${pr.number} ${quote(pr.title)}.`,
      `Check it out with \`glab mr checkout ${pr.number}\`, read the discussion (\`glab mr view ${pr.number} --comments\`), make the requested changes, commit and push to its branch. Say what you did about each comment.`,
    ].join('\n\n')
  }
  return [
    `Address the review comments on pull request #${pr.number} ${quote(pr.title)}.`,
    `Check it out with \`gh pr checkout ${pr.number}\`, read the reviews and comments (\`gh pr view ${pr.number} --comments\`, and the line comments with \`gh api repos/{owner}/{repo}/pulls/${pr.number}/comments\`), make the requested changes and commit. Ask me before pushing or replying on GitHub.`,
  ].join('\n\n')
}

export function solvePrompt(issue: GhIssue, host: HostId = 'github'): string {
  if (host === 'gitlab') {
    return [
      `Solve issue #${issue.number} ${quote(issue.title)}.`,
      `Read it first with \`glab issue view ${issue.number} --comments\`. Work on this thread's branch, keep the change focused, add or update tests, and commit. When it is done, open a merge request whose description says \`Closes #${issue.number}\`.`,
    ].join('\n\n')
  }
  return [
    `Solve issue #${issue.number} ${quote(issue.title)}.`,
    `Work on this thread's branch, keep the change focused, add or update tests, and commit. When it is done, open a pull request whose description says \`Closes #${issue.number}\` — ask me before pushing.`,
  ].join('\n\n')
}

/** Open in thread: a draft for the user to finish. */
export function draftAbout(kind: 'pr' | 'issue', number: number, title: string, host: HostId = 'github'): string {
  const w = hostWords(host)
  return kind === 'pr' ? `${w.Change} ${w.sign}${number} ${quote(title)}: ` : `Issue #${number} ${quote(title)}: `
}

/** The thread (newest first) that holds this pull request or issue of a repository. */
export function threadFor(links: Record<string, ThreadLink[]>, repoUrl: string, kind: 'pr' | 'issue', number: number, host: HostId = 'github'): string | null {
  const path = host === 'gitlab' ? (kind === 'pr' ? '-/merge_requests' : '-/issues') : kind === 'pr' ? 'pull' : 'issues'
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
