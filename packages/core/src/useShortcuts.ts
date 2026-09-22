/**
 * The suite's one keyboard-shortcut composable (V3) — replaces the notes and
 * systems copies.
 *
 * Where the two disagreed, the wider systems behaviour is kept: `ctrl` also
 * matches the macOS Cmd key (the text-input guard below already treated Meta
 * as a modifier), and `<select>` counts as a text input so its type-ahead is
 * not stolen. Both are inert for QuantNotes' bindings, which are all `ctrl: true`.
 */

import { onActivated, onDeactivated, onMounted, onUnmounted } from 'vue'
import { inActiveKeepAliveTree } from './keepAlive'

export interface ShortcutBinding {
  key: string
  ctrl?: boolean
  shift?: boolean
  alt?: boolean
  handler: (event: KeyboardEvent) => void
}

function isTextInput(target: EventTarget | null): boolean {
  // `closest` only exists on Elements. A keydown dispatched at `window` or
  // `document` — which is what a synthetic event does, and what some libraries
  // do — would otherwise throw here and take every binding in the handler with
  // it: the shortcut silently stops working and the stack trace points at a
  // guard clause nobody suspects.
  if (!(target instanceof Element)) return false
  return !!target.closest('input, textarea, [contenteditable="true"], select')
}

export function useShortcuts(bindings: ShortcutBinding[]) {
  function onKeydown(event: KeyboardEvent) {
    // A handler closer to the key — a command editor accepting its suggestion on
    // Ctrl+F, an embedded widget claiming a chord — signals "handled" with
    // preventDefault. Matching anyway runs the action a second time on the same
    // keystroke (the console's Ctrl+F both accepted the suggestion and opened
    // the search bar), so a handled event is not ours to match.
    if (event.defaultPrevented) return
    for (const binding of bindings) {
      if (binding.ctrl != null && binding.ctrl !== (event.ctrlKey || event.metaKey)) continue
      if (binding.shift != null && binding.shift !== event.shiftKey) continue
      if (binding.alt != null && binding.alt !== event.altKey) continue
      if (binding.key.toLowerCase() !== event.key.toLowerCase()) continue
      if (isTextInput(event.target) && binding.key.length === 1 && !event.ctrlKey && !event.metaKey) continue
      binding.handler(event)
      break
    }
  }

  // V3 warm cache: the host stays mounted in the module's cached stage while
  // another module is active, so the window listener has to stand down then.
  // Arming happens in BOTH onMounted and onActivated — layouts load async and
  // mount after the stage's activation flush, so onActivated alone would miss
  // the first visit; a duplicate (type, fn) pair is ignored by the DOM. But
  // that late mount can also land in a stage the user has already switched
  // away from, where arming blindly would leave this module's keys firing
  // inside another one — hence the guard.
  onMounted(() => {
    if (inActiveKeepAliveTree()) window.addEventListener('keydown', onKeydown)
  })
  onActivated(() => window.addEventListener('keydown', onKeydown))
  onDeactivated(() => window.removeEventListener('keydown', onKeydown))
  onUnmounted(() => window.removeEventListener('keydown', onKeydown))
}
