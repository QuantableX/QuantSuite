<script setup lang="ts">
import { bus, inActiveKeepAliveTree } from '@quantsuite/core'
import { useMcps } from '#mcp/composables/useMcps'
import { GENERAL_WORKSPACE_ID, useMcpWorkspaces } from '#mcp/composables/useMcpWorkspaces'
import { useTools } from '#mcp/composables/useTools'

const route = useRoute()
const router = useRouter()

// V3: theming is suite-owned (dark, monochrome, painted by theme-bridge);
// the module's own useTheme init is gone with its per-module copies.
const { mcps, refresh: refreshMcps } = useMcps()
// The header chip counts what QuantMCP actually serves (built-in groups,
// bridge catalogue, scripted tools) — `tools` alone is just the scripted
// registry, which is empty without a native manifest or user scripts.
const { customTools, refresh: refreshTools, quantmcpToolCount, enabledQuantmcpToolCount } = useTools()
// The suite workspace registry replaced the module's project list — order is
// pinned-first-then-recency (the one display order), so the old reorder
// drag-and-drop is gone with the registry it wrote to.
const {
  workspaces,
  selectedWorkspaceId,
  loading: workspacesLoading,
  refresh: refreshWorkspaces,
  subscribe: subscribeWorkspaces,
  selectWorkspace,
  addFolder,
} = useMcpWorkspaces()

const quantmcpEnabled = useState<boolean>('quantmcp-enabled', () => true)

const isProjectsRoute = computed(() => route.path.startsWith('/mcp/projects'))
const isDashboardRoute = computed(() => route.path === '/mcp')
const totalExternalMcps = computed(() => mcps.value.length)
const activeExternalMcps = computed(() => mcps.value.filter((m) => m.enabled).length)
const totalExternalTools = computed(() => customTools.value.length)
const activeExternalTools = computed(() => customTools.value.filter((t) => t.enabled).length)

// ── Connected AIs (PLAN-QUANTMCP-CONNECT §3.2) ──
// The backend tracks each installation; the sidebar groups Codex and Claude
// by product. Keep the member records for version/config details and Forget.
interface ClientRow {
  id: string
  name: string
  spec_id: string | null
  online: boolean
  sessions: number
  connected_since: number | null
  last_seen: number | null
  last_version: string | null
  installed: boolean
  installed_to: string[]
  install_source: string | null
}

