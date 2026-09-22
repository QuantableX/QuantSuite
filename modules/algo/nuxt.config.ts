import { fileURLToPath } from 'node:url'

/** QuantAlgo — Nuxt Layer. See ARCHITECTURE.md §1 "Layer gotchas". */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#algo': moduleSrc,
  },

  // Auto-imported components are GLOBAL by name across every layer. Without a
  // per-module prefix, RightSidebar / ThemeToggle / ConfirmModal / Modal /
  // StatusBar collide between modules and the last layer silently wins
  // (ARCHITECTURE.md 1, "Layer gotchas").
  components: [{ path: moduleComponents, prefix: 'Algo' }],
  css: [moduleCss],

  vite: {
    optimizeDeps: {
      include: ['monaco-editor', 'lightweight-charts'],
    },
  },
})
