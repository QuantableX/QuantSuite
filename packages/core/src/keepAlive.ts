/**
 * The V3 warm cache's one lifecycle helper — modules and the shell's own pages
 * stay mounted in a deactivated `<KeepAlive>` stage, and everything global they
 * hold has to stand down for that time.
 */

import { getCurrentInstance } from 'vue'

/**
 * True unless a KeepAlive ancestor is currently deactivated — the same walk
 * Vue runs before it invokes an `activated`/`deactivated` hook.
 *
 * Why anything armed in `onMounted` needs it: layouts and pages resolve async
 * and can mount into a stage the user has already switched away from. Vue then
 * skips their `onActivated` (deactivated ancestor) while the stage's
 * `onDeactivated` flush is long past, so a window listener, an interval or a
 * bus subscription started in `onMounted` would run for a module nobody is
 * looking at — until that module is visited and left once. Arming in
 * `onMounted` still has to happen, though: a page that mounts into an *active*
 * stage gets no `onActivated` for that first visit either.
 *
 * Call it from inside the hook — Vue sets the current instance while a
 * lifecycle hook runs, so there is nothing to capture during setup.
 */
export function inActiveKeepAliveTree(): boolean {
  for (let i = getCurrentInstance(); i; i = i.parent) if (i.isDeactivated) return false
  return true
}
