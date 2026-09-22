import { fileURLToPath } from 'node:url'

/** QuantFinance — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas". */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#finance': moduleSrc,
  },

  // Auto-imported components are GLOBAL by name across every layer. Without a
  // per-module prefix, AppHeader / LeftSidebar / RightSidebar collide between
  // modules and the last layer silently wins (ARCHITECTURE.md §1).
  components: [{ path: moduleComponents, prefix: 'Finance' }],
  css: [moduleCss],
})
