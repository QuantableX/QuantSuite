/**
 * The composer's context rules (card 9), free of Vue for node tests:
 * which trigger (`@` files and threads, `/` commands, `$` skills) the caret
 * is in, how a pick replaces it, and the inline chips — labels between two
 * invisible marks (FSI … PDI) that move, copy and delete like one character.
 */

export const CHIP_OPEN = '⁨'
export const CHIP_CLOSE = '⁩'

export type TriggerKind = '@' | '/' | '$'

export interface Trigger {
  kind: TriggerKind
  /** What was typed after the trigger character. */
  query: string
  /** Index of the trigger character. */
  start: number
}

/**
 * The trigger the caret is in, if any. `@` and `$` start a word; `/` only
 * starts a line. `@"…` keeps spaces until the closing quote.
 */
export function triggerAt(text: string, caret: number): Trigger | null {
  const before = text.slice(0, caret)
  // `@"path with spaces` (no closing quote yet)
  const quoted = /(^|[\s(])@"([^"\n]*)$/.exec(before)
  if (quoted) return { kind: '@', query: quoted[2], start: caret - quoted[2].length - 2 }
  const m = /(^|[\s(])([@$])([^\s@$"]*)$/.exec(before)
  if (m) {
    if (m[3].includes(CHIP_OPEN) || m[3].includes(CHIP_CLOSE)) return null
    return { kind: m[2] as TriggerKind, query: m[3], start: caret - m[3].length - 1 }
  }
  const slash = /(^|\n)\/([\w:.-]*)$/.exec(before)
  if (slash) return { kind: '/', query: slash[2], start: caret - slash[2].length - 1 }
  return null
}

/** Replace the trigger and its query with `insert` plus a space; the caret lands after it. */
export function replaceTrigger(text: string, t: Trigger, caret: number, insert: string): { text: string; caret: number } {
  const after = text.slice(caret)
  const glue = after.startsWith(' ') || after.startsWith('\n') ? '' : ' '
  const out = text.slice(0, t.start) + insert + glue + after
  return { text: out, caret: t.start + insert.length + glue.length }
}

// ── Chips ──

export function chipToken(label: string): string {
  return `${CHIP_OPEN}${label}${CHIP_CLOSE}`
}

/** Labels of the chips in a text, in order. */
export function chipsIn(text: string): string[] {
  const out: string[] = []
  const re = new RegExp(`${CHIP_OPEN}([^${CHIP_CLOSE}]*)${CHIP_CLOSE}`, 'g')
  for (const m of text.matchAll(re)) out.push(m[1])
  return out
}

/** A label no other chip in the text uses: "notes.md", "notes.md (2)". */
export function uniqueLabel(label: string, text: string): string {
  const taken = new Set(chipsIn(text))
  if (!taken.has(label)) return label
  for (let n = 2; ; n++) if (!taken.has(`${label} (${n})`)) return `${label} (${n})`
}

/**
 * The chip a Backspace (`dir` -1) or Delete (`dir` 1) at `caret` removes
 * whole, as [start, end) — or null when the caret is not next to one.
 */
export function chipNextTo(text: string, caret: number, dir: -1 | 1): [number, number] | null {
  if (dir < 0 && text[caret - 1] === CHIP_CLOSE) {
    const start = text.lastIndexOf(CHIP_OPEN, caret - 1)
    return start >= 0 ? [start, caret] : null
  }
  if (dir > 0 && text[caret] === CHIP_OPEN) {
    const end = text.indexOf(CHIP_CLOSE, caret)
    return end >= 0 ? [caret, end + 1] : null
  }
  return null
}

/** A caret inside a chip moves to its edge (the nearer one in `dir`). */
export function outsideChip(text: string, caret: number, dir: -1 | 1 = 1): number {
  const open = text.lastIndexOf(CHIP_OPEN, caret - 1)
  const close = text.lastIndexOf(CHIP_CLOSE, caret - 1)
  if (open < 0 || open < close) return caret
  const end = text.indexOf(CHIP_CLOSE, open)
  if (end < 0 || end < caret) return caret
  return dir < 0 ? open : end + 1
}

/** The text with chips as `[label]` — for copying out and for history. */
export function plainText(text: string): string {
  return text.replace(new RegExp(`${CHIP_OPEN}([^${CHIP_CLOSE}]*)${CHIP_CLOSE}`, 'g'), '[$1]')
}

/** Text and chip segments, for rendering a sent message. */
export function segments(text: string): { chip: boolean; text: string }[] {
  const out: { chip: boolean; text: string }[] = []
  let rest = text
  while (rest) {
    const open = rest.indexOf(CHIP_OPEN)
    const close = open >= 0 ? rest.indexOf(CHIP_CLOSE, open) : -1
    if (open < 0 || close < 0) {
      out.push({ chip: false, text: rest })
      break
    }
    if (open > 0) out.push({ chip: false, text: rest.slice(0, open) })
    out.push({ chip: true, text: rest.slice(open + 1, close) })
    rest = rest.slice(close + 1)
  }
  return out
}

// ── Commands ──

export interface CommandRow {
  name: string
  description: string
  argumentHint: string
  /** `app` = QuantPilot runs it; `provider` = the agent does. */
  source: 'app' | 'provider'
}

/** What QuantPilot does itself (T3 Code's app commands); more arrive with their cards. */
export const APP_COMMANDS: CommandRow[] = [
  { name: 'new', description: 'Start a new thread in this project', argumentHint: '', source: 'app' },
  { name: 'model', description: 'Choose the agent and model', argumentHint: '', source: 'app' },
  { name: 'permissions', description: 'Choose the permission mode', argumentHint: '', source: 'app' },
  { name: 'plan', description: 'Switch this thread to Plan mode', argumentHint: '', source: 'app' },
  { name: 'rename', description: 'Rename this thread', argumentHint: '<title>', source: 'app' },
  { name: 'copy', description: "Copy the agent's latest answer", argumentHint: '', source: 'app' },
  { name: 'pwd', description: "Show this thread's folder", argumentHint: '', source: 'app' },
  { name: 'fork', description: 'Fork this thread from the latest message', argumentHint: '', source: 'app' },
  { name: 'restart', description: 'Restart the agent session (after changing skills or MCP)', argumentHint: '', source: 'app' },
  { name: 'tui', description: "Open this conversation in the agent's own terminal UI", argumentHint: '', source: 'app' },
  { name: 'usage', description: "Show this agent's usage limits", argumentHint: '', source: 'app' },
]

/** App commands first when they match, then the provider's; prefix matches before substrings. */
export function commandRows(provider: { name: string; description: string; argumentHint?: string }[], query: string): CommandRow[] {
  const q = query.toLowerCase()
  const all: CommandRow[] = [
    ...APP_COMMANDS,
    ...provider
      .filter((c) => !APP_COMMANDS.some((a) => a.name === c.name))
      .map((c) => ({ name: c.name, description: c.description, argumentHint: c.argumentHint ?? '', source: 'provider' as const })),
  ]
  const rank = (c: CommandRow) => (c.name.toLowerCase().startsWith(q) ? 0 : c.name.toLowerCase().includes(q) ? 1 : c.description.toLowerCase().includes(q) ? 2 : 3)
  return all
    .map((c, i) => ({ c, r: rank(c), i }))
    .filter((x) => !q || x.r < 3)
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((x) => x.c)
}

/** An app command at the start of a message: what to run instead of sending. */
export function appCommand(text: string): { name: string; arg: string } | null {
  const m = /^\/([\w-]+)(?:\s+([\s\S]*))?$/.exec(text.trim())
  if (!m || !APP_COMMANDS.some((c) => c.name === m[1])) return null
  return { name: m[1], arg: (m[2] ?? '').trim() }
}
