/**
 * The live orchestration graph (round 2, card 08): the orchestrator in the
 * middle, its workers around it, a particle along an edge for every real
 * message, the message bus underneath. What to draw is computed here —
 * nodes, the radial layout, which edge a message travels, where a bus line
 * jumps to — pure, so the node tests import it.
 */
import type { CrewMessage, CrewSnapshot, CrewTask } from '#pilot/types'

/** Node states, in the colours of the rest of QuantPilot. */
export type NodeState = 'working' | 'waiting' | 'done' | 'error' | 'stopped' | 'idle'

export interface GraphNode {
  /** Stable across attempts: the task id (a crew) or the subagent's call id. */
  id: string
  /** The thread a click opens; none for a subagent (its parent thread). */
  threadId: string | null
  /** The worker's name: the task key, or the subagent's title. */
  name: string
  /** What it works on, one line. */
  task: string
  provider: string
  state: NodeState
  /** 0–100, or null when the worker does not report progress. */
  progress: number | null
  startedAt: number | null
  finishedAt: number | null
  /** Its latest step: the hover. */
  latest: string
  color: string
  attempts: number
}

export interface GraphCenter {
  threadId: string
  title: string
  provider: string
  /** `orchestrator · 6 workers · leading`. */
  sub: string
  state: NodeState
}

export interface Graph {
  kind: 'crew' | 'subagents'
  center: GraphCenter
  nodes: GraphNode[]
}

/** Worker colours: distinct on the dark theme; the orchestrator keeps the accent. */
export const WORKER_COLORS = ['#34d399', '#f59e0b', '#f472b6', '#a78bfa', '#22d3ee', '#fb7185', '#a3e635', '#fbbf24', '#60a5fa', '#e879f9', '#2dd4bf', '#fca5a5']
export const CENTER_COLOR = '#7aa2f7'

export function colorOf(index: number): string {
  return WORKER_COLORS[((index % WORKER_COLORS.length) + WORKER_COLORS.length) % WORKER_COLORS.length]
}

export function nodeState(status: CrewTask['status']): NodeState {
  switch (status) {
    case 'running':
      return 'working'
    case 'asking':
    case 'stalled':
      return 'waiting'
    case 'done':
      return 'done'
    case 'failed':
      return 'error'
    case 'stopped':
      return 'stopped'
    default:
      return 'idle'
  }
}

export const STATE_LABEL: Record<NodeState, string> = {
  working: 'working',
  waiting: 'waiting',
  done: 'done',
  error: 'error',
  stopped: 'stopped',
  idle: 'queued',
}

function firstLine(s: string, max = 90): string {
  const line = s.trim().split('\n')[0] ?? ''
  return line.length > max ? `${line.slice(0, max - 1)}…` : line
}

/** A crew as a graph. `latest` per task: its newest progress note or message. `providerOf` names a thread's agent (the orchestrator's). */
export function crewGraph(snap: CrewSnapshot, providerOf: (threadId: string) => string | undefined = () => undefined): Graph {
  const working = snap.tasks.filter((t) => t.status === 'running' || t.status === 'asking' || t.status === 'stalled').length
  const allSettled = snap.tasks.length > 0 && snap.tasks.every((t) => ['done', 'failed', 'stopped'].includes(t.status))
  const leading = snap.crew.status === 'stopped' ? 'stopped' : allSettled ? 'finished' : 'leading'
  const lastByTask = new Map<string, CrewMessage>()
  for (const m of snap.messages) if (m.taskId && m.kind !== 'plan') lastByTask.set(m.taskId, m)
  const nodes = snap.tasks.map((t, i) => {
    const last = lastByTask.get(t.id)
    const latest = t.note || (last ? firstLine(last.text, 160) : '') || (t.status === 'waiting' ? 'queued' : '')
    return {
      id: t.id,
      threadId: t.workerId,
      name: t.key,
      task: firstLine(t.spec),
      provider: t.provider,
      state: nodeState(t.status),
      progress: t.status === 'done' ? 100 : t.status === 'waiting' ? null : Math.max(0, Math.min(100, t.progress)),
      startedAt: t.startedAt,
      finishedAt: t.finishedAt,
      latest: t.status === 'done' || t.status === 'failed' ? firstLine((t.status === 'failed' && t.error) || t.summary || latest, 160) : latest,
      color: colorOf(i),
      attempts: t.attempts,
    } satisfies GraphNode
  })
  const n = snap.tasks.length
  return {
    kind: 'crew',
    center: {
      threadId: snap.crew.orchestratorId,
      title: snap.titles[snap.crew.orchestratorId] || 'Orchestrator',
      provider: providerOf(snap.crew.orchestratorId) || snap.members.find((m) => m.role === 'orchestrator')?.provider || '',
      sub: `orchestrator · ${n} worker${n === 1 ? '' : 's'} · ${leading}`,
      state: working > 0 ? 'working' : allSettled ? 'done' : 'idle',
    },
    nodes,
  }
}

