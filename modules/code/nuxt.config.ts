import { fileURLToPath } from 'node:url'

/**
 * QuantCode — Nuxt Layer (docs/PLAN-QUANTSPACE.md).
 *
 * The editor module of the QuantSpace app. It owns **no crate**: files and git
 * come from `qs.files` (the suite-wide primitives that happen to live in the
 * canvas plugin), terminals from `qs.console`, and the editor itself from
 * `QCodeEditor` in packages/ui. What is new here is the workbench around them —
 * editor groups, tabs, breadcrumbs, diagnostics, search.
 *
 * Components carry the `Code` prefix: component names are global across layers
 * and a collision is silent — the wrong component renders and nothing errors
 * (ARCHITECTURE.md §1 "Layer gotchas"). `imports.dirs` is deliberately unused
 * for the same reason; this module imports its own symbols explicitly.
 */
const moduleRoot = fileURLToPath(new URL('.', import.meta.url))
const moduleComponents = fileURLToPath(new URL('./app/components', import.meta.url))

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],

  alias: {
    '#code-root': moduleRoot,
  },

  components: [{ path: moduleComponents, prefix: 'Code' }],

  vite: {
    optimizeDeps: {
      include: ['monaco-editor'],
    },
  },
})
