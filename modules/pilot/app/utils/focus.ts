/**
 * F6 / Shift+F6 (card 17; T3 Code's keyboard-focus.md): the keyboard moves
 * between the module's areas — threads sidebar, conversation, message box,
 * inspector, terminal — wrapping around, skipping hidden ones, and lands on
 * the control that had the focus there last.
 */

interface Area {
  id: string
  root: string
  /** Where to land without a remembered control. */
  first: string
}

const AREAS: Area[] = [
  { id: 'sidebar', root: '.qp-side', first: '.qp-row[tabindex="0"], .qp-row.active, .qp-row, button' },
  { id: 'conversation', root: '.qp-tl-scroll', first: '' },
  { id: 'composer', root: '.qp-composer', first: '.qp-c-input' },
  { id: 'inspector', root: '.qp-insp', first: '.qp-insp-tab.on, [tabindex="0"], button' },
  { id: 'terminal', root: '.qp-dr', first: '.xterm-helper-textarea, button' },
]

const last = new Map<string, HTMLElement>()

function visible(el: Element | null): el is HTMLElement {
  return el instanceof HTMLElement && el.getClientRects().length > 0
}

/** A `focusin` inside the module: remember it for its area. */
export function noteFocus(target: EventTarget | null, scope: ParentNode) {
  if (!(target instanceof HTMLElement)) return
  for (const a of AREAS) {
    const root = scope.querySelector(a.root)
    if (root?.contains(target)) {
      last.set(a.id, target)
      return
    }
  }
}

/** The next (or previous) area gets the focus; `false` when there is none. */
export function moveFocus(dir: 1 | -1, scope: ParentNode): boolean {
  const areas = AREAS.map((a) => ({ a, el: scope.querySelector(a.root) })).filter((x): x is { a: Area; el: HTMLElement } => visible(x.el))
  if (!areas.length) return false
  const active = document.activeElement
  const at = areas.findIndex((x) => active && x.el.contains(active))
  const next = areas[at < 0 ? (dir === 1 ? 0 : areas.length - 1) : (at + dir + areas.length) % areas.length]
  const remembered = last.get(next.a.id)
  const target =
    (remembered?.isConnected && next.el.contains(remembered) && visible(remembered) ? remembered : null) ??
    (next.a.first ? [...next.el.querySelectorAll<HTMLElement>(next.a.first)].find(visible) : null) ??
    next.el
  if (!target.hasAttribute('tabindex') && target === next.el) target.setAttribute('tabindex', '-1')
  target.focus({ preventScroll: next.a.id !== 'sidebar' })
  return true
}
