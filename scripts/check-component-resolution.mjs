#!/usr/bin/env node
/**
 * Verify every component tag in a module template actually resolves.
 *
 *   node scripts/check-component-resolution.mjs [--fix]
 *
 * Reads Nuxt's generated `components.d.ts` — the authoritative registry, rather
 * than re-deriving names from file paths, which is what went wrong before:
 *
 *   - `Workspace/WorkspaceSwitcher.vue` is `WorkspaceSwitcher` (dir collapsed),
 *     a hand-rolled rule dropped the file segment instead and produced
 *     `Workspace`, so the component was never prefixed;
 *   - `UI/NotesBar.vue` is `UiNotesBar`, but PascalCasing "UI" naively gives
 *     `UINotesBar`, so the rewrite missed it and the prefix orphaned the tag.
 *
 * An unresolved tag does not fail the build. Vue renders nothing and logs at
 * runtime, which is exactly how QuantCode ended up looking broken.
 *
 * `--fix` rewrites a tag when exactly one registered component matches it
 * case-insensitively ignoring the module prefix. Ambiguous or unknown tags are
 * always reported, never guessed.
 */

import { readdirSync, readFileSync, writeFileSync, statSync, existsSync } from 'node:fs'
import { join, relative, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const MODULES = join(ROOT, 'modules')
const DTS = join(ROOT, 'apps', 'shell', '.nuxt', 'components.d.ts')
const fix = process.argv.includes('--fix')

if (!existsSync(DTS)) {
  console.error(`  skip  ${relative(ROOT, DTS)} not generated yet — run \`nuxt prepare\` first`)
  process.exit(0)
}

/** registered component name -> absolute source path */
const registry = new Map()
for (const m of readFileSync(DTS, 'utf8').matchAll(
  /export const (\w+): typeof import\("([^"]+)"\)/g
)) {
  const [, name, importPath] = m
  if (name.startsWith('Lazy')) continue
  registry.set(name, resolve(dirname(DTS), importPath))
}

/** Vue/Nuxt built-ins and anything a template may legitimately use unregistered. */
const BUILTIN = /^(Nuxt|Client|Dev|Lazy|Transition|TransitionGroup|Teleport|Suspense|Component|KeepAlive|Slot|Template)/

const problems = []
let fixed = 0

function walk(dir, out = []) {
  if (!existsSync(dir)) return out
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) walk(p, out)
    else out.push(p)
  }
  return out
}

const migrated = readdirSync(MODULES)
  .filter((d) => statSync(join(MODULES, d)).isDirectory() && existsSync(join(MODULES, d, 'module.json')))
  .filter((d) => JSON.parse(readFileSync(join(MODULES, d, 'module.json'), 'utf8')).status === 'migrated')

for (const id of migrated) {
  const files = walk(join(MODULES, id)).filter((f) => f.endsWith('.vue'))

  for (const file of files) {
    let text = readFileSync(file, 'utf8')
    const tpl = text.match(/<template>([\s\S]*)<\/template>/)
    if (!tpl) continue

    // Any imported binding can be used as a tag — a default import of a .vue
    // file, but also a named import from a package (TipTap's `EditorContent`).
    const explicit = new Set()
    for (const m of text.matchAll(/import\s+(?:type\s+)?([^'"]+?)\s+from\s+['"][^'"]+['"]/g)) {
      const clause = m[1]
      const named = clause.match(/\{([^}]*)\}/)
      if (named) {
        for (const part of named[1].split(',')) {
          const id = part.trim().split(/\s+as\s+/).pop()?.trim()
          if (id) explicit.add(id)
        }
      }
      const def = clause.replace(/\{[^}]*\}/, '').replace(/,/g, '').trim()
      if (def && /^\w+$/.test(def)) explicit.add(def)
    }

    const seen = new Set()
    for (const m of tpl[1].matchAll(/<([A-Z][A-Za-z0-9]*)/g)) {
      const tag = m[1]
      if (seen.has(tag) || BUILTIN.test(tag) || explicit.has(tag) || registry.has(tag)) continue
      seen.add(tag)

      // Candidates, from this module only, in decreasing confidence:
      //   1. exact match once the module prefix is stripped
      //      (<ExchangeForm> -> <AlgoExchangeForm>)
      //   2. the tag is the tail of a registered name — the component sits in a
      //      subdirectory the template never spelled out
      //      (<EquityCurve> -> <AlgoChartsEquityCurve>)
      //
      // Case-insensitive throughout: Nuxt renders `UI/` as `Ui`, which is
      // exactly the mismatch that orphaned QuantCode's status bar.
      const prefix = id[0].toUpperCase() + id.slice(1)
      const own = [...registry.keys()].filter(
        (name) => name.startsWith(prefix) && registry.get(name).includes(join('modules', id))
      )
      const lower = tag.toLowerCase()

      let candidates = own.filter((name) => name.slice(prefix.length).toLowerCase() === lower)
      if (candidates.length === 0) {
        candidates = own.filter((name) => name.toLowerCase().endsWith(lower))
      }

      if (candidates.length === 1 && fix) {
        const [target] = candidates
        text = text
          .replace(new RegExp(`<${tag}(?=[\\s/>])`, 'g'), `<${target}`)
          .replace(new RegExp(`</${tag}>`, 'g'), `</${target}>`)
        fixed++
        continue
      }

      problems.push(
        `${relative(ROOT, file)}: <${tag}> does not resolve` +
          (candidates.length === 1
            ? ` — did you mean <${candidates[0]}>? (run with --fix)`
            : candidates.length > 1
              ? ` — ambiguous: ${candidates.join(', ')}`
              : ` — no registered component matches; it renders as nothing at runtime`)
      )
    }

    if (fix && text !== readFileSync(file, 'utf8')) writeFileSync(file, text)
  }
}

for (const p of problems) console.error(`  ERROR ${p}`)

if (fix) console.log(`  ok    ${fixed} tag(s) rewritten`)

if (problems.length) {
  console.error(`\n${problems.length} unresolved component tag(s)`)
  process.exit(1)
}

console.log(`  ok    every component tag in ${migrated.length} module(s) resolves`)
