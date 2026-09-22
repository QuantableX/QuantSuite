import { fileURLToPath } from 'node:url'

/**
 * QuantTerminal — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas".
 *
 * The standalone app used `@nuxtjs/tailwindcss` (the v3-era module) and
 * `@nuxtjs/color-mode`. Neither is here: the shell issues the single Tailwind v4
 * import, and theming is `qs-core`'s job.
 */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt'],

  alias: {
    '#terminal': moduleSrc,
  },

  components: [{ path: moduleComponents, prefix: 'Terminal' }],
  css: [moduleCss],
})
