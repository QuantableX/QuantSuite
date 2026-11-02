/**
 * One pair of document listeners for every rendered message (parity 21).
 * Each Markdown body used to add its own `selectionchange` and `keydown`
 * listeners — some 1,600 with 400 turns on screen, all run on every key and
 * selection change. Now the message that holds the selection gets the
 * selection change (and the one that held it before, to let go), and the
 * last one gets the keys.
 */
export interface MessageEvents {
  selection: () => void
  key: (e: KeyboardEvent) => void
}

const byRoot = new Map<Element, MessageEvents>()
let last: MessageEvents | null = null
let armed = false

function hostOf(node: Node | null): MessageEvents | null {
  const el = node instanceof Element ? node : node?.parentElement
  const md = el?.closest('.qp-md')
  return md ? byRoot.get(md) ?? null : null
}

function onSelection() {
  const here = hostOf(window.getSelection()?.anchorNode ?? null)
  if (last && last !== here) last.selection()
  here?.selection()
  last = here ?? last
}

function onKey(e: KeyboardEvent) {
  last?.key(e)
}

export function registerMessage(root: Element, events: MessageEvents) {
  byRoot.set(root, events)
  if (!armed) {
    armed = true
    document.addEventListener('selectionchange', onSelection)
    document.addEventListener('keydown', onKey)
  }
}

export function unregisterMessage(root: Element) {
  const events = byRoot.get(root)
  byRoot.delete(root)
  if (events && last === events) last = null
}

/** For tests and diagnostics. */
export function registeredMessages(): number {
  return byRoot.size
}
