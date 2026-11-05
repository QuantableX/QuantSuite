#!/usr/bin/env node
/**
 * The remote manifest's inventory (PLAN-SERVER-CLIENT §6.2, card Server 1/7).
 *
 * Every Tauri command a plugin registers (its `build.rs` COMMANDS list) must
 * have an entry in its module's `remote` manifest — `module.json → remote →
 * commands` for the modules, `crates/qs-core/remote.json` for the core plugin
 * (`qs`), the app's own commands and the shared Tauri plugins:
 *
 *   "list_habits": "server:read"
 *
 * `run` is where the command executes when a client is connected to a server:
 *   server — the gateway forwards it to the server (the data lives there)
 *   client — it runs on the PC you sit at (screen, files, agent CLIs)
 *   deny   — not offered remotely (it acts on a disk or OS the caller cannot see)
 * `effect` is the side-effect class as for MCP capabilities: read, compute,
 * write, external (orders, processes, the network — step-up PIN, card 3).
 *
 *   node scripts/remote-inventory.mjs           report (exit 1 when an entry is missing or stale)
 *   node scripts/remote-inventory.mjs --init    add a first classification for missing entries
 *   node scripts/remote-inventory.mjs --table   print the per-module totals as a Markdown table
 *   node scripts/remote-inventory.mjs --write   regenerate crates/qs-gateway/remote-manifest.json
 *
 * `--init` never changes an existing entry: review what it adds, then `--write`.
 *
 * The server's gateway authorizes every remote call against
 * `crates/qs-gateway/remote-manifest.json` (card Server 3/7): all entries in
 * one file, keyed by the name a call carries (`plugin:algo|start_bot`, or the
 * bare name of an app command). The report fails when that file is stale.
 */

import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

export const RUNS = ['server', 'client', 'deny']
export const EFFECTS = ['read', 'compute', 'write', 'external']

/** Phase-1 placement of each plugin's module (PLAN-SERVER-CLIENT §3). */
const MODULE_RUN = {
  algo: 'server',
  systems: 'server',
  terminal: 'server',
  script: 'server',
  finance: 'server',
  notes: 'server',
  plan: 'server',
  habit: 'server',
  // No plugin of its own; its settings and entities live with the server's data.
  flow: 'server',
  pilot: 'client',
  canvas: 'client',
  console: 'client',
  mcp: 'client',
  memory: 'client',
  hud: 'client',
}

/** The commands of a plugin, from its build.rs `COMMANDS` list. */
export function buildCommands(buildRs) {
  const src = readFileSync(buildRs, 'utf8')
  const m = src.match(/const COMMANDS:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/)
  if (!m) return []
  return [...m[1].matchAll(/"([a-z0-9_]+)"/g)].map((x) => x[1])
}

/** Every plugin with its commands and the manifest file that covers it. */
export function plugins() {
  const out = [
    { id: 'core', plugin: 'qs', manifest: join(ROOT, 'crates/qs-core/remote.json'), commands: buildCommands(join(ROOT, 'crates/qs-core/build.rs')) },
  ]
  for (const id of readdirSync(join(ROOT, 'modules')).sort()) {
    const buildRs = join(ROOT, 'modules', id, 'crate', 'build.rs')
    if (!existsSync(buildRs)) continue
    out.push({ id, plugin: id, manifest: join(ROOT, 'modules', id, 'module.json'), commands: buildCommands(buildRs) })
  }
  return out
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'))
}

/** The `remote.commands` map of a manifest file. */
export function remoteOf(path) {
  if (!existsSync(path)) return {}
  return readJson(path).remote?.commands ?? {}
}

// ── the first classification (--init) ──────────────────────────────────────

