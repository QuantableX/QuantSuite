/**
 * Quotes of earlier replies (parity 14): where they came from and how to
 * go back. A quote keeps its text snapshot whatever happens to the source;
 * going back opens the source item (older history included, parity 02) and
 * highlights the quoted text there — the right one when the phrase repeats.
 */
export interface QuoteSource {
  threadId: string
  itemId: string
  occurrence: number
}

/** `pilot-item:<thread>:<item>:<occurrence>` */
export function parseQuotePath(path: string | null | undefined): QuoteSource | null {
  const m = /^pilot-item:([^:]+):([^:]+):(\d+)$/.exec(path ?? '')
  return m ? { threadId: m[1]!, itemId: m[2]!, occurrence: Number(m[3]) } : null
}

/**
 * The `occurrence`-th place `quote` appears in `text`, ignoring whitespace
 * (a selection's text and the DOM's text differ in line breaks), as
 * [start, end) offsets into `text`.
 */
export function findQuote(text: string, quote: string, occurrence = 0): [number, number] | null {
  const map: number[] = []
  let compact = ''
  for (let i = 0; i < text.length; i++) {
    if (!/\s/.test(text[i]!)) {
      compact += text[i]
      map.push(i)
    }
  }
  const q = quote.replace(/\s+/g, '')
  if (!q) return null
  let at = -1
  for (let n = 0; n <= occurrence; n++) {
    at = compact.indexOf(q, at + 1)
    if (at < 0) return null
  }
  return [map[at]!, map[at + q.length - 1]! + 1]
}

/** A DOM range for [start, end) of an element's text. */
export function rangeOf(root: HTMLElement, start: number, end: number): Range | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  const range = document.createRange()
  let pos = 0
  let started = false
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const len = n.textContent?.length ?? 0
    if (!started && start < pos + len) {
      range.setStart(n, start - pos)
      started = true
    }
    if (started && end <= pos + len) {
      range.setEnd(n, end - pos)
      return range
    }
    pos += len
  }
  return null
}

let clearTimer: ReturnType<typeof setTimeout> | undefined

/** Highlight the quote inside the item's element; false when the text is no longer there. */
export function highlightQuote(el: HTMLElement, quote: string, occurrence: number): boolean {
  const hit = findQuote(el.textContent ?? '', quote, occurrence)
  if (!hit) return false
  const range = rangeOf(el, hit[0], hit[1])
  if (!range) return false
  const css = (globalThis as any).CSS
  if (css?.highlights && (globalThis as any).Highlight) {
    css.highlights.set('qp-quote', new (globalThis as any).Highlight(range))
    clearTimeout(clearTimer)
    clearTimer = setTimeout(() => css.highlights.delete('qp-quote'), 6000)
  }
  ;(range.startContainer.parentElement ?? el).scrollIntoView({ block: 'center', behavior: 'smooth' })
  return true
}
