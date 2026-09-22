/**
 * QuantConsole's rows in the suite command palette — plain-terminal edition.
 *
 * Contributed through `registerPaletteActions` so `QCommandPalette` never
 * learns this module exists (ARCHITECTURE.md §1).
 *
 * Every action row prints its chord as the `hint`, read from `keymap.ts` — one
 * source, so the palette can never advertise a key the settings panel does not
 * show. The hint is the *shipped* chord: the palette registers at app start,
 * and a user's rebind lives in settings the page applies.
 *
 * Navigation goes through the `qss:navigate` window event rather than this
 * layer's router: the row is chosen inside a `packages/ui` dialog the shell
 * owns, and the alternative captures a router reference at plugin init that
 * goes stale after an HMR reload. The *action* rows reach the console through
 * its Pinia store instead — called fresh inside each `run`, never captured.
 */
import { moduleById, registerPaletteActions, type PaletteAction } from '@quantsuite/core'
import { CONSOLE_ACTIONS, prettyBinding } from '../keymap'
import { useConsoleStore } from '../stores/console'

/** The palette's group heading. From the manifest so it cannot drift. */
const GROUP = moduleById('console')?.title ?? 'QuantConsole'

function navigate(route: string) {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route } }))
}

/** An action's shipped chord, spelled for a keycap — the single keymap source. */
function chord(actionId: string): string | undefined {
  const spec = CONSOLE_ACTIONS.find((action) => action.id === actionId)?.defaultBinding
  return spec ? prettyBinding(spec) : undefined
}

/** Whether the console has a shell group for a pane action to land in. */
function hasShellTab(): boolean {
  const store = useConsoleStore()
  return store.activeTab?.kind === 'shell' && !!store.activeTab.root
}

const ACTIONS: PaletteAction[] = [
  {
    id: 'console.newTab',
    label: 'New shell tab',
    group: GROUP,
    keywords: 'terminal shell pty console prompt',
    hint: 'go',
    // Navigation, not a `createTab()`: the row has to work from every module,
    // and `?new=1` is what the page does with the request.
    run: () => navigate('/console?new=1'),
  },
  {
    id: 'console.suiteProcesses',
    label: 'Suite processes',
    group: GROUP,
    keywords: 'processes sidecars gateway running tasks',
    hint: 'go',
    run: () => navigate('/console?view=suite'),
  },
  {
    id: 'console.splitRight',
    label: 'Split right',
    group: GROUP,
    keywords: 'pane split vertical divide terminal',
    hint: chord('splitRight'),
    when: hasShellTab,
    run: () => {
      // Store first, then navigate: the mutation is synchronous, the
      // navigation is an event the shell answers.
      useConsoleStore().splitActive('row')
      navigate('/console')
    },
  },
  {
    id: 'console.splitDown',
    label: 'Split down',
    group: GROUP,
    keywords: 'pane split horizontal divide terminal',
    hint: chord('splitDown'),
    when: hasShellTab,
    run: () => {
      useConsoleStore().splitActive('col')
      navigate('/console')
    },
  },
  {
    id: 'console.zoomPane',
    label: 'Zoom pane',
    group: GROUP,
    keywords: 'pane zoom maximize focus fullscreen restore',
    hint: chord('zoomPane'),
    // Zooming needs something to hide — the store refuses a single-pane zoom,
    // so the row appears only when it would do anything.
    when: () => {
      const store = useConsoleStore()
      const tab = store.activeTab
      return !!tab && tab.kind === 'shell' && !!tab.root && store.tabPanes(tab).length > 1
    },
    run: () => {
      useConsoleStore().toggleZoom()
      navigate('/console')
    },
  },
]

export default defineNuxtPlugin(() => {
  // Static rows, no backend and nothing async: Ctrl+K has them from app start,
  // with or without a Tauri bridge; `when()` keeps the pane actions honest
  // before the console has anything to act on.
  registerPaletteActions('console', ACTIONS)
})
