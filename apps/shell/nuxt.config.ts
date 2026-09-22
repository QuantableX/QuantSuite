import { lstatSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import tailwindcss from '@tailwindcss/vite'

const uiComponents = fileURLToPath(new URL('../../packages/ui/src/components', import.meta.url))

// The workspace packages, pinned to THIS checkout. npm links
// `node_modules/@quantsuite/{core,ui}` to the main checkout by absolute path,
// and a card worktree (`.qs-worktrees/<id>/`) shares the main `node_modules`
// through a junction: without these aliases every import of `@quantsuite/core`
// / `@quantsuite/ui` inside a worktree resolved to MAIN's `packages/*`, so a
// card that edits them could neither type-check nor run from its worktree
// (PLAN-WORKTREES.md "Build hygiene"). Resolved from `import.meta.url` like the
// modules' `#<id>` aliases, so on main it is the very directory npm already
// resolves. Directory aliases on the `src/` each package's `exports` points
// into, so subpaths (`@quantsuite/ui/logos`) keep working; Nuxt writes them
// into the generated tsconfig `paths`, which is how vue-tsc follows them.
const corePackage = fileURLToPath(new URL('../../packages/core/src', import.meta.url))
const uiPackage = fileURLToPath(new URL('../../packages/ui/src', import.meta.url))

// Where Nuxt builds. Nuxt 4 moves a production build out of `.nuxt` into
// `node_modules/.cache/nuxt/.nuxt` whenever a `.nuxt` (from dev or prepare)
// already exists, so `nuxt build` cannot clobber a running dev server's files
// (@nuxt/kit, loadNuxtConfig — only when the config sets no `buildDir`). In a
// card worktree `node_modules` is a junction to the MAIN checkout's, so that
// cache is main's: every worktree build overwrote the same generated files as
// main's, and nitro's prerender, importing its build artefacts back through
// the junction, failed on every route with "Received protocol 'c:'"
// (2026-09-04). When the dependency directory is not this checkout's own —
// Node reports a junction as a symbolic link — the build stays in this
// checkout's `.nuxt`, the layout Nuxt 3 always used; a worktree never runs
// dev and build at the same time. Main's `node_modules` is a real directory,
// so there Nuxt's own rule keeps applying and nothing changes.
const shellDir = fileURLToPath(new URL('.', import.meta.url))
const sharedDeps = (() => {
  try {
    return lstatSync(join(shellDir, 'node_modules')).isSymbolicLink()
  } catch {
    return false
  }
})()

export default defineNuxtConfig({
  ...(sharedDeps ? { buildDir: join(shellDir, '.nuxt') } : {}),

  runtimeConfig: {
    public: { appVersion: JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8')).version },
  },

  ssr: false,
  devtools: { enabled: false },
  telemetry: false,
  compatibilityDate: '2025-01-01',

  // Module layers land here, one line per migration phase.
  extends: [
    '../../modules/systems',
    '../../modules/notes',
    '../../modules/algo',
    '../../modules/mcp',
    '../../modules/pilot',
    '../../modules/canvas',
    '../../modules/terminal',
    '../../modules/console',
    '../../modules/code',
    '../../modules/flow',
    '../../modules/plan',
    '../../modules/habit',
    '../../modules/finance',
    '../../modules/memory',
    '../../modules/script',
    '../../modules/hud',
  ],

  alias: {
    '@quantsuite/core': corePackage,
    '@quantsuite/ui': uiPackage,
  },

  css: ['~/assets/css/shell.css'],

  components: [
    { path: '~/components', pathPrefix: false },
    { path: uiComponents, pathPrefix: false },
  ],

  app: {
    head: {
      title: 'QuantSuite',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      ],
    },
  },

  vite: {
    // The single Tailwind pass for the whole app. Module stylesheets must NOT
    // carry their own `@import 'tailwindcss'` — see ARCHITECTURE.md §9.
    plugins: [tailwindcss()],
    clearScreen: false,
    server: { strictPort: true },
    envPrefix: ['VITE_', 'TAURI_'],
  },

  devServer: { port: 1420 },
})