const clients = ref<ClientRow[]>([])
const clientGroups = computed(() => {
  const groups = new Map<string, { id: string; name: string; members: ClientRow[] }>()
  for (const client of clients.value) {
    let id = client.id
    let name = client.name
    if (client.spec_id === 'claude-code' || client.spec_id === 'claude-desktop') {
      id = 'product:claude'
      name = 'Claude'
    } else if (client.spec_id === 'codex-cli') {
      id = 'product:codex'
      name = 'Codex'
    }
    const group = groups.get(id) ?? { id, name, members: [] }
    group.members.push(client)
    groups.set(id, group)
  }
  return [...groups.values()].map((group) => {
    const live = group.members.filter((c) => c.online)
    const since = live.flatMap((c) => c.connected_since === null ? [] : [c.connected_since])
    const seen = group.members.flatMap((c) => c.last_seen === null ? [] : [c.last_seen])
    const versions = new Set(live.map((c) => c.last_version))
    return {
      ...group,
      online: live.length > 0,
      sessions: live.reduce((sum, c) => sum + c.sessions, 0),
      connected_since: since.length ? Math.min(...since) : null,
      last_seen: seen.length ? Math.max(...seen) : null,
      last_version: versions.size === 1 ? live[0]!.last_version : null,
      installed: group.members.some((c) => c.installed),
    }
  }).sort((a, b) => Number(b.online) - Number(a.online)
    || (b.last_seen ?? 0) - (a.last_seen ?? 0)
    || a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
})
type ClientGroup = (typeof clientGroups.value)[number]

const clientsLoaded = ref(false)
let clientsInterval: ReturnType<typeof setInterval> | null = null
let offClientEvents: Array<() => void> = []

async function refreshClients() {
  try {
    if (!import.meta.client || !window.__TAURI_INTERNALS__) return
    const { invoke } = await import('@tauri-apps/api/core')
    clients.value = await invoke<ClientRow[]>('plugin:mcp|list_clients')
    clientsLoaded.value = true
  } catch (e) {
    console.error('Failed to load clients:', e)
  }
}

// Once per visit: a client whose config already lists QuantMCP — installed
// by hand or by an earlier version — lands in the registry as `detected`.
async function scanClients() {
  try {
    if (!import.meta.client || !window.__TAURI_INTERNALS__) return
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('plugin:mcp|scan_clients')
  } catch (e) {
    console.error('Failed to scan clients:', e)
  }
}

async function forgetClient(group: ClientGroup) {
  try {
    if (!import.meta.client || !window.__TAURI_INTERNALS__) return
    const { invoke } = await import('@tauri-apps/api/core')
    for (const client of group.members) {
      await invoke('plugin:mcp|forget_client', { id: client.id })
    }
  } catch (e) {
    console.error('Failed to forget client:', e)
  } finally {
    await refreshClients()
  }
}

function relativeTime(ms: number): string {
  const s = Math.round((Date.now() - ms) / 1000)
  if (s < 60) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} min ago`
  const h = Math.round(m / 60)
  if (h < 24) return `${h} h ago`
  const d = Math.round(h / 24)
  return d === 1 ? 'yesterday' : `${d} d ago`
}

function clockTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function clientSubline(c: ClientRow | ClientGroup): string {
  if (c.online) {
    const parts = [`${c.sessions} session${c.sessions === 1 ? '' : 's'}`]
    if (c.connected_since) parts.push(`since ${clockTime(c.connected_since)}`)
    if (c.last_version) parts.push(`v${c.last_version}`)
    return parts.join(' · ')
  }
  if (c.last_seen) return `last seen ${relativeTime(c.last_seen)}`
  if (c.installed) return 'installed, never connected'
  return 'never connected'
}

function clientTitle(group: ClientGroup): string {
  return group.members.map((c) => {
    const lines = [`${c.name}: ${clientSubline(c)}`]
    if (c.installed_to.length) lines.push(`${c.install_source === 'connect' ? 'Connected via' : 'Found in'}: ${c.installed_to.join(', ')}`)
    if (c.last_seen && !c.online) lines.push(`Last seen ${new Date(c.last_seen).toLocaleString()}`)
    return lines.join('\n')
  }).join('\n\n')
}

async function refreshQuantmcpStatus() {
  try {
    if (!import.meta.client || !window.__TAURI_INTERNALS__) return
    const { invoke } = await import('@tauri-apps/api/core')
    quantmcpEnabled.value = await invoke<boolean>('plugin:mcp|get_quantmcp_enabled')
  } catch (e) {
    console.error('Failed to load QuantMCP status:', e)
  }
}

// The connected list refreshes on the server's client events (below) and,
// while the dashboard is open, on a slow poll — that is what catches a
// session whose stream simply went away (no DELETE, no event of its own).
watch(
  () => route.path,
  (path) => {
    if (path.startsWith('/mcp/projects')) refreshWorkspaces()
    if (path === '/mcp') {
      scanClients().then(refreshClients)
      if (!clientsInterval) {
        clientsInterval = setInterval(refreshClients, 30_000)
      }
    } else if (clientsInterval) {
      clearInterval(clientsInterval)
      clientsInterval = null
    }
  },
  { immediate: true },
)

const NAV_ITEMS = [
  { icon: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z', label: 'Dashboard', to: '/mcp', exact: true },
  { icon: 'M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z', label: 'Workspaces', to: '/mcp/projects' },
  { icon: 'M9 7V3 M15 7V3 M6 7h12v5a6 6 0 0 1-12 0V7z M12 18v3', label: 'MCPs', to: '/mcp/mcps' },
  { icon: 'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z', label: 'Tools', to: '/mcp/tools' },
  { icon: 'M12 20h9 M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z', label: 'Scripts', to: '/mcp/scripts' },
  { icon: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8', label: 'Logs', to: '/mcp/logs' },
  { icon: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z', label: 'Docs', to: '/mcp/docs' },
]

function isNavActive(item: { to: string; exact?: boolean }): boolean {
  if (item.exact) return route.path === item.to
  return route.path.startsWith(item.to)
}

// Nothing up here is polled: the mcp and tool lists change through this UI
// alone and every mutation refreshes them itself (re-assigning both arrays
// every tick also invalidated each list and chip bound to them), and the
// QuantMCP flag has a single writer — the dashboard toggle, which already
// writes the shared state itself.
//
// V3 warm cache: this layout stays mounted in the module's cached stage while
// another module is active — refresh once on return. The call lives in BOTH
// onMounted and onActivated: layouts load async, so they mount after the
// stage's activation flush and onActivated alone would miss the first visit;
// the flag makes the overlap safe and is cleared again on the way out.
// (clientsInterval needs no hooks — its route watcher already stops it the
// moment the path leaves `/mcp` and restarts it on the way back.)
let statusLoaded = false
let unsubscribeWorkspaces: (() => void) | null = null

function ensureStatusRefresh() {
  if (statusLoaded) return
  statusLoaded = true
  refreshMcps()
  refreshTools()
  refreshQuantmcpStatus()
  // Registry changes from anywhere in the suite (open/pin/forget) reflect
  // here live; the subscription survives the warm cache on purpose.
  if (!unsubscribeWorkspaces) unsubscribeWorkspaces = subscribeWorkspaces()
  // The server announces every client that connects, disconnects or gets
  // installed; the list follows at once instead of waiting for the poll.
  if (offClientEvents.length === 0) {
    offClientEvents = ['mcp.client.connected', 'mcp.client.disconnected', 'mcp.client.installed']
      .map((topic) => bus.on(topic, () => void refreshClients()))
  }
}

// A late mount can land in a stage the user already left. Setting the flag
// there would turn the real activation into a no-op and the module would show
// the snapshot from a moment it was not on screen.
onMounted(() => {
  if (inActiveKeepAliveTree()) ensureStatusRefresh()
})
onActivated(ensureStatusRefresh)

onDeactivated(() => {
  statusLoaded = false
})

onUnmounted(() => {
  if (clientsInterval) clearInterval(clientsInterval)
  unsubscribeWorkspaces?.()
  unsubscribeWorkspaces = null
  for (const off of offClientEvents) off()
  offClientEvents = []
})
</script>

<template>
  <div class="app-layout">
    <!-- V3: header-first — the shared 51px QModuleHeader spans the full
         module width; the old sidebar-first shell (logo atop the nav, a
         statusbar that never reached the left edge) is gone. -->
    <QModuleHeader module-id="mcp">
      <div class="top-statusbar">
        <div class="statusbar-left">
          <span class="status-dot" :class="quantmcpEnabled ? 'dot-online' : 'dot-offline'" />
          <span class="statusbar-label">QuantMCP Status</span>
          <span class="statusbar-chip">{{ enabledQuantmcpToolCount }}/{{ quantmcpToolCount }} tools</span>
        </div>
        <div class="statusbar-right">
          <div class="status-metric">
            <span class="metric-label">MCPs</span>
            <span class="metric-value">{{ activeExternalMcps }}/{{ totalExternalMcps }}</span>
            <span class="metric-sub">active</span>
          </div>
          <div class="status-metric">
            <span class="metric-label">Tools</span>
            <span class="metric-value">{{ activeExternalTools }}/{{ totalExternalTools }}</span>
            <span class="metric-sub">active</span>
          </div>
        </div>
      </div>
    </QModuleHeader>

    <div class="app-body">
      <QSidebar>
        <nav class="py-2">
          <QNavItem
            v-for="item in NAV_ITEMS"
            :key="item.to"
            :label="item.label"
            :icon="item.icon"
            :active="isNavActive(item)"
            @click="router.push(item.to)"
          />
        </nav>
      </QSidebar>

      <main class="main-content">
        <slot />
      </main>

      <QRightPanel storage-key="mcp.right">
      <!-- Right sidebar: Workspaces page -->
      <div v-if="isProjectsRoute" class="right-sidebar-content">
        <div class="project-sidebar-header">
          <span class="sidebar-title">Workspaces</span>
          <button class="btn-icon" title="Add workspace folder" @click="addFolder">
            +
          </button>
        </div>

        <!-- General: global AGENT.md — valid for all workspaces, not a registry entry -->
        <div class="general-wrap">
          <div
            class="project-item"
            :class="{ active: selectedWorkspaceId === GENERAL_WORKSPACE_ID }"
            @click="selectWorkspace(GENERAL_WORKSPACE_ID)"
          >
            <span class="project-item-name">General</span>
            <span class="project-item-path">Global — all workspaces</span>
          </div>
        </div>

        <div v-if="workspacesLoading && workspaces.length === 0" class="sidebar-empty">
          Loading...
        </div>
        <div v-else-if="workspaces.length === 0" class="sidebar-empty">
          <p>No workspaces yet — open a folder anywhere in QuantSuite, or add one here.</p>
          <button class="btn-primary btn-sm" @click="addFolder">
            Add Workspace
          </button>
        </div>
        <ul v-else class="project-list">
          <li
            v-for="w in workspaces"
            :key="w.id"
            class="project-item"
            :class="{ active: selectedWorkspaceId === w.id }"
            @click="selectWorkspace(w.id)"
          >
            <span class="project-item-name">
              <span v-if="w.pinned" class="ws-pin" title="Pinned">&#9733;</span>
              {{ w.name }}
            </span>
            <span class="project-item-path" :title="w.path">
              {{ w.path || '-' }}
            </span>
          </li>
        </ul>
      </div>

      <div v-else-if="isDashboardRoute" class="right-sidebar-content">
        <div class="ai-sidebar-header">
          <span class="sidebar-title">Connected AIs</span>
          <button class="btn-icon" title="Refresh" @click="refreshClients">
            &#x21bb;
          </button>
        </div>

        <div v-if="!clientsLoaded" class="sidebar-empty">
          Loading...
        </div>
        <div v-else-if="clients.length === 0" class="sidebar-empty">
          <p>No AI has connected yet.</p>
          <NuxtLink to="/mcp/settings" class="btn-primary btn-sm">Connect…</NuxtLink>
        </div>
        <div v-else class="ai-list">
          <div
            v-for="c in clientGroups"
            :key="c.id"
            class="ai-item"
            :class="{ online: c.online }"
            :title="clientTitle(c)"
          >
            <span class="ai-dot" :class="c.online ? 'dot-online' : 'dot-offline'" />
            <span class="ai-text">
              <span class="ai-name">{{ c.name }}</span>
              <span class="ai-sub">{{ clientSubline(c) }}</span>
            </span>
            <button
              v-if="!c.online"
              class="ai-forget"
              title="Forget this client (its config is not touched)"
              @click.stop="forgetClient(c)"
            >
              &times;
            </button>
          </div>

          <NuxtLink to="/mcp/settings" class="ai-setup-link">
            + Connect…
          </NuxtLink>
        </div>
      </div>
      <div v-else class="right-sidebar-content" />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>

.app-layout {
  display: flex;
  flex-direction: column;
  /* Was 100vh. The module fills its container, never the viewport. */
  height: 100%;
}

.app-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

/* Lives inside the shared QModuleHeader's center section now. */
.top-statusbar {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 0 16px;
}

.statusbar-left,
.statusbar-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.statusbar-right {
  margin-left: auto;
  justify-content: flex-end;
}

.statusbar-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.statusbar-chip {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 3px 10px;
  white-space: nowrap;
  text-transform: uppercase;
  letter-spacing: 0.02em;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-online {
  background: var(--dot-online);
  box-shadow: 0 0 6px var(--dot-online-glow);
}

.dot-offline {
  background: var(--dot-offline);
  box-shadow: 0 0 6px var(--dot-offline-glow);
}

.status-metric {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: color-mix(in srgb, var(--bg-card) 90%, transparent);
  white-space: nowrap;
}

.metric-label {
  font-size: 11px;
  color: var(--text-muted);
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.metric-value {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
}

.metric-sub {
  font-size: 11px;
  color: var(--text-secondary);
}

.main-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 32px;
}

.right-sidebar-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.project-sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 14px 12px;
  border-bottom: 1px solid var(--border);
}

.sidebar-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.btn-icon {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 16px;
  transition: all var(--transition);
  padding: 0;
}

.btn-icon:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--accent);
}

.sidebar-empty {
  padding: 24px 14px;
  color: var(--text-muted);
  font-size: 13px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  align-items: flex-start;
}

.project-list {
  list-style: none;
  margin: 0;
  padding: 8px;
  overflow-y: auto;
  flex: 1;
}

.project-item {
  padding: 9px 10px;
  border-radius: var(--radius);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 2px;
  transition: background var(--transition), opacity var(--transition);
  margin-bottom: 2px;
  user-select: none;
}

.project-item:hover {
  background: var(--bg-hover);
}

.project-item.active {
  background: var(--bg-hover);
  border-left: 2px solid var(--accent);
  padding-left: 8px;
}

.project-item-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.project-item.active .project-item-name {
  color: var(--accent);
}

.project-item-path {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ai-sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 14px 12px;
  border-bottom: 1px solid var(--border);
}

.ai-list {
  padding: 8px;
  overflow-y: auto;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ai-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: var(--radius);
  transition: background var(--transition);
  min-width: 0;
}

.ai-item:hover {
  background: var(--bg-hover);
}

.ai-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-online {
  background: var(--dot-online);
  box-shadow: 0 0 6px var(--dot-online-glow);
}

.dot-offline {
  background: var(--dot-offline);
  box-shadow: 0 0 4px var(--dot-offline), 0 0 8px var(--dot-offline-glow);
  opacity: 0.45;
}

.ai-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.ai-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ai-item.online .ai-name {
  color: var(--text-primary);
}

.ai-sub {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ai-item.online .ai-sub {
  color: var(--text-secondary);
}

/* Forget: visible on hover only, and only on offline rows. */
.ai-forget {
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--text-muted);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition: opacity var(--transition), color var(--transition), background var(--transition);
}

.ai-item:hover .ai-forget {
  opacity: 1;
}

.ai-forget:hover {
  background: var(--bg-card);
  color: var(--text-primary);
}

.ai-setup-link {
  display: block;
  margin-top: 8px;
  padding: 8px 10px;
  font-size: 12px;
  color: var(--accent);
  text-decoration: none;
  border-radius: var(--radius);
  transition: background var(--transition);
}

.ai-setup-link:hover {
  background: var(--bg-hover);
}

.btn-primary {
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: var(--radius);
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--transition);
  text-decoration: none;
}

.btn-primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.btn-sm {
  padding: 5px 10px;
  font-size: 12px;
}

/* The "General" pseudo-entry sits above the list, outside the scrolling <ul>.
   The wrapper owns the hairline so the item itself is a plain .project-item
   and its active state looks exactly like every other workspace row. */
.general-wrap {
  margin: 8px 8px 0;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}

/* ── Workspace markers ── */
.ws-pin {
  color: var(--accent);
  font-size: 11px;
  margin-right: 2px;
}
</style>
