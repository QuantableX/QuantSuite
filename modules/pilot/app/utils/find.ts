/**
 * Find in a thread (parity 15). The backend (`pilot_thread_find`, history.rs)
 * lists the matching items of the whole thread with their match counts; this
 * file finds the same matches in the rendered text of a mounted item. Both
 * compare text the same way: case folded per character, chip marks and
 * Markdown emphasis characters (* _ ` ~) dropped, whitespace runs as one
 * space — so the counts agree.
 */
export interface FindHit {
  itemId: string
  turnId: string
  seq: number
  kind: string
  parent: string | null
  count: number
  snippet: string
}

export interface FindResult {
  query: string
  total: number
  items: number
  hits: FindHit[]
  truncated: boolean
}

const SKIP = new Set([0x2a, 0x5f, 0x60, 0x7e, 0x2068, 0x2069]) // * _ ` ~ and the chip marks FSI, PDI
// Unicode White_Space, as Rust's char::is_whitespace (JS \s differs in U+0085 and U+FEFF).
const isSpace = (c: number) => (c >= 0x09 && c <= 0x0d) || c === 0x20 || c === 0x85 || c === 0xa0 || c === 0x1680 || (c >= 0x2000 && c <= 0x200a) || c === 0x2028 || c === 0x2029 || c === 0x202f || c === 0x205f || c === 0x3000

/** The searchable form of `text`; per UTF-16 unit, the source span [start, end). */
export function normalize(text: string): { norm: string; start: number[]; end: number[] } {
  let norm = ''
  const start: number[] = []
  const end: number[] = []
  let i = 0
  for (const ch of text) {
    const at = i
    i += ch.length
    const cp = ch.codePointAt(0)!
    if (SKIP.has(cp)) continue
    if (isSpace(cp)) {
      if (!norm.endsWith(' ')) {
        norm += ' '
        start.push(at)
        end.push(i)
      }
      continue
    }
    const low = ch.toLowerCase()
    norm += low
    for (let k = 0; k < low.length; k++) {
      start.push(at)
      end.push(i)
    }
  }
  return { norm, start, end }
}

/** The query in searchable form, without spaces at the ends. */
export function needleOf(query: string): string {
  return normalize(query).norm.replace(/^ +| +$/g, '')
}

/** Non-overlapping matches of `needle` in `text`, as source offsets [start, end). */
export function matchesIn(text: string, needle: string): [number, number][] {
  if (!needle) return []
  const { norm, start, end } = normalize(text)
  const out: [number, number][] = []
  for (let at = norm.indexOf(needle); at >= 0; at = norm.indexOf(needle, at + needle.length)) {
    out.push([start[at]!, end[at + needle.length - 1]!])
  }
  return out
}

/** The element holding an item's readable text: a message, commentary or plan. */
export function textHost(el: Element): HTMLElement | null {
  return el.querySelector<HTMLElement>('.qp-md, .qp-um-text')
}

/** DOM ranges of every match in a mounted item. */
export function domMatches(el: Element, needle: string): Range[] {
  const host = textHost(el)
  if (!host || !needle) return []
  const nodes: { node: Text; at: number }[] = []
  let text = ''
  const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT)
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    // A diagram's drawing is not readable text; its source is.
    if ((n.parentElement as HTMLElement | null)?.closest('svg')) continue
    nodes.push({ node: n as Text, at: text.length })
    text += n.nodeValue ?? ''
  }
  const locate = (offset: number, isEnd: boolean) => {
    // The text node containing `offset` (an end may sit at a node's end).
    let lo = 0
    let hi = nodes.length - 1
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      const m = nodes[mid]!
      if (m.at < offset || (!isEnd && m.at === offset)) lo = mid
      else hi = mid - 1
    }
    const hit = nodes[lo]!
    return { node: hit.node, offset: offset - hit.at }
  }
  return matchesIn(text, needle).map(([s, e]) => {
    const a = locate(s, false)
    const b = locate(e, true)
    const r = document.createRange()
    r.setStart(a.node, a.offset)
    r.setEnd(b.node, b.offset)
    return r
  })
}

/** Where the global match `index` lies: hit and occurrence in it. */
export function locateMatch(hits: FindHit[], index: number): { hit: number; n: number } | null {
  let left = index
  for (let h = 0; h < hits.length; h++) {
    const c = hits[h]!.count
    if (left < c) return { hit: h, n: left }
    left -= c
  }
  return null
}

/** The global index of occurrence `n` of hit `hit`. */
export function matchIndex(hits: FindHit[], hit: number, n: number): number {
  let i = 0
  for (let h = 0; h < hit; h++) i += hits[h]!.count
  return i + n
}
