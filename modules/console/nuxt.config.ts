import { fileURLToPath } from 'node:url'

/**
 * QuantConsole — Nuxt Layer (docs/PLAN-CONSOLE.md, phase P0).
 *
 * Components carry the `Console` prefix, which is what keeps `Pane.vue` from
 * colliding with another module's `Pane.vue` (ARCHITECTURE.md §1 "Layer
 * gotchas": component names are global across layers, and the collision is
 * silent — the wrong component renders and nothing errors).
 *
 * `imports.dirs` is deliberately not used; this module imports its own
 * composables explicitly for the same reason.
 */
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  components: [{ path: moduleComponents, prefix: 'Console' }],
  css: [moduleCss],

  vite: {
    // xterm is loaded dynamically inside the pane (it touches `window` on
    // import), so pre-bundling it keeps the first pane from stalling on a
    // cold dep-optimize.
    optimizeDeps: {
      include: ['@xterm/xterm', '@xterm/addon-unicode11'],
    },
  },
})
