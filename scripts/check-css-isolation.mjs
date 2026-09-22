#!/usr/bin/env node
/**
 * Guard for ARCHITECTURE.md §9: no module stylesheet may reach outside its own
 * subtree.
 *
 * Runs over the module sources (not the bundle, so it also covers modules whose
 * CSS is not currently imported) and asserts every top-level rule is one of:
 *
 *   - `:root` / `[data-theme=...]`      custom properties only, deliberately global
 *   - scoped under `[data-module="<id>"]`
 *   - an at-rule whose body carries its own scoping (@keyframes, @font-face, ...)
 *
 * The failure this prevents is silent: an unscoped `.btn` or `body` rule in one
 * module restyles every other module, and nobody notices until a screenshot
 * comparison. Phase 1 found `.card`, `.btn`, `.input`, `.label` and `.pill` in a
 * single stylesheet — generic enough to collide with anything.
 */

import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const MODULES = join(ROOT, 'modules')

/**
 * Selectors allowed to stay global.
 *
 * Beyond `:root` / `[data-theme=…]`, this admits **class-qualified document
 * roots** — `.dark`, `.light`, `html.x`, `body.x`. Two reasons:
 *
 *   - A theme class lands on `<html>`, an *ancestor* of `[data-module="…"]`, so
 *     scoping it makes it unmatchable. That is what silently pinned QuantView to
 *     its light palette no matter what the theme was set to.
 *   - Unlike a bare `body { … }`, a class-qualified one is inert until that
 *     module sets the class, so it cannot clobber another module.
 *
 * A bare `html` or `body` selector stays forbidden.
 */
const GLOBAL_OK =
  /^(:root|\[data-theme[^\]]*\]|html\[data-theme[^\]]*\]|\.(dark|light)|(html|body)\.[\w-]+)$/
const SELF_SCOPED_AT = /^@(keyframes|font-face|property|charset|import|theme|layer)/

const problems = []

function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, '')
}

/** Top-level chunk splitter — brace-aware, string-aware. */
function splitTopLevel(css) {
  const out = []
  let depth = 0
  let start = 0
  let str = null
  for (let i = 0; i < css.length; i++) {
    const c = css[i]
    if (str) {
      if (c === '\\') i++
      else if (c === str) str = null
      continue
    }
    if (c === '"' || c === "'") { str = c; continue }
    if (c === '{') depth++
    else if (c === '}') {
      depth--
      if (depth === 0) { out.push(css.slice(start, i + 1)); start = i + 1 }
    } else if (c === ';' && depth === 0) {
      out.push(css.slice(start, i + 1)); start = i + 1
    }
  }
  if (start < css.length) out.push(css.slice(start))
  return out
}

function checkRules(css, moduleId, file, insideAt = false) {
  const scope = `[data-module="${moduleId}"]`

  for (const chunk of splitTopLevel(css)) {
    const t = chunk.trim()
    if (!t) continue

    if (t.startsWith('@')) {
      if (SELF_SCOPED_AT.test(t)) {
        if (/^@import\s/.test(t) && /tailwindcss/.test(t)) {
          problems.push(`${file}: module stylesheets must not import tailwindcss — the shell issues the single import`)
        }
        continue
      }
      // @media / @container / @supports: recurse into the body
      const body = t.match(/^@[^{]*\{([\s\S]*)\}$/)
      if (body) checkRules(body[1], moduleId, file, true)
      continue
    }

    const m = t.match(/^([^{]+)\{/)
    if (!m) continue

    for (const raw of m[1].split(',')) {
      const sel = raw.trim()
      if (!sel) continue
      if (GLOBAL_OK.test(sel)) continue
      if (sel.startsWith(scope)) continue

      problems.push(
        `${file}: unscoped selector "${sel}"${insideAt ? ' (inside an at-rule)' : ''} — ` +
          `must start with ${scope}, or be :root / [data-theme]`
      )
    }
  }
}

if (!existsSync(MODULES)) {
  console.log('  ok    no modules/ directory')
  process.exit(0)
}

let checked = 0

for (const id of readdirSync(MODULES)) {
  const dir = join(MODULES, id)
  if (!statSync(dir).isDirectory()) continue

  const manifestPath = join(dir, 'module.json')
  if (!existsSync(manifestPath)) continue
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  if (manifest.status !== 'migrated') continue

  // every .css under the module, not just main.css
  const stack = [join(dir, 'app')]
  while (stack.length) {
    const cur = stack.pop()
    if (!existsSync(cur)) continue
    for (const name of readdirSync(cur)) {
      const p = join(cur, name)
      if (statSync(p).isDirectory()) { stack.push(p); continue }
      if (!name.endsWith('.css')) continue
      checkRules(stripComments(readFileSync(p, 'utf8')), id, relative(ROOT, p))
      checked++
    }
  }
}

// ── Tailwind must actually scan the modules ──
//
// The shell issues the single `@import 'tailwindcss'`, but Tailwind v4 detects
// sources relative to its own project — and `modules/` sits outside apps/shell/.
// Without an explicit @source it silently generates ZERO utilities for module
// code. QuantCode builds its whole layout from utilities, so its root collapsed
// from 1144px to 412px and everything stacked. Nothing errors; it just looks
// broken.
const SHELL_CSS = join(ROOT, 'apps', 'shell', 'app', 'assets', 'css', 'shell.css')
if (existsSync(SHELL_CSS)) {
  const css = readFileSync(SHELL_CSS, 'utf8')
  if (/@import\s+['"]tailwindcss['"]/.test(css) && !/@source\s+['"][^'"]*modules['"]/.test(css)) {
    problems.push(
      `apps/shell/app/assets/css/shell.css: imports tailwindcss but has no ` +
        `@source pointing at modules/ — utilities used in module templates will not be generated.`
    )
  }
}

for (const p of problems) console.error(`  ERROR ${p}`)

if (problems.length) {
  console.error(`\n${problems.length} CSS isolation violation(s) — see docs/ARCHITECTURE.md §9`)
  process.exit(1)
}

console.log(`  ok    ${checked} module stylesheet(s) properly scoped`)
