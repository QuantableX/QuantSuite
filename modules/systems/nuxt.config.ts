import { fileURLToPath } from 'node:url'

/**
 * QuantSystems — Nuxt Layer.
 *
 * Three things worth knowing before copying this for the next module:
 *
 * 1. `~/` in a layer's config resolves against the SHELL's srcDir, not the
 *    layer's. A layer that writes `css: ['~/assets/css/main.css']` fails the
 *    build with "css entry could not be found". Resolve from `import.meta.url`.
 *
 * 2. `~/` INSIDE a layer's source files works for Vite but not for vue-tsc —
 *    the generated tsconfig maps `~/*` to the shell only. With eight modules it
 *    would be ambiguous anyway (whose `~/stores/app`?). Each module therefore
 *    gets its own alias and its sources import via `#systems/...`.
 *
 * 3. No `@import 'tailwindcss'` and no `@tailwindcss/vite` plugin here — the
 *    shell issues the single Tailwind pass for the whole app
 *    (ARCHITECTURE.md §9). Adding it back generates the utility layer twice.
 */
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))
const moduleCss = fileURLToPath(new URL('./app/assets/css/main.css', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#systems': moduleSrc,
  },

  // Auto-imported components are GLOBAL by name across every layer. Without a
  // per-module prefix, RightSidebar / ThemeToggle / ConfirmModal / Modal /
  // StatusBar collide between modules and the last layer silently wins
  // (ARCHITECTURE.md 1, "Layer gotchas").
  components: [{ path: moduleComponents, prefix: 'Systems' }],
  css: [moduleCss],

  vite: {
    optimizeDeps: {
      include: ['lightweight-charts'],
    },
  },
})
