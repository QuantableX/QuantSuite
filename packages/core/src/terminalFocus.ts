/**
 * Whether the keyboard currently belongs to a terminal.
 *
 * Suite-wide since 2026-08-26: QuantCanvas, QuantConsole and QuantCode all host
 * `ConsolePane` and all bind keys a shell wants, so the boundary belongs here
 * rather than inside one module's utils.
 *
 * These surfaces bind a handful of single-modifier keys — Ctrl+B, Ctrl+J, Ctrl+O,
 * Ctrl+P, Ctrl+Tab, Space — that a shell claims too: Ctrl+P is "previous
 * command" in PSReadLine, Ctrl+O is "execute and keep the line" in readline, and
 * Space is just a space. The usual `instanceof HTMLInputElement` guard never
 * catches a terminal, because a terminal is a scroll region with a hidden
 * textarea rather than a form field, so without this the canvas won every one of
 * those keys out from under the shell.
 *
 * `.console-pane` is the root the console module puts around a session, so it is
 * also the boundary of "inside a terminal" — the block list, the input editor and
 * xterm's own textarea all sit below it.
 */
const PANE_ROOT = '.console-pane'

export function terminalHasFocus(target?: EventTarget | null): boolean {
  if (typeof document === 'undefined') return false
  // The event's own target when there is one: a keydown handled on `window`
  // during a composition can outrun `document.activeElement`.
  const from = target instanceof Element ? target : document.activeElement
  return !!from?.closest(PANE_ROOT)
}
