/**
 * The board view (card 21): which threads a kanban card has, and the prompt
 * Start thread prefills. A card's threads are links in pilot.db — kind
 * `card`, url `card:<id>` — next to the pull requests.
 */
import type { ThreadLink } from '#pilot/types'

export const GENERAL_BOARD = 'general'

/** The card a link points at, or null for a pull request. */
export function cardIdOf(link: Pick<ThreadLink, 'kind' | 'url'>): string | null {
  return link.kind === 'card' && link.url.startsWith('card:') ? link.url.slice(5) : null
}

/** Card id → the threads started from it, oldest first. */
export function threadsByCard(links: Record<string, ThreadLink[]>): Record<string, string[]> {
  const out: Record<string, { id: string; at: number }[]> = {}
  for (const [threadId, list] of Object.entries(links)) {
    for (const l of list) {
      const card = cardIdOf(l)
      if (card) (out[card] ??= []).push({ id: threadId, at: l.createdAt })
    }
  }
  return Object.fromEntries(Object.entries(out).map(([card, rows]) => [card, rows.sort((a, b) => a.at - b.at).map((r) => r.id)]))
}

export interface PromptCard {
  id: string
  title: string
  description: string
  workspace_id: string
}

/**
 * The first message of a thread started from a card: the card itself, then
 * the workflow — a workspace card is claimed (its own branch and worktree),
 * a General card moves through the columns (AGENT.md §4).
 */
export function cardPrompt(card: PromptCard, boardLabel: string): string {
  const head = `Work on the kanban card "${card.title.trim()}" (card ${card.id}, ${boardLabel} board).`
  const body = card.description.trim()
  const workflow =
    card.workspace_id === GENERAL_BOARD
      ? 'Follow the General card workflow through QuantMCP: read the card with get_kanban_card and the board’s approval mode first, move the card to work with move_kanban_card when you start, and finish it the way the approval mode says.'
      : 'Follow the card workflow through QuantMCP: read the card with get_kanban_card and the board’s approval mode first, then claim it with claim_kanban_card and do all the work in the worktree the claim returns; finish with complete_kanban_card.'
  return [head, body, workflow].filter(Boolean).join('\n\n')
}