/** A thread's native subagents (Claude's Task, Codex's agents) as a graph. */
export interface SubagentLike {
  callId: string
  title: string
  type: string
  status: string
  tools: number
  last: string
  item: { createdAt?: number; updatedAt?: number }
}

export function subagentGraph(thread: { id: string; title: string; provider: string; state?: string }, subs: SubagentLike[]): Graph {
  const running = subs.filter((s) => s.status === 'running').length
  const ordered = [...subs].reverse()
  return {
    kind: 'subagents',
    center: {
      threadId: thread.id,
      title: thread.title || 'This thread',
      provider: thread.provider,
      sub: `${subs.length} subagent${subs.length === 1 ? '' : 's'}${running ? ` · ${running} working` : ''}`,
      state: running ? 'working' : 'idle',
    },
    nodes: ordered.map((s, i) => ({
      id: s.callId,
      threadId: null,
      name: firstLine(s.title, 28),
      task: s.type ? `${s.type} · ${s.tools} tool${s.tools === 1 ? '' : 's'}` : `${s.tools} tool${s.tools === 1 ? '' : 's'}`,
      provider: thread.provider,
      state: s.status === 'running' ? 'working' : s.status === 'failed' ? 'error' : s.status ? 'done' : 'idle',
      progress: null,
      startedAt: s.item.createdAt ?? null,
      finishedAt: s.status === 'running' ? null : (s.item.updatedAt ?? null),
      latest: s.last,
      color: colorOf(i),
      attempts: 1,
    })),
  }
}

// ── Layout ──

export interface Box {
  x: number
  y: number
}

export interface Layout {
  center: Box
  nodes: Box[]
  /** The world the nodes need; the stage scales it down to fit when larger. */
  width: number
  height: number
}

export const PER_RING = 12

/**
 * Up to 12 workers on one ellipse around the center (from the top,
 * clockwise); more go on further rings. Each ring is large enough that
 * neighbours do not touch (side by side at the top, one above the other at
 * the sides) and clears the center or the ring inside; then the rings
 * spread out to fill a larger stage. Positions are node centers.
 */
export function radialLayout(count: number, width: number, height: number, nodeW: number, nodeH: number, pad = 16, centerW = nodeW * 1.3, centerH = nodeH): Layout {
  const gapX = Math.max(8, nodeW * 0.1)
  const gapY = Math.max(6, nodeH * 0.18)
  const rings = Math.max(1, Math.ceil(count / PER_RING))
  const radii: { rx: number; ry: number; m: number }[] = []
  for (let r = 0; r < rings; r++) {
    const m = Math.max(1, Math.min(PER_RING, count - r * PER_RING))
    // The chord between neighbours is 2R·sin(step/2); between the top and a
    // side they are offset diagonally, so a ring of more than four needs √2 more.
    const chord = (m > 1 ? 2 * Math.sin(Math.PI / m) : 2) / (m > 4 ? Math.SQRT2 : 1)
    // Clear the ring inside (or the center) at every angle: √2 again for the diagonals.
    const prev = radii[r - 1]
    const rx = Math.max((nodeW + gapX) / chord, prev ? prev.rx + Math.SQRT2 * (nodeW + gapX) : Math.SQRT2 * ((centerW + nodeW) / 2 + gapX))
    const ry = Math.max((nodeH + gapY) / chord, prev ? prev.ry + Math.SQRT2 * (nodeH + gapY) : Math.SQRT2 * ((centerH + nodeH) / 2 + gapY))
    radii.push({ rx, ry, m })
  }
  const last = radii[radii.length - 1]!
  const sx = Math.max(1, (width / 2 - nodeW / 2 - pad) / last.rx)
  const sy = Math.max(1, (height / 2 - nodeH / 2 - pad) / last.ry)
  const W = Math.max(width, 2 * last.rx * sx + nodeW + 2 * pad)
  const H = Math.max(height, 2 * last.ry * sy + nodeH + 2 * pad)
  const cx = W / 2
  const cy = H / 2
  const nodes: Box[] = []
  for (let i = 0; i < count; i++) {
    const ring = Math.floor(i / PER_RING)
    const { rx, ry, m } = radii[ring]!
    const k = i - ring * PER_RING
    // Odd rings are turned half a step, so rings do not line up.
    const a = -Math.PI / 2 + ((k + (ring % 2) * 0.5) / m) * Math.PI * 2
    nodes.push({ x: cx + rx * sx * Math.cos(a), y: cy + ry * sy * Math.sin(a) })
  }
  return { center: { x: cx, y: cy }, nodes, width: W, height: H }
}

