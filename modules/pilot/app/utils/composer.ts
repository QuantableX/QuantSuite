/**
 * The composer's editing rules (docs/PLAN-QUANTPILOT.md, card 4; T3 Code's
 * docs/user/composer.md §Formatting and keybindings.md §Composer controls).
 *
 * The composer writes Markdown and what was typed is what is sent —
 * markers, numbering and all. These pure functions decide what a key does:
 * continue a list, indent, stay inside a code block, send, queue or steer.
 * The component only applies the result.
 */
import type { FollowUp, SendShortcut } from '../types'

/** Characters a message may hold (T3 Code's limit). */
export const MAX_CHARS = 120_000
/** A paste this large becomes a file the agent reads on demand. */
export const LARGE_PASTE_BYTES = 32 * 1024

/** The new text and selection after an edit. */
export interface Edit {
  text: string
  start: number
  end: number
}

function lineBounds(text: string, pos: number): { start: number; end: number } {
  const start = text.lastIndexOf('\n', pos - 1) + 1
  const nl = text.indexOf('\n', pos)
  return { start, end: nl < 0 ? text.length : nl }
}

/** Whether `pos` sits inside an open ``` block. */
export function inCodeFence(text: string, pos: number): boolean {
  const before = text.slice(0, lineBounds(text, pos).start)
  let open = false
  for (const line of before.split('\n')) if (/^\s*```/.test(line)) open = !open
  return open
}

const LIST = /^(\s*)(?:([-*+])|(\d+)([.)]))(\s+)(\[[ xX]\]\s+)?/
const QUOTE = /^(\s*>\s?)/

/**
 * Shift+Enter (and Enter inside a code block): a new line that keeps the
 * structure — the next list marker, the quote, the code indentation. On an
 * empty item it steps out one level, then leaves the list.
 */
export function newLine(text: string, start: number, end: number): Edit {
  const { start: ls, end: le } = lineBounds(text, start)
  const line = text.slice(ls, le)
  const insert = (s: string): Edit => {
    const t = text.slice(0, start) + s + text.slice(end)
    return { text: t, start: start + s.length, end: start + s.length }
  }

  if (inCodeFence(text, start)) {
    const indent = /^\s*/.exec(line)?.[0] ?? ''
    return insert('\n' + indent)
  }
  // Only at the end of the line does a list or quote continue.
  if (start !== end || start !== le) return insert('\n')

  const m = LIST.exec(line)
  if (m) {
    const [whole, indent, bullet, num, delim, gap, box] = m
    if (line.trim() === whole.trim()) {
      // An empty item: out one level, or out of the list.
      const out = indent.length >= 2 ? indent.slice(2) + line.slice(indent.length) : ''
      const t = text.slice(0, ls) + out + text.slice(le)
      const pos = ls + out.length
      return { text: t, start: pos, end: pos }
    }
    const marker = bullet ?? `${Number(num) + 1}${delim}`
    return insert(`\n${indent}${marker}${gap}${box ? '[ ] ' : ''}`)
  }
  const q = QUOTE.exec(line)
  if (q) {
    if (line.trim() === '>') {
      const t = text.slice(0, ls) + text.slice(le)
      return { text: t, start: ls, end: ls }
    }
    return insert('\n' + q[1])
  }
  return insert('\n')
}

/**
 * Tab / Shift+Tab: nest or un-nest list items and indent code-block lines.
 * `null` means Tab keeps its usual meaning (focus moves on).
 */
export function indent(text: string, start: number, end: number, dir: 1 | -1): Edit | null {
  const first = lineBounds(text, start).start
  const last = lineBounds(text, end).end
  const block = text.slice(first, last)
  const lines = block.split('\n')
  const code = inCodeFence(text, start)
  if (!code && !lines.every((l) => LIST.test(l) || !l.trim())) return null
  let delta0 = 0
  let total = 0
  const out = lines.map((l, i) => {
    if (dir > 0) {
      if (i === 0) delta0 = 2
      total += 2
      return '  ' + l
    }
    const n = Math.min(2, /^ */.exec(l)?.[0].length ?? 0)
    if (i === 0) delta0 = -n
    total -= n
    return l.slice(n)
  })
  const t = text.slice(0, first) + out.join('\n') + text.slice(last)
  return { text: t, start: Math.max(first, start + delta0), end: Math.max(first, end + total) }
}

export function isMultiline(text: string): boolean {
  return text.includes('\n')
}

export type KeyAction =
  | 'send'
  | 'queue'
  | 'steer'
  /** Ctrl+Shift+Enter: the oldest queued message goes into the running turn. */
  | 'steerQueued'
  /** Ctrl+Alt+Enter: send, keep it running, open a fresh thread. */
  | 'sendNewThread'
  | 'newline'
  | 'none'

export interface KeyState {
  shift: boolean
  /** Ctrl (Cmd on macOS). */
  mod: boolean
  alt: boolean
  shortcut: SendShortcut
  followUp: FollowUp
  running: boolean
  canSteer: boolean
  text: string
  caret: number
}

function followUp(state: KeyState, opposite: boolean): KeyAction {
  const steer = (state.followUp === 'steer') !== opposite
  return steer && state.canSteer ? 'steer' : 'queue'
}

/** What Enter does, given the modifiers, the setting and the thread's state. */
export function enterAction(s: KeyState): KeyAction {
  if (s.mod && s.alt) return 'sendNewThread'
  const multiline = isMultiline(s.text)
  const sendKeyNeedsMod = s.shortcut === 'modEnter' || (s.shortcut === 'modEnterMultiline' && multiline)
  if (!sendKeyNeedsMod) {
    if (s.mod && s.shift) return s.running ? 'steerQueued' : 'none'
    if (s.shift) return 'newline'
    if (inCodeFence(s.text, s.caret) && !s.mod) return 'newline'
    if (!s.running) return 'send'
    return followUp(s, s.mod)
  }
  // The send key is Ctrl+Enter; Shift added to it is the opposite action.
  if (!s.mod) return 'newline'
  if (!s.running) return 'send'
  return followUp(s, s.shift)
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function inline(line: string): string {
  // Inline code first: nothing inside it is styled.
  const parts = line.split(/(`[^`\n]+`)/g)
  return parts
    .map((p, i) => {
      if (i % 2 === 1) return `<span class="qp-hl-code">${esc(p)}</span>`
      let h = esc(p)
      h = h.replace(/(\*\*|__)(?=\S)([^*_]+?)(?<=\S)\1/g, '<span class="qp-hl-mark">$1</span><span class="qp-hl-strong">$2</span><span class="qp-hl-mark">$1</span>')
      h = h.replace(/(^|[^*\w])(\*|_)(?=\S)([^*_\n]+?)(?<=\S)\2(?![*\w])/g, '$1<span class="qp-hl-mark">$2</span><span class="qp-hl-em">$3</span><span class="qp-hl-mark">$2</span>')
      h = h.replace(/\[([^\]\n]+)\]\(([^)\s]+)\)/g, '<span class="qp-hl-mark">[</span><span class="qp-hl-link">$1</span><span class="qp-hl-mark">]($2)</span>')
      h = h.replace(/(^|\s)(@[\w./\\-]+|\$[\w:-]+|#\d+)/g, '$1<span class="qp-hl-token">$2</span>')
      return h
    })
    .join('')
}

