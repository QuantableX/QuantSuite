/**
 * Side chats (round 2, card 10): the bounded "main thread context" every
 * side message carries — the main thread's title and state and what was
 * said there since the side chat last asked, newest kept, about 24 KB at
 * most, saying what was left out. Pure, so the node tests import it.
 */

export const SIDE_CONTEXT_MAX = 24 * 1024

export interface MainItem {
  kind: string
  seq: number
  data: { text?: unknown; streaming?: unknown }
}

export interface MainContext {
  text: string
  /** The newest main item included — the next message starts after it. */
  lastSeq: number
  included: number
  omitted: number
}

/** `items` of the main thread, in order; only what came after `sinceSeq` counts. */
export function mainContext(main: { title: string; state: string }, items: MainItem[], sinceSeq: number, max = SIDE_CONTEXT_MAX): MainContext {
  const fresh = items.filter((i) => i.seq > sinceSeq && (i.kind === 'user' || i.kind === 'assistant') && String(i.data.text ?? '').trim())
  const lastSeq = items.length ? Math.max(sinceSeq, ...items.map((i) => i.seq)) : sinceSeq
  const head = `Main thread "${main.title || 'New thread'}" — ${main.state || 'idle'}.`
  if (!fresh.length) return { text: `${head}\nNothing new there since your last question.`, lastSeq, included: 0, omitted: 0 }
  const blocks: string[] = []
  let size = head.length + 80
  let omitted = 0
  for (let i = fresh.length - 1; i >= 0; i--) {
    const it = fresh[i]!
    const who = it.kind === 'user' ? 'User' : it.data.streaming ? 'Agent (still writing)' : 'Agent'
    let body = String(it.data.text).trim()
    const room = max - size - who.length - 8
    if (room <= 200) {
      omitted = i + 1
      break
    }
    if (body.length > room) body = `… ${body.slice(body.length - room + 2)}`
    blocks.unshift(`## ${who}\n${body}`)
    size += body.length + who.length + 8
  }
  const note = omitted ? `\n(${omitted} earlier message${omitted === 1 ? '' : 's'} left out.)` : ''
  return { text: `${head} What was said there since your last question:${note}\n\n${blocks.join('\n\n')}`, lastSeq, included: fresh.length - omitted, omitted }
}

/** "Side: what about the tests?" — a side chat's title from its first question. */
export function sideTitle(question: string): string {
  const q = question.replace(/\s+/g, ' ').trim()
  if (!q) return 'Side chat'
  return `Side: ${q.length > 48 ? `${q.slice(0, 48).trimEnd()}…` : q}`
}
