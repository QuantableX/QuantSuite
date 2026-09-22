import { fileURLToPath } from 'node:url'

/**
 * QuantCanvas — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas".
 *
 * This module keeps the standalone app's shape: `lib/`, `shared/` and `stores/`
 * sit beside `app/`, not inside it. `#canvas` points at `app/`, `#canvas-root`
 * at the module root, so both are reachable without depth-sensitive `../../`
 * paths.
 *
 * Note `imports.dirs` is deliberately NOT used: auto-imported symbols are global
 * across layers and would collide. This module imports its own explicitly.
 */
const moduleRoot = fileURLToPath(new URL('.', import.meta.url))
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#canvas': moduleSrc,
    '#canvas-root': moduleRoot,
  },

  components: [{ path: moduleComponents, prefix: 'Canvas' }],
  css: [moduleCss],

  vite: {
    optimizeDeps: {
      include: ['monaco-editor'],
    },
  },
})