/** Verbs by side-effect class; a command's tokens decide (read verbs first). */
const READ_FIRST = new Set(['get', 'list', 'read', 'load', 'fetch', 'search', 'find', 'query', 'view', 'describe', 'stats', 'status', 'count', 'tail', 'peek', 'is', 'has', 'can', 'lookup', 'resolve', 'info', 'show', 'history', 'logs', 'snapshot', 'export'])
const EXTERNAL_TOKENS = new Set(['place', 'execute', 'spawn', 'launch', 'exec', 'deploy', 'publish', 'push', 'pull', 'install', 'arm', 'kill', 'switch', 'send', 'pty', 'agent', 'agents', 'crew', 'shell', 'command', 'commands', 'run', 'forge', 'gauntlet', 'bot', 'bots', 'positions', 'export_key', 'github', 'pr', 'login', 'browser', 'navigate', 'record', 'dictation', 'speak', 'screenshot', 'capture'])
const WRITE_TOKENS = new Set(['create', 'add', 'update', 'save', 'set', 'delete', 'remove', 'rename', 'move', 'import', 'reorder', 'toggle', 'archive', 'unarchive', 'restore', 'mark', 'apply', 'write', 'upsert', 'insert', 'clear', 'reset', 'dismiss', 'cancel', 'confirm', 'unlock', 'approve', 'reject', 'claim', 'complete', 'merge', 'edit', 'patch', 'attach', 'detach', 'pin', 'unpin', 'append', 'commit', 'stash', 'checkout', 'init', 'register', 'unregister', 'enable', 'disable', 'disarm', 'duplicate', 'copy', 'paste', 'drop', 'upload', 'store', 'put', 'link', 'unlink', 'tag', 'untag', 'assign', 'accept', 'decline', 'snooze', 'retire', 'promote', 'stop', 'start', 'close', 'open', 'resume', 'pause', 'refresh', 'sync', 'rebuild', 'reindex', 'index', 'generate', 'backup', 'prune', 'purge', 'trash', 'cleanup', 'migrate', 'focus', 'hide', 'resize', 'position', 'dock', 'undock', 'notify', 'dispatch', 'emit', 'ack', 'answer', 'reply', 'stage', 'unstage', 'discard', 'revert', 'fork', 'branch', 'swap', 'bind', 'unbind', 'mute', 'unmute', 'grant', 'revoke', 'touch', 'feed', 'consume', 'write_text', 'type'])
const COMPUTE_TOKENS = new Set(['backtest', 'preview', 'compute', 'evaluate', 'eval', 'simulate', 'render', 'test', 'check', 'validate', 'lint', 'parse', 'analyze', 'analyse', 'sweep', 'score', 'estimate', 'diff', 'format', 'tokenize', 'transcribe', 'summarize', 'explain', 'suggest', 'convert', 'measure', 'probe', 'plan', 'mapping', 'coverage', 'compare'])

export function classify(id, cmd) {
  const tokens = cmd.split('_')
  // Strip a leading module word (pilot_, hud_, canvas_ …) before the verb.
  while (tokens.length > 1 && (tokens[0] === id || tokens[0] === 'qs' || tokens[0] === 'suite')) tokens.shift()
  let effect
  if (tokens.some((t) => EXTERNAL_TOKENS.has(t)) && !READ_FIRST.has(tokens[0])) effect = 'external'
  else if (READ_FIRST.has(tokens[0])) effect = 'read'
  else if (tokens.some((t) => WRITE_TOKENS.has(t))) effect = 'write'
  else if (tokens.some((t) => COMPUTE_TOKENS.has(t))) effect = 'compute'
  else effect = 'read'
  return [effect, tokens]
}

export function placement(id, cmd) {
  const [effect] = classify(id, cmd)
  let run = MODULE_RUN[id] ?? 'client'
  // A server module's command that acts on the caller's own disk, windows or
  // editor cannot run for a remote caller: the remote file browser and
  // downloads replace it (card 4).
  if (run === 'server' && /(^|_)(reveal|open_in|open_folder|open_file|show_in|explorer|pick_|dialog|clipboard|open_editor|open_path|open_dir|open_data_dir|open_log)/.test(cmd)) run = 'deny'
  return `${run}:${effect}`
}

// ── the gateway's copy ─────────────────────────────────────────────────────

export const GATEWAY_MANIFEST = join(ROOT, 'crates/qs-gateway/remote-manifest.json')

