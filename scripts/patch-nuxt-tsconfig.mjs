#!/usr/bin/env node
/**
 * Workaround for an upstream Nuxt bug.
 *
 * Nuxt 4.2 writes `vue-router/volar/sfc-route-blocks` into every generated
 * tsconfig's `vueCompilerOptions.plugins`. No published vue-router exports that
 * subpath — verified against 4.5.0, 4.5.1, 4.6.0 and 4.6.4 — so vue-tsc throws
 * MODULE_NOT_FOUND before it type-checks a single file.
 *
 * It is not a version mismatch and downgrading does not help. The `prepare:types`
 * hook only reaches one of the five tsconfigs Nuxt emits, so this strips the
 * entry from all of them instead.
 *
 * It must leave everything else as Nuxt wrote it, `compilerOptions.paths`
 * above all: that is where the `@quantsuite/core` / `@quantsuite/ui` aliases
 * from apps/shell/nuxt.config.ts land, which pin a worktree's typecheck to its
 * own packages/* (PLAN-WORKTREES.md "Build hygiene"). Hence the JSON
 * round-trip that touches only the plugin list.
 *
 * Delete this script and the `pretypecheck` hook once Nuxt drops the stale
 * reference.
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { readdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const BAD_PLUGIN = 'vue-router/volar/sfc-route-blocks'
const SHELL = join(dirname(fileURLToPath(import.meta.url)), '..', 'apps', 'shell')

const dirs = [
  join(SHELL, '.nuxt'),
  join(SHELL, 'node_modules', '.cache', 'nuxt', '.nuxt'),
]

let patched = 0

for (const dir of dirs) {
  if (!existsSync(dir)) continue
  for (const name of readdirSync(dir)) {
    if (!name.startsWith('tsconfig') || !name.endsWith('.json')) continue
    const file = join(dir, name)

    let raw
    try {
      raw = readFileSync(file, 'utf8')
    } catch {
      continue
    }
    if (!raw.includes(BAD_PLUGIN)) continue

    // These files are plain JSON as emitted by Nuxt.
    let json
    try {
      json = JSON.parse(raw)
    } catch {
      console.warn(`  warn  ${file} is not parseable JSON — skipped`)
      continue
    }

    const plugins = json?.vueCompilerOptions?.plugins
    if (!Array.isArray(plugins)) continue

    json.vueCompilerOptions.plugins = plugins.filter((p) => p !== BAD_PLUGIN)
    writeFileSync(file, JSON.stringify(json, null, 2) + '\n')
    patched++
  }
}

console.log(`  ok    patched ${patched} generated tsconfig(s) — removed "${BAD_PLUGIN}"`)