/**
 * The composer's backdrop: the same text, coloured — never resized or
 * re-weighted, so the caret in the transparent textarea above stays aligned.
 */
export function highlight(text: string): string {
  let fence = false
  const lines = text.split('\n').map((line, i) => {
    if (/^\s*```/.test(line)) {
      fence = !fence
      return `<span class="qp-hl-fence">${esc(line)}</span>`
    }
    if (fence) return `<span class="qp-hl-code">${esc(line)}</span>`
    if (i === 0 && /^\/[\w:-]+/.test(line)) {
      const m = /^(\/[\w:-]+)(.*)$/.exec(line)!
      return `<span class="qp-hl-token">${esc(m[1])}</span>${inline(m[2])}`
    }
    let m = /^(#{1,6}\s)(.*)$/.exec(line)
    if (m) return `<span class="qp-hl-mark">${esc(m[1])}</span><span class="qp-hl-heading">${inline(m[2])}</span>`
    m = /^(\s*>\s?)(.*)$/.exec(line)
    if (m) return `<span class="qp-hl-mark">${esc(m[1])}</span><span class="qp-hl-quote">${inline(m[2])}</span>`
    m = /^(\s*(?:[-*+]|\d+[.)])\s+(?:\[[ xX]\]\s+)?)(.*)$/.exec(line)
    if (m) return `<span class="qp-hl-mark">${esc(m[1])}</span>${inline(m[2])}`
    if (/^\s*(---|\*\*\*|___)\s*$/.test(line)) return `<span class="qp-hl-mark">${esc(line)}</span>`
    return inline(line)
  })
  // A trailing newline needs a character after it to take up its line.
  return lines.join('\n') + (text.endsWith('\n') ? ' ' : '')
}

/** "48 KB" for an attachment chip. */
export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}
