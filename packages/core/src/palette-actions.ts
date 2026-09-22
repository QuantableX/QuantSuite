/**
 * The command palette's extension point (PLAN-CONSOLE.md §P6).
 *
 * `QCommandPalette` is ONE surface for the whole suite (Ctrl+K). Its own rows —
 * modules, entities, notes, to-dos, files — stay hardcoded there because they
 * are suite chrome. A module contributes its own rows by registering them from
 * a client plugin in its layer, conventionally
 * `modules/<id>/app/plugins/<id>-palette.client.ts`:
 *
 *   registerPaletteActions('console', [
 *     { id: 'console.history', label: 'Command history', group: 'QuantConsole',
 *       hint: 'go', run: () => navigate('/console/history') },
 *   ])
 *
 * This exists so `packages/ui` never imports from `modules/**` (ARCHITECTURE.md
 * §1) — the palette knows the shape of an action, not which modules there are.
 */

import { shallowRef } from 'vue'
import { isModuleEnabled } from './app-availability'

export interface PaletteAction {
  /** Stable id, module-prefixed: `console.newTab`. */
  id: string
  label: string
  /** Group heading in the palette; usually the module title. */
  group: string
  /** Extra words the search should match — a workflow's command, a path. */
  keywords?: string
  /** Right-aligned hint, the way the existing rows show "go". */
  hint?: string
  /**
   * Runs on Enter. The palette closes unless this returns `false` — or resolves
   * to it, since an async action's rejection has nowhere to be shown once the
   * palette is gone (see `runRegistered` in QCommandPalette).
   */
  run: () => void | boolean | Promise<void | boolean>
  /** Cheap dynamic entries: re-read on every palette open, not on a timer. */
  when?: () => boolean
  /**
   * Only offered once something is typed.
   *
   * For rows derived from data — a module's saved workflows, say. Without it a
   * module with a dozen of them fills the palette's whole first screen and pushes
   * the entity, note and file results below the fold, which is a regression for
   * everyone who uses the palette as a jump list.
   */
  queryOnly?: boolean
}

/**
 * A `shallowRef` over a replaced Map, not a plain Map: QCommandPalette watches
 * `paletteActions`, so a module registering after the palette mounted has to
 * invalidate that watcher. A mutated Map is not a reactive dependency and the
 * watcher would never fire — which is precisely the two cases that matter, an
 * HMR re-run and a module whose rows arrive from a backend after app start.
 */
const registry = shallowRef(new Map<string, PaletteAction[]>())

/**
 * Registers a module's rows, **replacing** whatever it registered before.
 *
 * Replace rather than merge-by-id (which is what `registerSettingsSections`
 * does) because these lists are derived from data, not from a fixed set of
 * components: a module that re-registers after a saved workflow was deleted
 * must lose that row, and a merge would keep it forever. Replacing also makes
 * an HMR re-run idempotent, which is the duplicate-rows failure.
 */
export function registerPaletteActions(moduleId: string, actions: PaletteAction[]): void {
  registry.value = new Map(registry.value).set(moduleId, [...actions])
}

/**
 * Everything registered, grouped runs kept contiguous so the palette can put
 * one heading above each group without sorting rows itself. Registration order
 * decides which group comes first; a module's own array order is preserved.
 *
 * `when()` is deliberately NOT evaluated here — the caller decides how often it
 * runs, and the palette runs it once per open.
 */
export function paletteActions(): PaletteAction[] {
  const byGroup = new Map<string, PaletteAction[]>()
  for (const [moduleId, actions] of registry.value) {
    if (!isModuleEnabled(moduleId)) continue
    for (const action of actions) {
      const bucket = byGroup.get(action.group)
      if (bucket) bucket.push(action)
      else byGroup.set(action.group, [action])
    }
  }
  return [...byGroup.values()].flat()
}
