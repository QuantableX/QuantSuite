<script setup lang="ts">
import { useMcps } from '#mcp/composables/useMcps'
import { useTools } from '#mcp/composables/useTools'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { ArrowUpRight, Plug, Settings2, Wrench } from 'lucide-vue-next'
definePageMeta({ layout: 'mcp' })

const { mcps, loading, refresh, statuses, refreshStatuses, startMcp, stopMcp, toggle } = useMcps()
// The tool lists and counts are the composable's shared state — the layout
// header shows the same numbers, so both read one source.
const {
  nativeTools,
  customTools,
  refresh: refreshTools,
  toggleTool,
  codebaseIndexTools,
  agentosTools,
  kanbanTools,
  worktreeTools,
  bridgeTools,
  bridgeModules,
  quantmcpToolCount,
  enabledQuantmcpToolCount,
} = useTools()

let statusInterval: ReturnType<typeof setInterval> | null = null
const toolsExpanded = ref(false)
const codebaseToolsExpanded = ref(false)

const nativeToolsExpanded = ref(false)
const customToolsExpanded = ref(false)
const agentosToolsExpanded = ref(false)
const kanbanToolsExpanded = ref(false)
const worktreeToolsExpanded = ref(false)
const bridgeToolsExpanded = ref(false)
const expandedBridgeModules = ref<Set<string>>(new Set())
const quantmcpEnabled = useState<boolean>('quantmcp-enabled', () => true)
const expandedMcps = ref<Set<string>>(new Set())

onMounted(async () => {
  refresh()
  refreshTools()
  if (window.__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core')
    quantmcpEnabled.value = await invoke<boolean>('plugin:mcp|get_quantmcp_enabled')
  }
})

// V3 warm cache: poll statuses only while visible. Start in BOTH onMounted
// and onActivated — async pages mount after the stage's activation flush, so
// onActivated alone misses the first visit; the timer guard makes the
// overlap safe. That same late mount can land in a stage the user has already
// left, where the timer guard then blocks the real activation and the polling
// never starts — so onMounted starts it only inside an active tree.
function ensureStatusPolling() {
  if (statusInterval) return
  refreshStatuses()
  statusInterval = setInterval(refreshStatuses, 5000)
}

onMounted(() => {
  if (inActiveKeepAliveTree()) ensureStatusPolling()
})
onActivated(ensureStatusPolling)

onDeactivated(() => {
  if (statusInterval) {
    clearInterval(statusInterval)
    statusInterval = null
  }
})

onUnmounted(() => {
  if (statusInterval) clearInterval(statusInterval)
})

const totalMcps = computed(() => mcps.value.length)
const enabledCodebaseToolCount = computed(() => codebaseIndexTools.value.length)
const enabledAgentosToolCount = computed(() => agentosTools.value.length)
const enabledKanbanToolCount = computed(() => kanbanTools.value.length)
const enabledWorktreeToolCount = computed(() => worktreeTools.value.length)
const enabledNativeToolCount = computed(() => nativeTools.value.filter(t => t.enabled).length)
const enabledCustomToolCount = computed(() => customTools.value.filter(t => t.enabled).length)

function getStatus(id: string, enabled: boolean) {
  if (!enabled) {
    const s = statuses.value[id]
    if (!s || s === 'stopped') return 'disabled'
  }
  const s = statuses.value[id]
  if (!s) return 'stopped'
  if (s === 'running') return 'running'
  if (s === 'stopped') return 'stopped'
  if (typeof s === 'object' && 'error' in s) return 'error'
  return 'stopped'
}

function isRunning(id: string) {
  return statuses.value[id] === 'running'
}

function toggleMcpExpand(id: string) {
  if (expandedMcps.value.has(id)) {
    expandedMcps.value.delete(id)
  } else {
    expandedMcps.value.add(id)
  }
  expandedMcps.value = new Set(expandedMcps.value)
}

function isMcpExpanded(id: string) {
  return expandedMcps.value.has(id)
}

