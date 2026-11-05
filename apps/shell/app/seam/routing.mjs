// Where a call goes when this PC uses a server (PLAN-SERVER-CLIENT §6.1–§6.3.3,
// card Server 5/7). Plain JavaScript so `scripts/test-seam-routing.mjs` checks
// the very same table the app uses.
//
//   'server' — through the gateway to the server
//   'local'  — to this PC's Tauri, as without a server
//   'deny'   — refused (secrets, the server's own disk or windows)
//   'both'   — asked of both, results merged (the entity search)
//
// The manifest's placement decides, except for the core's settings and entity
// commands: those carry their module in an argument and go to the server only
// for the server's module data (`manifest.data`).

const DATA = new Set([
  'plugin:qs|get_settings',
  'plugin:qs|get_setting',
  'plugin:qs|set_setting',
  'plugin:qs|upsert_entity',
  'plugin:qs|delete_entity',
  'plugin:qs|list_entities',
  'plugin:qs|count_entities',
  'plugin:qs|search_entities',
  'plugin:qs|link_entities',
  'plugin:qs|unlink_entities',
  'plugin:qs|linked_entities',
])

/** The module of an entity id (`<module>:<kind>:<id>`). */
function moduleOf(id) {
  return typeof id === 'string' ? id.split(':')[0] : null
}

function pluginEntry(manifest, cmd) {
  if (!cmd.startsWith('plugin:')) return undefined
  const plugin = cmd.slice(7).split('|')[0]
  return manifest.plugins?.[plugin] ?? manifest.plugins?.[`core:${plugin}`]
}

function dataRoute(manifest, cmd, a) {
  const data = manifest.data ?? { modules: [], scopes: [] }
  const has = (list, v) => typeof v === 'string' && list.includes(v)
  switch (cmd) {
    case 'plugin:qs|get_settings':
    case 'plugin:qs|get_setting':
    case 'plugin:qs|set_setting':
      return has(data.scopes, a.scope) ? 'server' : 'local'
    case 'plugin:qs|upsert_entity':
      return has(data.modules, a.entity?.module) ? 'server' : 'local'
    case 'plugin:qs|list_entities':
    case 'plugin:qs|count_entities':
      // Without a module: this PC's own entities (the server answers only per module).
      return has(data.modules, a.module) ? 'server' : 'local'
    case 'plugin:qs|search_entities':
      return 'both'
    case 'plugin:qs|delete_entity':
    case 'plugin:qs|linked_entities':
      return has(data.modules, moduleOf(a.id)) ? 'server' : 'local'
    case 'plugin:qs|link_entities':
    case 'plugin:qs|unlink_entities':
      return has(data.modules, moduleOf(a.link?.src)) ? 'server' : 'local'
    default:
      return 'local'
  }
}

/**
 * @param {{ commands: Record<string, string>, plugins?: Record<string, string>, data?: { modules: string[], scopes: string[] } }} manifest
 * @param {string} cmd
 * @param {Record<string, any> | undefined} args
 * @returns {'server' | 'local' | 'deny' | 'both'}
 */
export function route(manifest, cmd, args) {
  if (DATA.has(cmd)) return dataRoute(manifest, cmd, args && typeof args === 'object' ? args : {})
  const entry = manifest.commands[cmd] ?? pluginEntry(manifest, cmd)
  const run = typeof entry === 'string' ? entry.split(':')[0] : null
  if (run === 'server') return 'server'
  if (run === 'deny') return 'deny'
  // `client`, and everything the manifest does not list (Tauri's own
  // plugins: path, image, resources, menu, tray, …), runs here.
  return 'local'
}

/** Results of the entity search, the server's first, without this PC's stale
 * copies of the server's modules. */
export function mergeSearch(manifest, local, remote, limit) {
  const server = new Set(manifest.data?.modules ?? [])
  const mine = (Array.isArray(local) ? local : []).filter((e) => !server.has(e?.module))
  const merged = [...(Array.isArray(remote) ? remote : []), ...mine]
  return typeof limit === 'number' && limit > 0 ? merged.slice(0, limit) : merged
}
