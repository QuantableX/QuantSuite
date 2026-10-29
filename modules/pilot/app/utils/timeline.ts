/**
 * How a thread's items read as a conversation (docs/PLAN-QUANTPILOT.md,
 * card 3; T3 Code's activity log, PandaCode's "Worked for …").
 *
 * Per turn: the user's message, then everything the agent did folded into
 * one "Worked for …" disclosure — commentary, reasoning, tool calls (runs
 * of up to five tools form one group with a summary), resolved requests —
 * and outside it the latest answer (the final reply, or while working the
 * newest narration that no tool followed yet), pending requests, plans,
 * errors and notices. Pure functions: the components only render this.
 */
import type { ThreadItem } from '../types'

export const GROUP_MAX = 5

export interface ToolGroup {
  kind: 'tools'
  id: string
  items: ThreadItem[]
  summary: string
  running: number
  failed: number
}

export type ActivityEntry = { kind: 'item'; id: string; item: ThreadItem } | ToolGroup

export interface TurnBlock {
  turnId: string
  user: ThreadItem | null
  activity: ActivityEntry[]
  /** Tool calls in the fold. */
  toolCount: number
  /** Short categories for the fold's header ("Ran 3 commands, edited 2 files"). */
  summary: string
  /** Plans, errors and notices that came before the answer. */
  before: ThreadItem[]
  answer: ThreadItem | null
  /** Pending approvals, questions and plans — they need the user. */
  requests: ThreadItem[]
  /** Plans, errors and notices after the answer (all of them when there is none). */
  outside: ThreadItem[]
  startedAt: number
  updatedAt: number
}

const WORK_KINDS = new Set(['tool', 'reasoning', 'todo'])

function isPending(it: ThreadItem): boolean {
  return (it.kind === 'approval' || it.kind === 'question' || it.kind === 'plan') && it.data.status === 'pending'
}

const CATEGORY_ORDER = ['command', 'edit', 'agent', 'mcp', 'read', 'search', 'web', 'todo', 'tool'] as const

function phrase(category: string, n: number): string {
  const s = n === 1 ? '' : 's'
  switch (category) {
    case 'command':
      return `ran ${n} command${s}`
    case 'edit':
      return `edited ${n} file${s}`
    case 'agent':
      return `ran ${n} agent${s}`
    case 'mcp':
      return `called ${n} MCP tool${s}`
    case 'read':
      return `read ${n} file${s}`
    case 'search':
      return n === 1 ? 'searched once' : `searched ${n} times`
    case 'web':
      return `fetched ${n} page${s}`
    case 'todo':
      return 'updated the task list'
    default:
      return `used ${n} tool${s}`
  }
}

/** "Ran 3 commands, edited 2 files and 4 more" — at most two categories named. */
export function summarize(tools: ThreadItem[]): string {
  const counts = new Map<string, number>()
  for (const t of tools) {
    const c = String(t.data.category ?? 'tool')
    counts.set(c, (counts.get(c) ?? 0) + 1)
  }
  const ordered = CATEGORY_ORDER.filter((c) => counts.has(c)).map((c) => [c, counts.get(c)!] as const)
  for (const [c, n] of counts) if (!CATEGORY_ORDER.includes(c as (typeof CATEGORY_ORDER)[number])) ordered.push([c as 'tool', n])
  if (!ordered.length) return ''
  const named = ordered.slice(0, 2).map(([c, n]) => phrase(c, n))
  const rest = ordered.slice(2).reduce((sum, [, n]) => sum + n, 0)
  let text = named.join(', ')
  if (rest > 0) text += ` and ${rest} more`
  return text.charAt(0).toUpperCase() + text.slice(1)
}

function activityOf(items: ThreadItem[]): ActivityEntry[] {
  const out: ActivityEntry[] = []
  let run: ThreadItem[] = []
  const flush = () => {
    if (!run.length) return
    for (let i = 0; i < run.length; i += GROUP_MAX) {
      const chunk = run.slice(i, i + GROUP_MAX)
      out.push({
        kind: 'tools',
        id: `g:${chunk[0].id}`,
        items: chunk,
        summary: summarize(chunk),
        running: chunk.filter((t) => t.data.status === 'running').length,
        failed: chunk.filter((t) => t.data.status === 'failed').length,
      })
    }
    run = []
  }
  for (const it of items) {
    if (it.kind === 'tool') {
      run.push(it)
      continue
    }
    flush()
    out.push({ kind: 'item', id: it.id, item: it })
  }
  flush()
  return out
}

export function groupTurns(items: ThreadItem[]): TurnBlock[] {
  const order: string[] = []
  const byTurn = new Map<string, ThreadItem[]>()
  for (const it of items) {
    let list = byTurn.get(it.turnId)
    if (!list) {
      list = []
      byTurn.set(it.turnId, list)
      order.push(it.turnId)
    }
    list.push(it)
  }
  return order.map((turnId) => {
    const all = byTurn.get(turnId)!
    const user = all.find((i) => i.kind === 'user') ?? null
    const rest = all.filter((i) => i !== user)
    let lastWork = -1
    rest.forEach((it, i) => {
      if (WORK_KINDS.has(it.kind) || it.parent || ((it.kind === 'approval' || it.kind === 'question') && !isPending(it))) {
        lastWork = i
      }
    })
    let answerIndex = -1
    for (let i = rest.length - 1; i > lastWork; i--) {
      const it = rest[i]
      if (it.kind === 'assistant' && !it.parent) {
        answerIndex = i
        break
      }
    }
    const answer = answerIndex >= 0 ? rest[answerIndex] : null
    const requests: ThreadItem[] = []
    const before: ThreadItem[] = []
    const outside: ThreadItem[] = []
    const folded: ThreadItem[] = []
    rest.forEach((it, i) => {
      if (i === answerIndex) return
      if (isPending(it)) requests.push(it)
      else if (it.kind === 'plan' || it.kind === 'error' || it.kind === 'notice') (answer && i < answerIndex ? before : outside).push(it)
      else if (it.kind === 'assistant' && !it.parent && !String(it.data.text ?? '').trim()) return
      else folded.push(it)
    })
    const tools = folded.filter((i) => i.kind === 'tool')
    return {
      turnId,
      user,
      activity: activityOf(folded),
      toolCount: tools.length,
      summary: summarize(tools),
      before,
      answer,
      requests,
      outside,
      startedAt: all[0].createdAt,
      updatedAt: all.reduce((m, i) => Math.max(m, i.updatedAt), 0),
    }
  })
}

/** 12s · 1m 12s · 1h 3m */
export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000))
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${s % 60}s`
  return `${Math.floor(m / 60)}h ${m % 60}m`
}
