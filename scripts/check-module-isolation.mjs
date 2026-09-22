#!/usr/bin/env node
/**
 * Guard against the silent cross-module collisions that Nuxt layers and Pinia
 * make easy (docs/ARCHITECTURE.md §1 "Layer gotchas", §14 conventions).
 *
 * Every one of these fails *quietly*: the wrong store, the wrong component or
 * the wrong composable is used, and nothing errors.
 *
 *   1. Pinia store ids must be `<module>/<store>`. A bare `defineStore('app')`
 *      in two modules means the second silently gets the first module's state.
 *   2. Composables must not be relied on by auto-import across layers — a module
 *      using one of its own composables must import it explicitly.
 *   3. Component directories must be declared with a per-module `prefix` in the
 *      layer config, or same-named components collide.
 *   4. Modules must not use viewport units outside fixed-position overlays.
 *
 * Run via `npm run check`.
 */

import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs'
import { join, relative, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const MODULES = join(ROOT, 'modules')

const problems = []

/**
 * Remove comments before scanning.
 *
 * Without this the guard reports its own documentation: a comment explaining why
 * `min-h-screen` was removed contains the string `min-h-screen`, and one
 * explaining the `<slot>` rule contains `<NuxtPage`.
 */
function stripComments(text) {
  return text
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1')
}

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
  .filter((d) => statSync(join(MODULES, d)).isDirectory())
  .filter((d) => existsSync(join(MODULES, d, 'module.json')))
  .filter((d) => JSON.parse(readFileSync(join(MODULES, d, 'module.json'), 'utf8')).status === 'migrated')

// ── 0. every window a module creates must be listed in the Tauri capability ──
//
// A window label missing from `capabilities/default.json` is the worst kind of
// silent failure: the window opens, renders and looks correct, and every single
// `invoke` inside it is denied — per call, at runtime, with nothing at build
// time. Modules render inside `main`, but QuantHUD builds five windows of its
// own, and each label has to be listed.
{
  const capPath = join(ROOT, 'apps', 'src-tauri', 'capabilities', 'default.json')
  if (existsSync(capPath)) {
    const listed = new Set(JSON.parse(readFileSync(capPath, 'utf8')).windows ?? [])
    for (const id of migrated) {
      const crateSrc = join(MODULES, id, 'crate', 'src')
      if (!existsSync(crateSrc)) continue
      for (const f of walk(crateSrc).filter((p) => p.endsWith('.rs'))) {
        const text = readFileSync(f, 'utf8')
        for (const m of text.matchAll(/WebviewWindowBuilder::new\(\s*&?\w+\s*,\s*(?:"([^"]+)"|(\w+))/g)) {
          const label = m[1]
          // Labels held in a constant are resolved below, not here.
          if (!label) continue
          if (!listed.has(label)) {
            problems.push(
              `apps/src-tauri/capabilities/default.json: window "${label}" (created in ` +
                `${relative(ROOT, f)}) is not listed — every invoke in it would be denied.`
            )
          }
        }
      }
    }
  }
}

// ── 0b. window-creating commands must be async ──
//
// A synchronous `#[tauri::command]` runs on the main thread. Calling
// `WebviewWindowBuilder::build()` there deadlocks: build() hands the event loop
// a task and waits for it, while the event loop waits for the command to
// return. The app freezes — no error, no panic, no log. Async commands run on
// the async runtime, so the event loop stays free to service the build.
{
  const rustRoots = [join(ROOT, 'crates'), MODULES]
  for (const root of rustRoots) {
    if (!existsSync(root)) continue
    for (const f of walk(root).filter((p) => p.endsWith('.rs') && !p.includes('target'))) {
      // Comments must go first: a doc comment explaining this very rule sits
      // above the *next* command, so it lands in the previous command's chunk
      // and frames an innocent function as a window builder.
      const text = stripComments(readFileSync(f, 'utf8'))
      if (!text.includes('WebviewWindowBuilder')) continue

      // Split on the attribute so each chunk is one command's signature+body.
      const chunks = text.split('#[tauri::command]').slice(1)
      for (const chunk of chunks) {
        const sig = chunk.match(/^\s*(?:pub\s+)?(async\s+)?fn\s+([a-z_][a-z0-9_]*)/)
        if (!sig) continue
        if (!chunk.includes('WebviewWindowBuilder')) continue
        if (!sig[1]) {
          problems.push(
            `${relative(ROOT, f)}: command \`${sig[2]}\` builds a window but is not async — ` +
              `a sync command runs on the main thread and deadlocks the event loop.`
          )
        }
      }
    }
  }
}

// ── 0c. commands that block must be async ──
//
// Same root cause as 0b, one step wider. Tauri's `#[tauri::command]` defaults
// to `ExecutionContext::Blocking` (tauri-macros/src/command/wrapper.rs): a
// command that is not `async fn` runs **on the main thread**, inline in the IPC
// handler. So `docker info`, a git spawn, an HTTP call or a PTY write inside a
// sync command is not slow — it is a frozen window, and Windows logs
// `AppHangB1` / "Keine Rückmeldung" for it. Four of those are in the event log
// for quantsuite.exe 0.1.0.0.
//
// Marking a command `async fn` moves it to the async runtime; the event loop
// stays free to pump window messages.
//
// The ratchet is the migration state, not an exemption: a file listed there
// still has sync commands doing blocking work, and its number may only go
// down. It is **empty** — every command in the suite that blocks is async, and
// the guard fails the moment one is added back.
{
  // Only genuinely blocking operations — an in-memory state read is fine on
  // the main thread and must not be flagged, or the rule gets ignored.
  const BLOCKING = [
    [/\bCommand::new\b/, 'spawns a process'],
    [/\breqwest::/, 'makes an HTTP request'],
    [/\bfs::(read|write|copy|rename|remove|create_dir|read_dir|read_to_string)/, 'does filesystem I/O'],
    [/\bFile::(open|create)\b/, 'does filesystem I/O'],
    [/\bthread::sleep\b/, 'sleeps'],
    [/\.recv_timeout\(|\.recv\(\)/, 'blocks on a channel'],
    [/\.write_all\(/, 'writes to a pipe or file'],
  ]

  // Every `fn name(…) { … }` in a file, body matched by brace depth.
  //
  // Bodies are needed, not just the command's own text: most commands delegate
  // (`db::save_page(&conn, …)`, `write_config(…)`), so scanning only the
  // command body catches the loud cases and misses the quiet ones. Resolution
  // is **crate-wide**, not per file — QuantMCP's commands live in lib.rs and do
  // all their real work in mcp.rs / process.rs / projects.rs, so a per-file
  // scan reported that module as clean while ~50 of its commands blocked.
  function collectBodies(text, into) {
    for (const m of text.matchAll(/\bfn\s+([a-z_][a-z0-9_]*)\s*(?:<[^>]*>)?\s*\(/g)) {
      const open = text.indexOf('{', m.index)
      if (open === -1) continue
      let depth = 0
      let end = open
      for (; end < text.length; end++) {
        if (text[end] === '{') depth++
        else if (text[end] === '}' && --depth === 0) break
      }
      // First definition wins; a same-named fn in two files of one crate is
      // rare enough that the reason line stays truthful either way.
      if (!into.has(m[1])) into.set(m[1], text.slice(open, end + 1))
    }
    return into
  }

  /** The first blocking operation reachable from `name`, following local calls. */
  function blockingReason(name, bodies, seen = new Set()) {
    if (seen.has(name)) return null
    seen.add(name)
    const body = bodies.get(name)
    if (!body) return null

    const direct = BLOCKING.find(([re]) => re.test(body))
    if (direct) return direct[1]

    for (const call of body.matchAll(/\b([a-z_][a-z0-9_]*)\s*\(/g)) {
      if (call[1] === name || !bodies.has(call[1])) continue
      const reason = blockingReason(call[1], bodies, seen)
      if (reason) return reason
    }
    return null
  }

  // file -> number of sync blocking commands still present. Only ever lower.
  //
  // Not a whitelist of "fine as they are" — an entry is work still owed. Empty
  // is the goal state and the current state: 128 commands were converted, so
  // adding a file here again means a regression was accepted, not excused.
  const RATCHET = {}

  const counts = {}
  const rustRoots = [join(ROOT, 'crates'), MODULES]
  const sources = rustRoots
    .filter(existsSync)
    .flatMap((root) => walk(root).filter((p) => p.endsWith('.rs') && !p.includes('target')))

  // Group by crate — everything under one `…/src/` tree resolves together.
  const crates = new Map()
  for (const f of sources) {
    const key = f.replace(/\\/g, '/').replace(/\/src\/.*$/, '')
    if (!crates.has(key)) crates.set(key, [])
    crates.get(key).push(f)
  }

  for (const files of crates.values()) {
    const bodies = new Map()
    for (const f of files) collectBodies(stripComments(readFileSync(f, 'utf8')), bodies)

    for (const f of files) {
      const text = stripComments(readFileSync(f, 'utf8'))
      if (!text.includes('#[tauri::command]')) continue
      const rel = relative(ROOT, f).replace(/\\/g, '/')

      for (const chunk of text.split('#[tauri::command]').slice(1)) {
        const sig = chunk.match(/^\s*(?:pub(?:\([^)]*\))?\s+)?(async\s+)?fn\s+([a-z_][a-z0-9_]*)/)
        if (!sig || sig[1]) continue // no signature, or already async

        const reason = blockingReason(sig[2], bodies)
        if (!reason) continue

        counts[rel] = (counts[rel] ?? 0) + 1
        if (counts[rel] > (RATCHET[rel] ?? 0)) {
          problems.push(
            `${rel}: command \`${sig[2]}\` ${reason} but is not async — a sync command ` +
              `runs on the main thread and freezes the window ("Keine Rückmeldung").`
          )
        }
      }
    }
  }

  for (const [file, allowed] of Object.entries(RATCHET)) {
    const actual = counts[file] ?? 0
    if (actual < allowed) {
      problems.push(
        `scripts/check-module-isolation.mjs: ${file} is down to ${actual} sync blocking ` +
          `command(s) — lower its ratchet entry from ${allowed} to ${actual} so it cannot drift back.`
      )
    }
  }
}

const seenStoreIds = new Map()

for (const id of migrated) {
  const dir = join(MODULES, id)
  const sources = walk(dir).filter((f) => /\.(vue|ts)$/.test(f) && !f.includes(`${'node_modules'}`))

  // ── 1. Pinia store ids ──
  for (const f of sources) {
    const text = readFileSync(f, 'utf8')
    for (const m of text.matchAll(/defineStore\(\s*['"]([^'"]+)['"]/g)) {
      const storeId = m[1]
      if (!storeId.startsWith(`${id}/`)) {
        problems.push(
          `${relative(ROOT, f)}: store id "${storeId}" is not namespaced — must be "${id}/${storeId}". ` +
            `Two modules sharing an id silently share one store.`
        )
      }
      const prev = seenStoreIds.get(storeId)
      if (prev && prev !== id) {
        problems.push(`store id "${storeId}" defined in both ${prev} and ${id}`)
      }
      seenStoreIds.set(storeId, id)
    }
  }

  // ── 2. composables must be imported explicitly ──
  const composablesDirs = [join(dir, 'app', 'composables'), join(dir, 'composables')].filter(existsSync)
  const composables = composablesDirs.flatMap((d) =>
    readdirSync(d)
      .filter((f) => /\.(ts|js)$/.test(f))
      .map((f) => f.replace(/\.(ts|js)$/, ''))
  )

  for (const f of sources) {
    if (composablesDirs.some((d) => f.startsWith(d))) continue
    const text = readFileSync(f, 'utf8')
    for (const name of composables) {
      const used = new RegExp(`\\b${name}\\s*\\(`).test(text)
      const imported = new RegExp(`import\\s*\\{[^}]*\\b${name}\\b[^}]*\\}`).test(text)
      if (used && !imported) {
        problems.push(
          `${relative(ROOT, f)}: uses "${name}" via auto-import — add ` +
            `\`import { ${name} } from '#${id}/composables/${name}'\`. ` +
            `Auto-imported composables are global across layers; another module's version may win.`
        )
      }
    }
  }

  // ── 3. component prefix declared ──
  const hasComponents = existsSync(join(dir, 'app', 'components'))
  if (hasComponents) {
    const cfgPath = join(dir, 'nuxt.config.ts')
    const cfg = existsSync(cfgPath) ? readFileSync(cfgPath, 'utf8') : ''
    if (!/components:\s*\[\s*\{[^}]*prefix:/.test(cfg)) {
      problems.push(
        `modules/${id}/nuxt.config.ts: no component prefix declared — same-named ` +
          `components in another module would silently win.`
      )
    }
  }

  // ── 4. viewport units, in CSS and as Tailwind utilities ──
  //
  // `min-h-screen` / `h-screen` are `100vh` wearing a utility class. An earlier
  // version of this guard only looked for the literal, and QuantCode shipped a
  // `min-h-screen` layout root that forced the module past its container.
  const VIEWPORT_UTILITY = /\b(?:min-|max-)?[hw]-screen\b/
  for (const f of sources.filter((f) => /\.vue$/.test(f))) {
    const text = stripComments(readFileSync(f, 'utf8'))
    const hasLiteral = /100v[wh]/.test(text)
    const hasUtility = VIEWPORT_UTILITY.test(text)
    if (!hasLiteral && !hasUtility) continue
    if (/position:\s*fixed/.test(text)) continue // legitimate for overlays

    for (const line of text.split('\n')) {
      const t = line.trim()
      if (t.startsWith('/*') || t.startsWith('*') || t.startsWith('<!--')) continue
      if (/100v[wh]/.test(line) || VIEWPORT_UTILITY.test(line)) {
        problems.push(
          `${relative(ROOT, f)}: "${t}" — a module fills its container, not the viewport ` +
            `(fixed-position overlays are the exception).`
        )
      }
    }
  }

  // ── 5. invoke() must name the plugin ──
  //
  // Module commands live behind `plugin:<id>|`. A bare name fails at runtime
  // with "command not found" — per call, never at build time, so the module
  // just looks dead. 282 call sites were bare after the first six migrations.
  const buildRs = join(dir, 'crate', 'build.rs')
  if (existsSync(buildRs)) {
    const block = readFileSync(buildRs, 'utf8').match(/const COMMANDS:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/)
    const commands = new Set(block ? [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]) : [])

    for (const f of sources) {
      const text = readFileSync(f, 'utf8')
      for (const m of text.matchAll(/\(\s*(['"])([a-z_][a-z0-9_]*)\1/g)) {
        if (commands.has(m[2])) {
          problems.push(
            `${relative(ROOT, f)}: '${m[2]}' is called without its plugin namespace — ` +
              `use 'plugin:${id}|${m[2]}'.`
          )
        }
      }
    }
  }

  // ── 6. window routes opened from Rust must exist ──
  //
  // A module's pages are namespaced under `/<id>` so Nuxt layers cannot collide
  // on merge. A crate that opens `WebviewUrl::App("/region-selector")` builds a
  // window that loads nothing: it appears, stays blank, and reports no error.
  // QuantHUD shipped three of these.
  const crateSrc = join(dir, 'crate', 'src')
  if (existsSync(crateSrc)) {
    const pagesDir = join(dir, 'app', 'pages')
    const pages = existsSync(pagesDir)
      ? new Set(
          walk(pagesDir)
            .filter((f) => f.endsWith('.vue'))
            .map((f) => '/' + relative(pagesDir, f).replace(/\\/g, '/').replace(/\.vue$/, '').replace(/\/index$/, ''))
        )
      : new Set()

    for (const f of walk(crateSrc).filter((f) => f.endsWith('.rs'))) {
      const text = readFileSync(f, 'utf8')
      if (!text.includes('WebviewUrl::App')) continue

      // Two forms: the literal `WebviewUrl::App("/x")`, and a route built with
      // `format!("/x?…")` and passed in as a variable — QuantHUD's colour
      // picker does the latter to hand the overlay its monitor offsets.
      const literals = [...text.matchAll(/WebviewUrl::App\(\s*"([^"]*)"/g)].map((m) => m[1])
      const formatted = [...text.matchAll(/format!\(\s*"(\/[^"]*)"/g)].map((m) => m[1])

      for (const raw of [...literals, ...formatted]) {
        const route = raw.split('?')[0].replace(/\/$/, '')
        if (!route.startsWith('/')) continue
        if (!route.startsWith(`/${id}`)) {
          problems.push(
            `${relative(ROOT, f)}: opens a window at "${route}", outside /${id} — ` +
              `module pages are namespaced, so this window loads a blank page.`
          )
        } else if (pages.size && !pages.has(route)) {
          problems.push(
            `${relative(ROOT, f)}: opens a window at "${route}", but ${id} has no such page.`
          )
        }
      }
    }
  }

  // ── 7. modules must not draw window chrome (PLAN-V2 §2) ──
  //
  // v2: the shell owns the one titlebar, the rail and the drawer. A module
  // that renders window controls, a drag region or a Home button reintroduces
  // exactly the per-module chrome the shell reset removed. QuantHUD's own
  // overlay windows are the one exception — it owns them outright.
  if (id !== 'hud') {
    const CHROME = [
      ['<QWindowControls', 'renders window controls — only the shell titlebar carries them'],
      ['v-drag-window', 'declares a window drag region — only the shell titlebar drags the window'],
      ['data-tauri-drag-region', 'declares a window drag region — only the shell titlebar drags the window'],
      ['Home — all apps', 'draws a Home button — module switching lives on the rail'],
    ]
    for (const f of sources.filter((s) => s.endsWith('.vue'))) {
      const text = stripComments(readFileSync(f, 'utf8'))
      for (const [needle, why] of CHROME) {
        if (text.includes(needle)) {
          problems.push(`${relative(ROOT, f)}: ${why} (PLAN-V2 §2).`)
        }
      }
    }
  }

  // ── 8. layouts must yield with <slot>, not <NuxtPage> ──
  //
  // The shell already renders `<NuxtLayout><NuxtPage/></NuxtLayout>`. A layout
  // that renders NuxtPage itself nests a second page inside the layout.
  const layoutPath = join(dir, 'app', 'layouts', `${id}.vue`)
  if (existsSync(layoutPath)) {
    const layout = stripComments(readFileSync(layoutPath, 'utf8'))
    if (/<NuxtPage\b/.test(layout)) {
      problems.push(
        `modules/${id}/app/layouts/${id}.vue: renders <NuxtPage> — a layout must yield with <slot>.`
      )
    }
    if (!/<slot\b/.test(layout)) {
      problems.push(`modules/${id}/app/layouts/${id}.vue: has no <slot> — its page will never render.`)
    }
  }
}

for (const p of problems) console.error(`  ERROR ${p}`)

if (problems.length) {
  console.error(`\n${problems.length} module isolation violation(s) — see docs/ARCHITECTURE.md §1`)
  process.exit(1)
}

console.log(`  ok    ${migrated.length} module(s) isolated (stores, composables, components, viewport)`)