function mcpDetails(mcp: any) {
  const lines: string[] = []
  const transport = typeof mcp.transport === 'string'
    ? mcp.transport.toUpperCase()
    : `HTTP :${mcp.transport.http.port}`
  lines.push(`Transport: ${transport}`)
  if (mcp.command) lines.push(`Command: ${mcp.command}`)
  if (mcp.args?.length) lines.push(`Args: ${mcp.args.join(' ')}`)
  if (mcp.working_dir) lines.push(`Working dir: ${mcp.working_dir}`)
  const envKeys = Object.keys(mcp.env || {})
  if (envKeys.length) lines.push(`Env: ${envKeys.join(', ')}`)
  return lines
}

async function toggleQuantmcp(enabled: boolean) {
  quantmcpEnabled.value = enabled
  if (window.__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('plugin:mcp|set_quantmcp_enabled', { enabled })
  }
}

function toggleBridgeModule(module: string) {
  if (expandedBridgeModules.value.has(module)) {
    expandedBridgeModules.value.delete(module)
  } else {
    expandedBridgeModules.value.add(module)
  }
  expandedBridgeModules.value = new Set(expandedBridgeModules.value)
}
</script>

<template>
  <div class="dashboard">
    <QPageHeading title="Connections">
      <section class="connection-actions" aria-label="Connection tools">
        <div class="actions-row">
          <NuxtLink to="/mcp/mcps" class="action-card">
            <Plug :size="14" />
            <span class="action-label">Servers</span>
            <ArrowUpRight :size="13" class="action-arrow" />
          </NuxtLink>
          <NuxtLink to="/mcp/tools" class="action-card">
            <Wrench :size="14" />
            <span class="action-label">Tools</span>
            <ArrowUpRight :size="13" class="action-arrow" />
          </NuxtLink>
          <NuxtLink to="/mcp/settings" class="action-card">
            <Settings2 :size="14" />
            <span class="action-label">Settings</span>
            <ArrowUpRight :size="13" class="action-arrow" />
          </NuxtLink>
        </div>
      </section>
    </QPageHeading>

    <!-- The built-in server and the external ones side by side; a long list scrolls in its panel. -->
    <div class="dashboard-panels">
    <!-- QuantMCP built-in server — own box -->
    <section class="section">
      <h2 class="section-title">QuantMCP</h2>
      <div class="mcp-list">
        <div class="mcp-block">
          <div class="mcp-row">
            <button class="chevron-btn" @click="toolsExpanded = !toolsExpanded">
              <span class="row-chevron" :class="{ expanded: toolsExpanded }">›</span>
            </button>
            <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
            <span class="mcp-name">QuantMCP</span>
            <span class="badge-tools">{{ enabledQuantmcpToolCount }} / {{ quantmcpToolCount }} tools</span>
            <span class="mcp-transport">HTTP :3100</span>
            <div class="mcp-row-actions">
              <label class="toggle-switch" @click.stop>
                <input type="checkbox" :checked="quantmcpEnabled" @change="toggleQuantmcp(!quantmcpEnabled)" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>

          <div v-if="toolsExpanded" class="tool-rows">
            <!-- Codebase Tools sub-section -->
            <div v-if="codebaseIndexTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="codebaseToolsExpanded = !codebaseToolsExpanded">
                <span class="row-chevron" :class="{ expanded: codebaseToolsExpanded }">›</span>
                <span class="sub-section-label">Codebase Tools</span>
                <span class="badge-tools">{{ enabledCodebaseToolCount }} / {{ codebaseIndexTools.length }}</span>
              </div>
              <div v-if="codebaseToolsExpanded">
                <div v-for="tool in codebaseIndexTools" :key="tool.name" class="inner-row deep-row">
                  <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="native-badge-sm">quantmcp</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                </div>
              </div>
            </div>

            <!-- AgentOS Tools sub-section -->
            <div v-if="agentosTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="agentosToolsExpanded = !agentosToolsExpanded">
                <span class="row-chevron" :class="{ expanded: agentosToolsExpanded }">›</span>
                <span class="sub-section-label">AgentOS Tools</span>
                <span class="badge-tools">{{ enabledAgentosToolCount }} / {{ agentosTools.length }}</span>
              </div>
              <div v-if="agentosToolsExpanded">
                <div v-for="tool in agentosTools" :key="tool.name" class="inner-row deep-row">
                  <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="native-badge-sm">quantmcp</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                </div>
              </div>
            </div>

            <!-- Kanban Tools sub-section -->
            <div v-if="kanbanTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="kanbanToolsExpanded = !kanbanToolsExpanded">
                <span class="row-chevron" :class="{ expanded: kanbanToolsExpanded }">›</span>
                <span class="sub-section-label">Kanban Tools</span>
                <span class="badge-tools">{{ enabledKanbanToolCount }} / {{ kanbanTools.length }}</span>
              </div>
              <div v-if="kanbanToolsExpanded">
                <div v-for="tool in kanbanTools" :key="tool.name" class="inner-row deep-row">
                  <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="native-badge-sm">quantmcp</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                </div>
              </div>
            </div>

            <!-- Worktree Tools sub-section: isolated checkouts for parallel agents (PLAN-WORKTREES) -->
            <div v-if="worktreeTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="worktreeToolsExpanded = !worktreeToolsExpanded">
                <span class="row-chevron" :class="{ expanded: worktreeToolsExpanded }">›</span>
                <span class="sub-section-label">Worktree Tools</span>
                <span class="badge-tools">{{ enabledWorktreeToolCount }} / {{ worktreeTools.length }}</span>
              </div>
              <div v-if="worktreeToolsExpanded">
                <div v-for="tool in worktreeTools" :key="tool.name" class="inner-row deep-row">
                  <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="native-badge-sm">quantmcp</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                </div>
              </div>
            </div>

            <!-- Suite Tools sub-section: the quantsuite.* bridge catalogue -->
            <div v-if="bridgeTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="bridgeToolsExpanded = !bridgeToolsExpanded">
                <span class="row-chevron" :class="{ expanded: bridgeToolsExpanded }">›</span>
                <span class="sub-section-label">Suite Tools</span>
                <span class="badge-tools">{{ bridgeTools.length }} / {{ bridgeTools.length }}</span>
              </div>
              <div v-if="bridgeToolsExpanded">
                <div v-for="group in bridgeModules" :key="group.module" class="bridge-module">
                  <div class="sub-section-header deep-row" @click="toggleBridgeModule(group.module)">
                    <span class="row-chevron" :class="{ expanded: expandedBridgeModules.has(group.module) }">›</span>
                    <span class="sub-section-label">{{ group.module }}</span>
                    <span class="badge-tools">{{ group.tools.length }}</span>
                  </div>
                  <div v-if="expandedBridgeModules.has(group.module)">
                    <div v-for="tool in group.tools" :key="tool.tool" class="inner-row bridge-row">
                      <span class="status-dot" :class="quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                      <span class="inner-name">{{ tool.name }}</span>
                      <span class="native-badge-sm">{{ tool.sideEffects }}</span>
                      <span class="inner-detail">{{ tool.description }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- Basic Tools sub-section -->
            <div v-if="nativeTools.length" class="tool-sub-section">
              <div class="sub-section-header" @click="nativeToolsExpanded = !nativeToolsExpanded">
                <span class="row-chevron" :class="{ expanded: nativeToolsExpanded }">›</span>
                <span class="sub-section-label">Basic Tools</span>
                <span class="badge-tools">{{ enabledNativeToolCount }} / {{ nativeTools.length }}</span>
              </div>
              <div v-if="nativeToolsExpanded">
                <div v-for="tool in nativeTools" :key="tool.id" class="inner-row deep-row">
                  <span class="status-dot" :class="tool.enabled && quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="native-badge-sm">built-in</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                  <label class="toggle-switch inner-toggle" @click.stop>
                    <input type="checkbox" :checked="tool.enabled" @change="toggleTool(tool.id, !tool.enabled)" />
                    <span class="toggle-slider"></span>
                  </label>
                </div>
              </div>
            </div>

            <!-- Custom Tools sub-section -->
            <div class="tool-sub-section">
              <div class="sub-section-header" @click="customToolsExpanded = !customToolsExpanded">
                <span class="row-chevron" :class="{ expanded: customToolsExpanded }">›</span>
                <span class="sub-section-label">Custom Tools</span>
                <span class="badge-tools">{{ enabledCustomToolCount }} / {{ customTools.length }}</span>
              </div>
              <div v-if="customToolsExpanded">
                <div v-for="tool in customTools" :key="tool.id" class="inner-row deep-row">
                  <span class="status-dot" :class="tool.enabled && quantmcpEnabled ? 'dot-stopped' : 'dot-disabled'"></span>
                  <span class="inner-name">{{ tool.name }}</span>
                  <span class="inner-detail">{{ tool.description }}</span>
                  <label class="toggle-switch inner-toggle" @click.stop>
                    <input type="checkbox" :checked="tool.enabled" @change="toggleTool(tool.id, !tool.enabled)" />
                    <span class="toggle-slider"></span>
                  </label>
                </div>
                <div v-if="!customTools.length" class="empty-tools deep-empty">
                  No custom tools — <NuxtLink to="/mcp/tools" class="detail-link">add tools</NuxtLink>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>

    <!-- External MCPs — own box -->
    <section v-if="mcps.length" class="section mcp-section">
      <h2 class="section-title">MCPs</h2>
      <div class="mcp-list">
        <div v-for="mcp in mcps" :key="mcp.id" class="mcp-block">
          <div class="mcp-row">
            <button class="chevron-btn" @click="toggleMcpExpand(mcp.id)">
              <span class="row-chevron" :class="{ expanded: isMcpExpanded(mcp.id) }">›</span>
            </button>
            <span class="status-dot" :class="'dot-' + getStatus(mcp.id, mcp.enabled)"></span>
            <span class="mcp-name">{{ mcp.name }}</span>
            <span class="mcp-transport">
              {{ typeof mcp.transport === 'string' ? mcp.transport : `HTTP :${mcp.transport.http.port}` }}
            </span>
            <div class="mcp-row-actions">
              <button
                v-if="mcp.command"
                class="btn-icon"
                @click="isRunning(mcp.id) ? stopMcp(mcp.id) : startMcp(mcp.id)"
                :title="isRunning(mcp.id) ? 'Stop' : 'Start'"
              >
                <span v-if="isRunning(mcp.id)">&#9632;</span>
                <span v-else>&#9654;</span>
              </button>
              <label class="toggle-switch" @click.stop>
                <input type="checkbox" :checked="mcp.enabled" @change="toggle(mcp.id, !mcp.enabled)" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>

          <div v-if="isMcpExpanded(mcp.id)" class="mcp-details">
            <div v-for="line in mcpDetails(mcp)" :key="line" class="detail-row">
              <span class="detail-key">{{ line.split(': ')[0] }}</span>
              <span class="detail-val">{{ line.split(': ').slice(1).join(': ') }}</span>
            </div>
            <div v-if="!mcp.command && typeof mcp.transport !== 'string'" class="detail-row">
              <span class="detail-key">Endpoint</span>
              <span class="detail-val">http://localhost:{{ mcp.transport.http.port }}/mcp</span>
            </div>
          </div>
        </div>
      </div>
    </section>

    <QEmptyState v-if="!loading && totalMcps === 0" title="No MCPs configured yet.">
      <NuxtLink to="/mcp/mcps" class="detail-link">Add a server</NuxtLink>
    </QEmptyState>
    </div>
  </div>
</template>

<style scoped>
.dashboard { display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0; container-type: inline-size; }
.dashboard > :deep(.qph) { flex-shrink: 0; }

.action-arrow { color: var(--text-muted); }

/* Actions row: slim links beside the title */
.actions-row { display: flex; flex-wrap: wrap; gap: var(--qss-gap-tight); }

.action-card {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--qss-ctl-h);
  padding: 0 10px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  color: var(--text-secondary);
  transition: all var(--transition);
}

