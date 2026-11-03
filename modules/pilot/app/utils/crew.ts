/**
 * Crews (round 2, card 07): what the crew panel, the gate bar and the log
 * show — labels, counts, who said what to whom. Pure, so the node tests
 * import it.
 */
import type { CrewMessage, CrewSnapshot, CrewTask, CrewTaskStatus } from '#pilot/types'

export const STATUS_LABEL: Record<CrewTaskStatus, string> = {
  waiting: 'Waiting',
  running: 'Working',
  asking: 'Asking',
  stalled: 'Stalled',
  done: 'Done',
  failed: 'Failed',
  stopped: 'Stopped',
}

/** The colour family of a task's state. */
export function statusTone(status: CrewTaskStatus): 'run' | 'wait' | 'warn' | 'ok' | 'bad' | 'mute' {
  switch (status) {
    case 'running':
      return 'run'
    case 'asking':
      return 'wait'
    case 'stalled':
      return 'warn'
    case 'done':
      return 'ok'
    case 'failed':
      return 'bad'
    default:
      return 'mute'
  }
}

/** The agent icon's ring for a task's worker. */
export function iconStatus(status: CrewTaskStatus): 'working' | 'waiting' | 'error' | 'idle' {
  if (status === 'running') return 'working'
  if (status === 'asking' || status === 'stalled') return 'waiting'
  if (status === 'failed') return 'error'
  return 'idle'
}

export function settled(status: string): boolean {
  return status === 'done' || status === 'failed' || status === 'stopped'
}

export interface CrewCounts {
  total: number
  done: number
  running: number
  failed: number
  /** Not settled yet. */
  open: number
}

export function counts(tasks: CrewTask[]): CrewCounts {
  return {
    total: tasks.length,
    done: tasks.filter((t) => t.status === 'done').length,
    running: tasks.filter((t) => t.status === 'running').length,
    failed: tasks.filter((t) => t.status === 'failed').length,
    open: tasks.filter((t) => !settled(t.status)).length,
  }
}

/** `3/6` — done of all. */
export function countLabel(tasks: CrewTask[]): string {
  const c = counts(tasks)
  return `${c.done}/${c.total}`
}

/** The crew a thread leads, else the one it works in (the newest). */
export function crewOfThread(crews: CrewSnapshot[], threadId: string | null): CrewSnapshot | null {
  if (!threadId) return null
  const led = crews.filter((c) => c.crew.orchestratorId === threadId)
  if (led.length) return led.reduce((a, b) => (b.crew.createdAt > a.crew.createdAt ? b : a))
  const member = crews.filter((c) => c.members.some((m) => m.threadId === threadId))
  return member.length ? member.reduce((a, b) => (b.crew.createdAt > a.crew.createdAt ? b : a)) : null
}

/** Every crew worker thread: the sidebar lists them under their orchestrator's crew, not on their own. */
export function workerIds(crews: CrewSnapshot[]): Set<string> {
  const out = new Set<string>()
  for (const c of crews) for (const m of c.members) if (m.role === 'worker') out.add(m.threadId)
  return out
}

export function pendingGates(snap: CrewSnapshot | null): CrewMessage[] {
  return (snap?.open ?? []).filter((m) => m.kind === 'gate' && m.status === 'pending')
}

export function openQuestions(snap: CrewSnapshot | null): CrewMessage[] {
  return (snap?.open ?? []).filter((m) => m.kind === 'ask' && m.status === 'open')
}

/** `merge: May I merge?` → name and question. */
export function gateParts(text: string): { name: string; question: string } {
  const i = text.indexOf(':')
  if (i < 0) return { name: text.trim(), question: '' }
  return { name: text.slice(0, i).trim(), question: text.slice(i + 1).trim() }
}

/** Who a log line is from or to, in words. */
export function party(id: string, snap: CrewSnapshot): string {
  if (id === 'user') return 'You'
  if (id === 'quantpilot') return 'QuantPilot'
  if (id === 'crew') return 'Crew'
  if (id === snap.crew.orchestratorId) return 'Orchestrator'
  const member = snap.members.find((m) => m.threadId === id)
  const task = member?.taskId ? snap.tasks.find((t) => t.id === member.taskId) : null
  if (task) {
    const attempts = snap.members.filter((m) => m.taskId === task.id).length
    return attempts > 1 && member ? `${task.key} #${member.attempt}` : task.key
  }
  return snap.titles[id] || 'a thread'
}

export const KIND_LABEL: Record<string, string> = {
  plan: 'planned',
  dispatch: 'dispatched',
  steer: 'steered',
  ask: 'asked',
  reply: 'answered',
  progress: 'progress',
  done: 'done',
  failed: 'failed',
  escalation: 'escalated',
  gate: 'gate',
  decision: 'decided',
  peer: 'message',
  stop: 'stopped',
  retry: 'retry',
  note: 'note',
}

export function kindLabel(kind: string): string {
  return KIND_LABEL[kind] ?? kind
}

function hhmm(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
}

/** The whole log as plain text (Copy). */
export function logText(messages: CrewMessage[], snap: CrewSnapshot): string {
  return messages.map((m) => `${hhmm(m.at)}  ${party(m.fromId, snap)} → ${party(m.toId, snap)}  [${kindLabel(m.kind)}]  ${m.text.trim()}`).join('\n')
}

export { hhmm as logTime }

/** A task's line under its key: the latest note, why it waits, or its outcome. */
export function taskDetail(t: CrewTask): string {
  if (t.status === 'done' || t.status === 'failed') return (t.status === 'failed' && t.error ? t.error : t.summary) || ''
  return t.note || ''
}