/** The scale that fits a world into the stage (never above 1). */
export function fitScale(world: { width: number; height: number }, stage: { width: number; height: number }): number {
  if (world.width <= 0 || world.height <= 0) return 1
  return Math.min(1, stage.width / world.width, stage.height / world.height)
}

// ── Messages on the edges ──

/** Which edge a message travels: a worker's, towards the center or out of it; `center` = only the orchestrator glows. */
export type Travel = { node: string; inward: boolean } | { center: true } | null

export function travelOf(m: CrewMessage, snap: CrewSnapshot): Travel {
  const orch = snap.crew.orchestratorId
  const task = m.taskId ? snap.tasks.find((t) => t.id === m.taskId) : null
  if (task) {
    if (m.toId === orch || m.fromId === task.workerId || m.kind === 'escalation') return { node: task.id, inward: true }
    return { node: task.id, inward: false }
  }
  // A peer message runs between two workers: shown on the sender's edge.
  const member = snap.members.find((x) => x.threadId === m.fromId && x.taskId)
  if (member?.taskId) return { node: member.taskId, inward: true }
  if (m.fromId === orch || m.toId === orch || m.fromId === 'user' || m.toId === 'user') return { center: true }
  return null
}

// ── The bus ──

/** `14:59:16.907`. */
export function busTime(ms: number): string {
  const d = new Date(ms)
  const p = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`
}

/** Where a bus line leads: the thread the message shows in, and words to find there. */
export function jumpTarget(m: CrewMessage, snap: CrewSnapshot): { threadId: string; query: string } | null {
  const orch = snap.crew.orchestratorId
  const task = m.taskId ? snap.tasks.find((t) => t.id === m.taskId) : null
  const key = task?.key ?? ''
  const head = (s: string, n = 48) => firstLine(s, n + 1).replace(/…$/, '')
  switch (m.kind) {
    case 'reply':
    case 'steer':
      return { threadId: m.toId, query: head(m.text) }
    case 'dispatch':
      return { threadId: m.toId, query: `Your task: ${key}` }
    case 'ask':
      return { threadId: orch, query: m.id }
    case 'progress': {
      const note = m.text.split(' — ').slice(1).join(' — ')
      return { threadId: m.fromId, query: head(note || m.text) }
    }
    case 'done':
    case 'failed':
    case 'escalation':
      return { threadId: orch, query: `[Crew] ${key}` }
    case 'gate':
      return { threadId: orch, query: head(m.text.split(':')[0] ?? m.text) }
    case 'decision':
      return { threadId: orch, query: '[Crew] The user' }
    case 'peer':
    case 'note':
      return m.fromId.length > 20 ? { threadId: m.fromId, query: head(m.text) } : { threadId: orch, query: key || head(m.text) }
    default:
      return { threadId: orch, query: key || head(m.text) }
  }
}

/** `6 agents on R2-07 crew demo · 42 messages on the bus`. */
export function footerLine(snap: CrewSnapshot): string {
  const agents = snap.tasks.length + 1
  const on = snap.crew.objective ? ` on ${snap.crew.objective}` : ''
  return `${agents} agents${on} · ${snap.messageCount} message${snap.messageCount === 1 ? '' : 's'} on the bus`
}

/** `1m 05s` for a node's time. */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000))
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, '0')}s`
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`
}
