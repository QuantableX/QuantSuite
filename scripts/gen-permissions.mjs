#!/usr/bin/env node
/**
 * Generate a Tauri plugin's `permissions/default.toml` from the COMMANDS list
 * in its `build.rs`.
 *
 *   node scripts/gen-permissions.mjs modules/notes/crate [--exclude cmd,cmd]
 *
 * Tauri denies any command without a permission, and the allow-* names are
 * mechanical (`snake_case` -> `allow-kebab-case`). Hand-writing 54 of them is
 * just a way to introduce a typo that only shows up at runtime.
 *
 * Anything genuinely privileged should be excluded and granted explicitly
 * instead of shipping in the default set.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const [, , crateDir, ...flags] = process.argv
if (!crateDir) {
  console.error('usage: gen-permissions.mjs <crate-dir> [--exclude a,b] [--description "..."]')
  process.exit(1)
}

function flagValue(name) {
  const i = flags.indexOf(name)
  return i >= 0 ? flags[i + 1] : undefined
}

const exclude = new Set((flagValue('--exclude') ?? '').split(',').filter(Boolean))
const description = flagValue('--description')

const buildRs = readFileSync(join(crateDir, 'build.rs'), 'utf8')
const block = buildRs.match(/const COMMANDS:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/)
if (!block) {
  console.error(`no COMMANDS array found in ${crateDir}/build.rs`)
  process.exit(1)
}

const commands = [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
const kept = commands.filter((c) => !exclude.has(c))
const skipped = commands.filter((c) => exclude.has(c))

const pkgName = (readFileSync(join(crateDir, 'Cargo.toml'), 'utf8').match(/name\s*=\s*"([^"]+)"/) ?? [])[1]
const plugin = (pkgName ?? '').replace(/^tauri-plugin-/, '')

const lines = [
  '"$schema" = "schemas/schema.json"',
  '',
  '[default]',
  `description = "${description ?? `Default permissions for the ${plugin} module.`}"`,
  'permissions = [',
  ...kept.map((c) => `  "allow-${c.replace(/_/g, '-')}",`),
  ']',
  '',
]

const dir = join(crateDir, 'permissions')
if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
writeFileSync(join(dir, 'default.toml'), lines.join('\n'))

console.log(
  `  ${crateDir}: ${kept.length} permission(s) written` +
    (skipped.length ? `, ${skipped.length} excluded (${skipped.join(', ')})` : '')
)