/** Every entry in one map, keyed by the invoke name a remote call carries. */
export function combinedManifest() {
  const commands = {}
  const events = new Set()
  const topics = new Set()
  for (const p of plugins()) {
    const remote = remoteOf(p.manifest)
    for (const c of p.commands) if (remote[c]) commands[`plugin:${p.plugin}|${c}`] = remote[c]
    if (!existsSync(p.manifest)) continue
    const json = readJson(p.manifest)
    for (const e of json.remote?.events ?? []) events.add(e)
    // The bus topics a server-placed module publishes reach every client (card Server 4/7).
    if (MODULE_RUN[p.id] === 'server') for (const t of json.publishes ?? []) topics.add(typeof t === 'string' ? t : t.topic)
  }
  const core = readJson(join(ROOT, 'crates/qs-core/remote.json'))
  for (const [c, v] of Object.entries(core.app ?? {})) commands[c] = v
  const sorted = Object.fromEntries(Object.keys(commands).sort().map((k) => [k, commands[k]]))
  return {
    $comment: 'Generated by `node scripts/remote-inventory.mjs --write` from every module.json and crates/qs-core/remote.json. Do not edit: the gateway authorizes remote calls against it (PLAN-SERVER-CLIENT §6.2).',
    commands: sorted,
    plugins: core.plugins ?? {},
    events: [...events].sort(),
    topics: [...topics].filter(Boolean).sort(),
    // Core settings and entities of the server-placed modules live on the
    // server (§6.3.3): the client routes `get_setting`/`set_setting` by scope
    // and the entity commands by module; the server refuses any other scope
    // or module (card Server 5/7). `todos` is the shared to-do document.
    data: {
      modules: Object.keys(MODULE_RUN).filter((m) => MODULE_RUN[m] === 'server').sort(),
      scopes: [...Object.keys(MODULE_RUN).filter((m) => MODULE_RUN[m] === 'server'), 'todos'].sort(),
    },
  }
}

export function manifestText() {
  return JSON.stringify(combinedManifest(), null, 2) + '\n'
}

// ── report ─────────────────────────────────────────────────────────────────

/**
 * The developer modules work on the folders and agent CLIs of the PC you sit
 * at (PLAN-SERVER-CLIENT §6.3.10, card Server 7/7): none of their commands
 * runs on the server for another device — QuantPilot, QuantCode/Canvas, the
 * console (no remote shell on the trading machine), QuantMCP with its boards
 * and index, memory, the HUD.
 */
export const LOCAL_ONLY = ['pilot', 'canvas', 'console', 'mcp', 'memory', 'hud']

export function inventory() {
  const rows = []
  const problems = []
  for (const p of plugins()) {
    const remote = remoteOf(p.manifest)
    for (const c of p.commands) {
      if (LOCAL_ONLY.includes(p.id) && String(remote[c] ?? '').startsWith('server:')) {
        problems.push(`${p.id}: ${c} is placed on the server, but ${p.id} stays on each PC (PLAN-SERVER-CLIENT §6.3.10)`)
      }
      const v = remote[c]
      if (!v) {
        problems.push(`${p.id}: ${c} has no remote entry`)
        continue
      }
      const [run, effect] = String(v).split(':')
      if (!RUNS.includes(run) || !EFFECTS.includes(effect)) problems.push(`${p.id}: ${c} = "${v}" is not <server|client|deny>:<read|compute|write|external>`)
      rows.push({ id: p.id, cmd: c, run, effect })
    }
    for (const c of Object.keys(remote)) {
      if (!p.commands.includes(c)) problems.push(`${p.id}: remote entry ${c} is not a registered command`)
    }
  }
  const current = existsSync(GATEWAY_MANIFEST) ? readFileSync(GATEWAY_MANIFEST, 'utf8').replace(/\r\n/g, '\n') : ''
  if (current !== manifestText()) problems.push('crates/qs-gateway/remote-manifest.json is stale: run node scripts/remote-inventory.mjs --write')
  return { rows, problems }
}

