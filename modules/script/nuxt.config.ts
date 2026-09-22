import { fileURLToPath } from 'node:url'

/**
 * QuantScript — Nuxt Layer (docs/PLAN-QUANTSCRIPT.md). Born inside the suite
 * like QuantConsole: components carry the `Script` prefix, composables are
 * imported explicitly, the stylesheet is scoped from its first line and uses
 * the suite tokens only.
 */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt'],

  alias: {
    '#script': moduleSrc,
  },

  components: [{ path: moduleComponents, prefix: 'Script' }],
  css: [moduleCss],

  vite: {
    optimizeDeps: {
      include: ['monaco-editor'],
    },
  },
})
