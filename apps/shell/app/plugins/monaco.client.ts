/**
 * Monaco's web workers, registered once for the whole app.
 *
 * Four surfaces load Monaco — QuantCode's editor (`QCodeEditor`), QuantCanvas'
 * file window, QuantAlgo's strategy editor — and `MonacoEnvironment` has to be
 * set BEFORE the first of them imports it. A component that installs it in its
 * own `onMounted` is already too late: Monaco's TypeScript mode starts as soon
 * as the module is imported, finds no worker factory, falls back to the AMD
 * loader and throws on `require.toUrl`.
 *
 * A plugin is the one place early enough for all of them. Client-only, because
 * the factory touches `self` — the shell is `ssr: false` anyway.
 */
import { installMonacoEnvironment } from '@quantsuite/ui'

export default defineNuxtPlugin(() => {
  installMonacoEnvironment()
})