.action-card:hover { background: var(--bg-hover); color: var(--text-primary); }
.action-label { font-size: var(--qss-text-label); font-weight: 500; }

.dashboard-panels {
  flex: 1 1 0;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-rows: minmax(0, 1fr);
  gap: var(--qss-gap);
  align-items: start;
}

.section {
  display: flex;
  flex-direction: column;
  min-height: 0;
  max-height: 100%;
  padding: var(--qss-panel-pad);
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--qss-panel-radius);
}

.section-title { flex-shrink: 0; font-size: var(--qss-text-section); font-weight: 600; margin-bottom: var(--qss-gap-tight); color: var(--text-primary); }

.mcp-list { flex: 1 1 auto; min-height: 0; overflow-y: auto; }

.mcp-block { border-bottom: 1px solid var(--qss-border-subtle); }
.mcp-block:last-child { border-bottom: none; }

.mcp-row {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  min-height: var(--qss-row-h);
  font-size: var(--qss-text-section);
}

.chevron-btn {
  background: transparent;
  border: none;
  padding: 2px 4px;
  cursor: pointer;
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

.row-chevron {
  font-size: 14px;
  color: var(--text-muted);
  transition: transform var(--transition);
  display: inline-block;
  line-height: 1;
}
.row-chevron.expanded { transform: rotate(90deg); }

.status-dot   { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.dot-running  { background: var(--dot-running); }
.dot-stopped  { background: var(--dot-stopped); }
.dot-error    { background: var(--error); }
.dot-disabled { background: var(--dot-disabled); opacity: 0.45; }

.mcp-name { font-weight: 600; }

.badge-tools {
  font-size: var(--qss-text-meta);
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.mcp-transport {
  margin-left: auto;
  color: var(--text-muted);
  font-size: var(--qss-text-meta);
  font-family: var(--qss-font-mono);
}

.mcp-row-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  margin-left: var(--qss-gap-tight);
}

.btn-icon {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1;
}

.btn-icon:hover { background: var(--bg-hover); color: var(--text-primary); }

/* Tool rows: hairlines inside the one panel */
.tool-rows { border-top: 1px solid var(--qss-border-subtle); }

.inner-row {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  min-height: var(--qss-row-h);
  padding: 2px 4px 2px 26px;
  font-size: var(--qss-text-label);
  border-bottom: 1px solid var(--qss-border-subtle);
}
.inner-row:last-child { border-bottom: none; }

.inner-name { font-weight: 500; font-family: var(--qss-font-mono); min-width: 120px; }
.inner-detail { flex: 1; color: var(--text-muted); font-size: var(--qss-text-meta); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.inner-toggle { margin-left: auto; flex-shrink: 0; }

.tool-sub-section { border-bottom: 1px solid var(--qss-border-subtle); }
.tool-sub-section:last-child { border-bottom: none; }

.sub-section-header {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  min-height: var(--qss-row-h);
  padding: 2px 4px 2px 26px;
  font-size: var(--qss-text-label);
  cursor: pointer;
  user-select: none;
}
.sub-section-header:hover { background: var(--bg-hover); }

.sub-section-label {
  font-weight: 600;
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
}

.native-badge-sm {
  font-size: 11px;
  color: var(--text-muted);
  background: var(--bg-hover);
  border-radius: 6px;
  padding: 0 5px;
  flex-shrink: 0;
}

.deep-row { padding-left: 42px !important; }
.deep-empty { padding-left: 42px !important; }
.bridge-row { padding-left: 58px !important; }
.bridge-module .sub-section-label { font-family: var(--qss-font-mono); }

.empty-tools {
  padding: 6px 4px 6px 26px;
  font-size: var(--qss-text-label);
  color: var(--text-muted);
}

/* MCP expanded details */
.mcp-details {
  padding: 4px 4px var(--qss-gap-tight) 26px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.detail-row {
  display: flex;
  gap: 10px;
  font-size: var(--qss-text-meta);
  font-family: var(--qss-font-mono);
}

.detail-key { color: var(--text-muted); min-width: 90px; flex-shrink: 0; }
.detail-val { color: var(--text-secondary); word-break: break-all; }

.detail-link { color: var(--accent); text-decoration: none; }
.detail-link:hover { text-decoration: underline; }

/* Toggle switch */
.toggle-switch { position: relative; width: 36px; height: 20px; flex-shrink: 0; cursor: pointer; }
.toggle-switch input { display: none; }

.toggle-slider {
  position: absolute;
  inset: 0;
  background: var(--border);
  border-radius: 10px;
  cursor: pointer;
  transition: all var(--transition);
}

.toggle-slider::before {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  background: var(--text-secondary);
  border-radius: 50%;
  transition: all var(--transition);
}

.toggle-switch input:checked + .toggle-slider { background: var(--accent); }
.toggle-switch input:checked + .toggle-slider::before { transform: translateX(16px); background: white; }

@container (max-width: 900px) {
  .dashboard-panels { grid-template-columns: minmax(0, 1fr); grid-template-rows: none; align-content: start; overflow-y: auto; }
  .section { max-height: none; }
}
</style>
