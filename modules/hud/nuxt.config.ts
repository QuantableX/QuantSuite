import { fileURLToPath } from 'node:url'

/**
 * QuantHUD — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas".
 *
 * QuantHUD kept its sources at the repo root (Nuxt 3 style). They now live under
 * `app/`, which is the Nuxt 4 default.
 *
 * Its routes render in their own windows, not in the shell's stage — the shell
 * suppresses its chrome for any window that is not `main` (ARCHITECTURE.md §10).
 */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  alias: {
    '#hud': moduleSrc,
  },

  components: [{ path: moduleComponents, prefix: 'Hud' }],
  css: [moduleCss],
})
