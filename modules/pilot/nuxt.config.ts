import { fileURLToPath } from 'node:url'

/** QuantPilot — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas". */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#pilot': moduleSrc,
  },

  // Auto-imported components are GLOBAL by name across every layer; the
  // per-module prefix keeps Face / Transcript / Composer from colliding.
  components: [{ path: moduleComponents, prefix: 'Pilot' }],
  css: [moduleCss],
})