/** The `remote` block as text, so the rest of module.json keeps its layout. */
function remoteBlock(commands, eol, indent, extra = {}) {
  const pad = ' '.repeat(indent)
  const lines = Object.entries(commands).map(([c, v]) => `${pad}    ${JSON.stringify(c)}: ${JSON.stringify(v)}`)
  // Other keys of the block (the events a server-placed module emits) stay, first.
  const others = Object.entries(extra).map(([k, v]) => `${pad}  ${JSON.stringify(k)}: ${JSON.stringify(v)},${eol}`).join('')
  return `${pad}"remote": {${eol}${others}${pad}  "commands": {${eol}${lines.join(',' + eol)}${eol}${pad}  }${eol}${pad}}`
}

function init() {
  let added = 0
  for (const p of plugins()) {
    const exists = existsSync(p.manifest)
    const raw = exists ? readFileSync(p.manifest, 'utf8') : '{\n}\n'
    const eol = raw.includes('\r\n') ? '\r\n' : '\n'
    const json = JSON.parse(raw)
    const commands = json.remote?.commands ?? {}
    const next = {}
    for (const c of p.commands) {
      next[c] = commands[c] ?? placement(p.id, c)
      if (!commands[c]) added++
    }
    if (json.remote) {
      // Replace the existing block (generated by this script, same layout).
      const start = raw.indexOf('"remote": {')
      let depth = 0
      let end = start
      for (let i = raw.indexOf('{', start); i < raw.length; i++) {
        if (raw[i] === '{') depth++
        if (raw[i] === '}' && --depth === 0) { end = i + 1; break }
      }
      const indent = start - raw.lastIndexOf('\n', start) - 1
      const { commands: _c, ...extra } = json.remote
      writeFileSync(p.manifest, raw.slice(0, start) + remoteBlock(next, eol, indent, extra).trimStart() + raw.slice(end))
    } else {
      // Append as the last member of the top-level object.
      const close = raw.lastIndexOf('}')
      const before = raw.slice(0, close).replace(/\s+$/, '')
      const sep = before.endsWith('{') ? '' : ','
      writeFileSync(p.manifest, `${before}${sep}${eol}${remoteBlock(next, eol, 2)}${eol}}${eol}`)
    }
  }
  console.log(`added ${added} entries — review them`)
}

function table(rows) {
  const by = new Map()
  for (const r of rows) {
    const k = r.id
    const t = by.get(k) ?? { server: 0, client: 0, deny: 0, read: 0, compute: 0, write: 0, external: 0, total: 0 }
    t[r.run]++
    t[r.effect]++
    t.total++
    by.set(k, t)
  }
  console.log('| Plugin | Commands | server | client | deny | read | compute | write | external |')
  console.log('|---|---|---|---|---|---|---|---|---|')
  const sum = { total: 0, server: 0, client: 0, deny: 0, read: 0, compute: 0, write: 0, external: 0 }
  for (const [k, t] of [...by].sort((a, b) => b[1].total - a[1].total)) {
    console.log(`| ${k} | ${t.total} | ${t.server} | ${t.client} | ${t.deny} | ${t.read} | ${t.compute} | ${t.write} | ${t.external} |`)
    for (const f of Object.keys(sum)) sum[f] += t[f]
  }
  console.log(`| **all** | **${sum.total}** | ${sum.server} | ${sum.client} | ${sum.deny} | ${sum.read} | ${sum.compute} | ${sum.write} | ${sum.external} |`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  if (process.argv.includes('--init')) init()
  if (process.argv.includes('--write')) writeFileSync(GATEWAY_MANIFEST, manifestText())
  const { rows, problems } = inventory()
  if (process.argv.includes('--table')) table(rows)
  for (const p of problems) console.error(`  ✗ ${p}`)
  if (problems.length) {
    console.error(`remote manifest: ${problems.length} problem(s)`)
    process.exit(1)
  }
  console.log(`  ok    ${rows.length} command(s) placed in the remote manifest`)
}
