#!/usr/bin/env node
/**
 * Validates every modules/<id>/module.json against the contract in
 * docs/ARCHITECTURE.md §1, and generates modules/registry.generated.json —
 * the single source both the shell (rail, home, palette) and any tooling read.
 *
 * Run via `npm run check:modules`.
 */

import { readdirSync, readFileSync, writeFileSync, statSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const MODULES_DIR = join(ROOT, 'modules')

/** Topics published by qs-core itself — see crates/qs-core. */
const CORE_TOPICS = [
  'core.theme.changed',
  'core.module.focused',
  'core.entity.upserted',
  'core.entity.deleted',
  'core.setting.changed',
  'core.window.shown',
  'core.window.hidden',
  'core.process.panicked',
  // Published by the shell when the picker opens a project folder (E2).
  'core.workspace.opened',
  // Published by the shell palette's file results (E5).
  'core.file.open',
  // Published by the agent-call broker in qs-core (E4).
  'agent.call.requested',
  'agent.call.completed',
]

const REQUIRED = ['id', 'title', 'description', 'status', 'routes']
const STATUSES = ['planned', 'in_progress', 'migrated', 'stub']
const TOPIC_RE = /^[a-z0-9_]+(\.[a-z0-9_]+){2,}$/

/**
 * V3: the rail lists apps, not modules. Every module belongs to exactly one
 * app from modules/apps.json (`app` + `appOrder`); the one exception is a
 * module with `ownWindow`, which lives outside the suite window entirely.
 */
const APPS_FILE = join(MODULES_DIR, 'apps.json')
if (!existsSync(APPS_FILE)) {
  console.error('modules/apps.json does not exist')
  process.exit(1)
}
const APPS = JSON.parse(readFileSync(APPS_FILE, 'utf8')).apps
const APP_IDS = new Set(APPS.map((a) => a.id))

const errors = []
const warnings = []

function fail(mod, msg) {
  errors.push(`${mod}: ${msg}`)
}
function warn(mod, msg) {
  warnings.push(`${mod}: ${msg}`)
}

for (const app of APPS) {
  if (app.defaultEnabled !== undefined && typeof app.defaultEnabled !== 'boolean') {
    fail(app.id, 'defaultEnabled must be a boolean')
  }
}

if (!existsSync(MODULES_DIR)) {
  console.error('modules/ does not exist')
  process.exit(1)
}

const dirs = readdirSync(MODULES_DIR).filter((d) => statSync(join(MODULES_DIR, d)).isDirectory())

const modules = []
for (const dir of dirs) {
  const file = join(MODULES_DIR, dir, 'module.json')
  if (!existsSync(file)) {
    fail(dir, 'has no module.json')
    continue
  }

  let m
  try {
    m = JSON.parse(readFileSync(file, 'utf8'))
  } catch (e) {
    fail(dir, `module.json is not valid JSON — ${e.message}`)
    continue
  }

  for (const key of REQUIRED) {
    if (m[key] === undefined) fail(dir, `missing required field "${key}"`)
  }

  if (m.id !== dir) fail(dir, `id "${m.id}" does not match its directory name`)
  if (m.status && !STATUSES.includes(m.status))
    fail(dir, `status "${m.status}" must be one of ${STATUSES.join(', ')}`)

  // Routes must be namespaced, or Nuxt layers collide on merge (§1).
  const root = m.routes?.root
  if (root && root !== `/${m.id}`)
    fail(dir, `routes.root must be "/${m.id}", got "${root}"`)
  if (m.routes?.home && !m.routes.home.startsWith(`/${m.id}`))
    fail(dir, `routes.home "${m.routes.home}" must live under /${m.id}`)

  for (const t of m.publishes ?? []) {
    if (!TOPIC_RE.test(t)) fail(dir, `publishes invalid topic "${t}" — expected <domain>.<entity>.<verb>, lowercase`)
    if (!t.startsWith(`${m.id}.`) && !t.startsWith('market.'))
      warn(dir, `publishes "${t}" outside its own namespace`)
  }
  for (const t of m.subscribes ?? []) {
    if (!TOPIC_RE.test(t)) fail(dir, `subscribes to invalid topic "${t}"`)
  }

  // Modules render inside the single suite window (ARCHITECTURE.md §10). The
  // one exception declares `ownWindow`: QuantHUD is an always-on-top overlay,
  // which is not something that can live inside another window.
  if (m.window)
    fail(dir, 'declares a "window" block — modules render in the suite window, not their own')

  // v2 (PLAN-V2 §2): the shell owns the window chrome — one titlebar, the
  // rail, the drawer. Modules render content only, so per-module chrome
  // declarations are not merely unused, they are a design violation.
  if (m.chrome)
    fail(dir, 'declares a "chrome" block — the shell owns all window chrome (PLAN-V2 §2)')

  // V3: the rail is app-level. Module ordering lives in `appOrder`.
  if (m.rail)
    fail(dir, 'declares a "rail" block — the rail lists apps since V3; declare "app" + "appOrder" instead')

  if (m.ownWindow) {
    if (m.app) fail(dir, 'declares both "ownWindow" and "app" — an own-window module lives outside the app structure')
  } else {
    if (!m.app) fail(dir, 'missing required field "app" (see modules/apps.json)')
    else if (m.app === 'dashboard') fail(dir, '"app" may not be "dashboard" — the dashboard owns no modules')
    else if (!APP_IDS.has(m.app)) fail(dir, `"app" is "${m.app}", not one of ${[...APP_IDS].join(', ')}`)
    if (typeof m.appOrder !== 'number') fail(dir, 'missing required field "appOrder" (position inside its app)')
  }

  for (const c of m.capabilities ?? []) {
    if (!c.name || !c.command) fail(dir, `capability needs both "name" and "command"`)
    if (c.command && m.plugin && !c.command.startsWith(`plugin:${m.plugin}|`))
      fail(dir, `capability "${c.name}" command must start with "plugin:${m.plugin}|"`)
    if (c.sideEffects && !['read', 'compute', 'write', 'external'].includes(c.sideEffects))
      fail(dir, `capability "${c.name}" has unknown sideEffects "${c.sideEffects}"`)
    // Optional JSON Schema for the tool's arguments. Without one, the MCP
    // bridge serves a permissive schema and the description carries the
    // whole contract.
    if (c.inputSchema !== undefined) {
      if (typeof c.inputSchema !== 'object' || c.inputSchema === null || Array.isArray(c.inputSchema))
        fail(dir, `capability "${c.name}" inputSchema must be a JSON Schema object`)
      else if (c.inputSchema.type !== 'object')
        fail(dir, `capability "${c.name}" inputSchema must have "type": "object" (tool arguments are named)`)
    }
  }

  modules.push(m)
}

// Every subscribed topic needs a publisher, or the wiring silently never fires.
const published = new Set([...CORE_TOPICS, ...modules.flatMap((m) => m.publishes ?? [])])
for (const m of modules) {
  for (const t of m.subscribes ?? []) {
    if (!published.has(t)) warn(m.id, `subscribes to "${t}" which no module publishes`)
  }
}

// appOrder must be unique within an app, otherwise ordering is non-deterministic.
const seenPerApp = new Map()
for (const m of modules) {
  if (m.mergedInto) {
    const target = modules.find((entry) => entry.id === m.mergedInto)
    if (!target || target.mergedInto || target.status !== 'migrated' || target.app !== m.app)
      fail(m.id, 'mergedInto must name an active module in the same app')
    continue
  }
  if (!m.app || m.appOrder === undefined) continue
  const key = `${m.app}:${m.appOrder}`
  if (seenPerApp.has(key)) fail(m.id, `appOrder ${m.appOrder} in app "${m.app}" already used by "${seenPerApp.get(key)}"`)
  else seenPerApp.set(key, m.id)
}

for (const w of warnings) console.warn(`  warn  ${w}`)
for (const e of errors) console.error(`  ERROR ${e}`)

if (errors.length) {
  console.error(`\n${errors.length} error(s) in module manifests`)
  process.exit(1)
}

// Global order: apps in their declared order, modules by appOrder inside each;
// own-window modules (no app) sort last.
const appIndex = new Map(APPS.map((a, i) => [a.id, i]))
const globalOrder = (m) =>
  m.app ? (appIndex.get(m.app) ?? 99) * 100 + (m.appOrder ?? 99) : 9999
modules.sort((a, b) => globalOrder(a) - globalOrder(b))

const registry = {
  generated: 'by scripts/validate-modules.mjs — do not edit',
  apps: APPS.map((a) => ({
    id: a.id,
    title: a.title,
    order: a.order,
    route: a.route ?? null,
    logo: a.logo,
    defaultEnabled: a.defaultEnabled ?? true,
    modules: modules
      .filter((m) => m.app === a.id && !m.mergedInto)
      .sort((x, y) => x.appOrder - y.appOrder)
      .map((m) => m.id),
  })),
  modules: modules.filter((m) => !m.mergedInto).map((m) => ({
    id: m.id,
    title: m.title,
    description: m.description,
    status: m.status,
    phase: m.phase ?? null,
    app: m.app ?? null,
    appOrder: m.appOrder ?? null,
    route: m.routes?.home ?? m.routes?.root ?? `/${m.id}`,
    order: globalOrder(m),
    plugin: m.plugin ?? null,
    // Set only by a module that cannot live inside the suite window —
    // QuantHUD's always-on-top overlay. Selecting it invokes this command
    // instead of navigating the suite window.
    ownWindow: m.ownWindow ?? null,
  })),
  // Flattened for the MCP bridge: one entry per agent-callable capability,
  // named `quantsuite.<module>.<name>` (ARCHITECTURE.md §7).
  capabilities: modules
    .filter((m) => m.status === 'migrated')
    .flatMap((m) =>
      (m.capabilities ?? []).map((c) => ({
        tool: `quantsuite.${m.mergedInto ?? m.id}.${c.name}`,
        module: m.mergedInto ?? m.id,
        name: c.name,
        command: c.command,
        description: c.description ?? '',
        sideEffects: c.sideEffects ?? 'write',
        ...(c.inputSchema !== undefined ? { inputSchema: c.inputSchema } : {}),
      }))
    ),
}

writeFileSync(join(MODULES_DIR, 'registry.generated.json'), JSON.stringify(registry, null, 2) + '\n')

const migrated = modules.filter((m) => m.status === 'migrated').length
console.log(
  `  ok    ${modules.length} module manifests valid — ${migrated} migrated, ${modules.length - migrated} planned` +
    (warnings.length ? ` (${warnings.length} warning(s))` : '')
)
