<script setup lang="ts">
import { useTools } from '#mcp/composables/useTools'
definePageMeta({ layout: 'mcp' })

import type { ToolDef, ToolFormData } from '#mcp/composables/useTools'

// Every list here is the composable's shared state — one `refresh()` loads
// the scripted tools and the built-in catalogue the layout header counts.
const {
  nativeTools,
  customTools,
  loading,
  saving,
  error,
  controlsDisabled,
  preferencesReady,
  groupSelected,
  groupEnabled,
  toolSelected,
  selectedCount,
  setPreference,
  enabledQuantmcpToolCount,
  quantmcpToolCount,
  refresh,
  addTool,
  updateTool,
  deleteTool,
  toggleTool,
  codebaseIndexTools,
  agentosTools,
  kanbanTools,
  worktreeTools,
  bridgeTools,
  bridgeModules,
} = useTools()

const showModal = ref(false)
const editingTool = ref<ToolDef | null>(null)
const showTest = ref(false)
const testingTool = ref<ToolDef | null>(null)
const showEditor = ref(false)
const editingScriptPath = ref('')
const editingScriptToolName = ref('')
const ciToolsExpanded = ref<Set<string>>(new Set())
const aosToolsExpanded = ref<Set<string>>(new Set())
const kanToolsExpanded = ref<Set<string>>(new Set())

// Collapsible section state
const codebaseToolsOpen = ref(false)
const agentosToolsOpen = ref(false)
const kanbanToolsOpen = ref(false)
const worktreeToolsOpen = ref(false)
const wtToolsExpanded = ref<Set<string>>(new Set())
const suiteToolsOpen = ref(false)
const basicToolsOpen = ref(false)
const customToolsOpen = ref(false)

onMounted(() => {
  refresh()
})

function openAdd() {
  editingTool.value = null
  showModal.value = true
}

function openEdit(tool: ToolDef) {
  editingTool.value = tool
  showModal.value = true
}

function openTest(tool: ToolDef) {
  testingTool.value = tool
  showTest.value = true
}

function openScriptEditor(tool: ToolDef) {
  editingScriptPath.value = tool.script_path
  editingScriptToolName.value = tool.name
  showEditor.value = true
}

async function handleSubmit(data: ToolFormData) {
  if (editingTool.value) {
    await updateTool(editingTool.value.id, data)
  } else {
    await addTool(data)
  }
}

async function handleDelete(tool: ToolDef) {
  if (confirm(`Delete tool "${tool.name}"?`)) {
    await deleteTool(tool.id)
  }
}

function paramCount(tool: ToolDef): number {
  return Object.keys(tool.input_schema?.properties || {}).length
}

function toggleAosTool(name: string) {
  const next = new Set(aosToolsExpanded.value)
  next.has(name) ? next.delete(name) : next.add(name)
  aosToolsExpanded.value = next
}

function toggleWtTool(name: string) {
  const next = new Set(wtToolsExpanded.value)
  if (next.has(name)) next.delete(name)
  else next.add(name)
  wtToolsExpanded.value = next
}

function toggleKanTool(name: string) {
  const next = new Set(kanToolsExpanded.value)
  next.has(name) ? next.delete(name) : next.add(name)
  kanToolsExpanded.value = next
}

function toggleCiTool(name: string) {
  const next = new Set(ciToolsExpanded.value)
  if (next.has(name)) {
    next.delete(name)
  } else {
    next.add(name)
  }
  ciToolsExpanded.value = next
}


</script>

<template>
  <div class="tools-page">
    <div class="page-header">
      <div class="page-title-row">
        <h1 class="page-title">Tools</h1>
        <p class="page-subtitle">{{ preferencesReady ? enabledQuantmcpToolCount : 'Loading' }} / {{ quantmcpToolCount }} tools enabled for AI clients</p>
      </div>
      <div class="header-actions">
        <button class="btn-secondary" @click="refresh" :disabled="loading || saving">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
        <button class="btn-add" @click="openAdd">+ Add Tool</button>
      </div>
    </div>

    <div class="exposure-notes">
      <p class="exposure-help">Disable tools you don't need to reduce token usage. Group switches keep your individual choices. These settings apply to all QuantMCP clients.</p>
      <p class="exposure-help">Connected clients are notified automatically. Reconnect or start a new chat if your client still shows disabled tools.</p>
    </div>
    <p v-if="error" class="exposure-error" role="alert">{{ error }} <button class="btn-secondary" :disabled="loading || saving" @click="refresh">Retry</button></p>

    <!-- Every group is one row of one panel; the open ones scroll inside it. -->
    <div class="tools-body">
    <!-- Codebase Tools (built into QuantMCP) -->
    <section v-if="codebaseIndexTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="codebaseToolsOpen" @click="codebaseToolsOpen = !codebaseToolsOpen">
        <span class="section-chevron" :class="{ open: codebaseToolsOpen }">&#9656;</span>
        Codebase Tools
        <span class="section-count">{{ selectedCount('codebase', codebaseIndexTools) }} / {{ codebaseIndexTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('codebase')" :disabled="controlsDisabled" label="Enable codebase tools" @change="setPreference('group', 'codebase', $event)" />
      </div>
      <div v-if="codebaseToolsOpen" class="tool-list">
        <div
          v-for="tool in codebaseIndexTools"
          :key="tool.name"
          class="tool-card ci-tool-card"
          :class="{ 'tool-off': !groupEnabled('codebase') || !toolSelected(tool.name) }"
          @click="toggleCiTool(tool.name)"
        >
          <div class="tool-info">
            <div class="tool-name">
              {{ tool.name }}
              <span class="ci-badge">quantmcp</span>
            </div>
            <div class="tool-description">{{ tool.description }}</div>
            <div class="tool-meta">
              <span class="tool-params">{{ tool.parameters.length }} param{{ tool.parameters.length !== 1 ? 's' : '' }}</span>
            </div>
            <div v-if="ciToolsExpanded.has(tool.name)" class="ci-params">
              <div v-if="tool.parameters.length === 0" class="ci-param-empty">No parameters</div>
              <div v-for="param in tool.parameters" :key="param.name" class="ci-param-row">
                <span class="ci-param-name">{{ param.name }}</span>
                <span class="ci-param-type">{{ param.param_type }}</span>
                <span v-if="param.required" class="ci-param-required">required</span>
                <span v-if="!param.required && param.default_value" class="ci-param-default">= {{ param.default_value }}</span>
                <span class="ci-param-desc">{{ param.description }}</span>
              </div>
            </div>
          </div>
          <McpToolToggle :checked="toolSelected(tool.name)" :disabled="controlsDisabled" :group-off="!groupEnabled('codebase')" :label="`Enable ${tool.name}`" @change="setPreference('tool', tool.name, $event)" />
        </div>
      </div>
    </section>

    <!-- AgentOS Tools (agent instructions + memory + concepts) -->
    <section v-if="agentosTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="agentosToolsOpen" @click="agentosToolsOpen = !agentosToolsOpen">
        <span class="section-chevron" :class="{ open: agentosToolsOpen }">&#9656;</span>
        AgentOS Tools
        <span class="section-count">{{ selectedCount('agentos', agentosTools) }} / {{ agentosTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('agentos')" :disabled="controlsDisabled" label="Enable agentos tools" @change="setPreference('group', 'agentos', $event)" />
      </div>
      <div v-if="agentosToolsOpen" class="tool-list">
        <div
          v-for="tool in agentosTools"
          :key="tool.name"
          class="tool-card ci-tool-card"
          :class="{ 'tool-off': !groupEnabled('agentos') || !toolSelected(tool.name) }"
          @click="toggleAosTool(tool.name)"
        >
          <div class="tool-info">
            <div class="tool-name">
              {{ tool.name }}
              <span class="ci-badge">quantmcp</span>
            </div>
            <div class="tool-description">{{ tool.description }}</div>
            <div class="tool-meta">
              <span class="tool-params">{{ tool.parameters.length }} param{{ tool.parameters.length !== 1 ? 's' : '' }}</span>
            </div>
            <div v-if="aosToolsExpanded.has(tool.name)" class="ci-params">
              <div v-if="tool.parameters.length === 0" class="ci-param-empty">No parameters</div>
              <div v-for="param in tool.parameters" :key="param.name" class="ci-param-row">
                <span class="ci-param-name">{{ param.name }}</span>
                <span class="ci-param-type">{{ param.param_type }}</span>
                <span v-if="param.required" class="ci-param-required">required</span>
                <span v-if="!param.required && param.default_value" class="ci-param-default">= {{ param.default_value }}</span>
                <span class="ci-param-desc">{{ param.description }}</span>
              </div>
            </div>
          </div>
          <McpToolToggle :checked="toolSelected(tool.name)" :disabled="controlsDisabled" :group-off="!groupEnabled('agentos')" :label="`Enable ${tool.name}`" @change="setPreference('tool', tool.name, $event)" />
        </div>
      </div>
    </section>

    <!-- Kanban Tools -->
    <section v-if="kanbanTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="kanbanToolsOpen" @click="kanbanToolsOpen = !kanbanToolsOpen">
        <span class="section-chevron" :class="{ open: kanbanToolsOpen }">&#9656;</span>
        Kanban Tools
        <span class="section-count">{{ selectedCount('kanban', kanbanTools) }} / {{ kanbanTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('kanban')" :disabled="controlsDisabled" label="Enable kanban tools" @change="setPreference('group', 'kanban', $event)" />
      </div>
      <div v-if="kanbanToolsOpen" class="tool-list">
        <div
          v-for="tool in kanbanTools"
          :key="tool.name"
          class="tool-card ci-tool-card"
          :class="{ 'tool-off': !groupEnabled('kanban') || !toolSelected(tool.name) }"
          @click="toggleKanTool(tool.name)"
        >
          <div class="tool-info">
            <div class="tool-name">
              {{ tool.name }}
              <span class="ci-badge">quantmcp</span>
            </div>
            <div class="tool-description">{{ tool.description }}</div>
            <div class="tool-meta">
              <span class="tool-params">{{ tool.parameters.length }} param{{ tool.parameters.length !== 1 ? 's' : '' }}</span>
            </div>
            <div v-if="kanToolsExpanded.has(tool.name)" class="ci-params">
              <div v-if="tool.parameters.length === 0" class="ci-param-empty">No parameters</div>
              <div v-for="param in tool.parameters" :key="param.name" class="ci-param-row">
                <span class="ci-param-name">{{ param.name }}</span>
                <span class="ci-param-type">{{ param.param_type }}</span>
                <span v-if="param.required" class="ci-param-required">required</span>
                <span v-if="!param.required && param.default_value" class="ci-param-default">= {{ param.default_value }}</span>
                <span class="ci-param-desc">{{ param.description }}</span>
              </div>
            </div>
          </div>
          <McpToolToggle :checked="toolSelected(tool.name)" :disabled="controlsDisabled" :group-off="!groupEnabled('kanban')" :label="`Enable ${tool.name}`" @change="setPreference('tool', tool.name, $event)" />
        </div>
      </div>
    </section>

    <!-- Worktree Tools: isolated checkouts for parallel agents (docs/PLAN-WORKTREES.md) -->
    <section v-if="worktreeTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="worktreeToolsOpen" @click="worktreeToolsOpen = !worktreeToolsOpen">
        <span class="section-chevron" :class="{ open: worktreeToolsOpen }">&#9656;</span>
        Worktree Tools
        <span class="section-count">{{ selectedCount('worktree', worktreeTools) }} / {{ worktreeTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('worktree')" :disabled="controlsDisabled" label="Enable worktree tools" @change="setPreference('group', 'worktree', $event)" />
      </div>
      <div v-if="worktreeToolsOpen" class="tool-list">
        <div
          v-for="tool in worktreeTools"
          :key="tool.name"
          class="tool-card ci-tool-card"
          :class="{ 'tool-off': !groupEnabled('worktree') || !toolSelected(tool.name) }"
          @click="toggleWtTool(tool.name)"
        >
          <div class="tool-info">
            <div class="tool-name">
              {{ tool.name }}
              <span class="ci-badge">quantmcp</span>
            </div>
            <div class="tool-description">{{ tool.description }}</div>
            <div class="tool-meta">
              <span class="tool-params">{{ tool.parameters.length }} param{{ tool.parameters.length !== 1 ? 's' : '' }}</span>
            </div>
            <div v-if="wtToolsExpanded.has(tool.name)" class="ci-params">
              <div v-if="tool.parameters.length === 0" class="ci-param-empty">No parameters</div>
              <div v-for="param in tool.parameters" :key="param.name" class="ci-param-row">
                <span class="ci-param-name">{{ param.name }}</span>
                <span class="ci-param-type">{{ param.param_type }}</span>
                <span v-if="param.required" class="ci-param-required">required</span>
                <span v-if="!param.required && param.default_value" class="ci-param-default">= {{ param.default_value }}</span>
                <span class="ci-param-desc">{{ param.description }}</span>
              </div>
            </div>
          </div>
          <McpToolToggle :checked="toolSelected(tool.name)" :disabled="controlsDisabled" :group-off="!groupEnabled('worktree')" :label="`Enable ${tool.name}`" @change="setPreference('tool', tool.name, $event)" />
        </div>
      </div>
    </section>

    <!-- Suite Tools: the quantsuite.* bridge catalogue from the module manifests -->
    <section v-if="bridgeTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="suiteToolsOpen" @click="suiteToolsOpen = !suiteToolsOpen">
        <span class="section-chevron" :class="{ open: suiteToolsOpen }">&#9656;</span>
        Suite Tools
        <span class="section-count">{{ selectedCount('suite', bridgeTools) }} / {{ bridgeTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('suite')" :disabled="controlsDisabled" label="Enable suite tools" @change="setPreference('group', 'suite', $event)" />
      </div>
      <div v-if="suiteToolsOpen" class="tool-list">
        <template v-for="group in bridgeModules" :key="group.module">
          <div class="bridge-module-label">
            <span>quantsuite.{{ group.module }} <span class="section-count">{{ selectedCount(`suite.${group.module}`, group.tools) }} / {{ group.tools.length }}</span></span>
            <McpToolToggle :checked="groupSelected(`suite.${group.module}`)" :disabled="controlsDisabled" :group-off="!groupEnabled('suite')" :label="`Enable ${group.module} tools`" @change="setPreference('group', `suite.${group.module}`, $event)" />
          </div>
          <div v-for="tool in group.tools" :key="tool.tool" class="tool-card" :class="{ 'tool-off': !groupEnabled(`suite.${group.module}`) || !toolSelected(tool.tool) }">
            <div class="tool-info">
              <div class="tool-name">
                {{ tool.name }}
                <span class="ci-badge">{{ group.module }}</span>
                <span class="native-badge">{{ tool.sideEffects }}</span>
              </div>
              <div class="tool-description">{{ tool.description }}</div>
            </div>
            <McpToolToggle :checked="toolSelected(tool.tool)" :disabled="controlsDisabled" :group-off="!groupEnabled(`suite.${group.module}`)" :label="`Enable ${tool.tool}`" @change="setPreference('tool', tool.tool, $event)" />
          </div>
        </template>
      </div>
    </section>

    <!-- Basic Tools (native, non-editable) -->
    <section v-if="nativeTools.length" class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="basicToolsOpen" @click="basicToolsOpen = !basicToolsOpen">
        <span class="section-chevron" :class="{ open: basicToolsOpen }">&#9656;</span>
        Basic Tools
        <span class="section-count">{{ selectedCount('native', nativeTools) }} / {{ nativeTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('native')" :disabled="controlsDisabled" label="Enable native tools" @change="setPreference('group', 'native', $event)" />
      </div>
      <div v-if="basicToolsOpen" class="tool-list">
        <div v-for="tool in nativeTools" :key="tool.id" class="tool-card" :class="{ 'tool-off': !groupEnabled('native') || !tool.enabled }">
          <div class="tool-info">
            <div class="tool-name">
              {{ tool.name }}
              <span class="native-badge">built-in</span>
            </div>
            <div class="tool-description">{{ tool.description }}</div>
            <div class="tool-meta">
              <span class="tool-params">{{ paramCount(tool) }} param{{ paramCount(tool) !== 1 ? 's' : '' }}</span>
            </div>
          </div>
          <div class="tool-actions">
            <button class="btn-icon" @click="openTest(tool)" title="Test">&#9654;</button>
            <McpToolToggle :checked="tool.enabled" :disabled="controlsDisabled" :group-off="!groupEnabled('native')" :label="`Enable ${tool.name}`" @change="toggleTool(tool.id, $event)" />
          </div>
        </div>
      </div>
    </section>

    <!-- Custom Tools (user-created, full CRUD) -->
    <section class="tool-section">
      <div class="section-heading">
      <h2 class="section-label"><button class="section-toggle" :aria-expanded="customToolsOpen" @click="customToolsOpen = !customToolsOpen">
        <span class="section-chevron" :class="{ open: customToolsOpen }">&#9656;</span>
        Custom Tools
        <span class="section-count">{{ selectedCount('custom', customTools) }} / {{ customTools.length }}</span>
      </button></h2>
      <McpToolToggle :checked="groupSelected('custom')" :disabled="controlsDisabled" label="Enable custom tools" @change="setPreference('group', 'custom', $event)" />
      </div>
      <div v-if="customToolsOpen">
        <div v-if="customTools.length" class="tool-list">
          <div v-for="tool in customTools" :key="tool.id" class="tool-card" :class="{ 'tool-off': !groupEnabled('custom') || !tool.enabled }">
            <div class="tool-info">
              <div class="tool-name">{{ tool.name }}</div>
              <div class="tool-description">{{ tool.description }}</div>
              <div class="tool-meta">
                <span class="tool-script">{{ tool.script_path }}</span>
                <span v-if="tool.interpreter" class="tool-interp">{{ tool.interpreter }}</span>
                <span v-if="tool.timeout_secs" class="tool-timeout" title="Timeout">{{ tool.timeout_secs }}s</span>
                <span class="tool-params">{{ paramCount(tool) }} param{{ paramCount(tool) !== 1 ? 's' : '' }}</span>
              </div>
            </div>
            <div class="tool-actions">
              <button class="btn-icon" @click="openTest(tool)" title="Test">&#9654;</button>
              <button
                v-if="tool.script_path"
                class="btn-icon"
                @click="openScriptEditor(tool)"
                title="Edit Script"
              >&#128196;</button>
              <button class="btn-icon" @click="openEdit(tool)" title="Edit Tool">&#9998;</button>
              <button class="btn-icon btn-danger" @click="handleDelete(tool)" title="Delete">&#10005;</button>
              <McpToolToggle :checked="tool.enabled" :disabled="controlsDisabled" :group-off="!groupEnabled('custom')" :label="`Enable ${tool.name}`" @change="toggleTool(tool.id, $event)" />
            </div>
          </div>
        </div>
        <QEmptyState v-else-if="!loading" compact title="No custom tools defined yet." description="Create tools that will be served by the built-in QuantMCP server.">
          <button class="btn-add" @click="openAdd">+ Add Tool</button>
        </QEmptyState>
      </div>
    </section>
    </div>

    <McpToolFormModal v-model="showModal" :edit-tool="editingTool" @submit="handleSubmit" />
    <McpToolTestModal v-model="showTest" :tool="testingTool" />
    <McpScriptEditorModal
      v-model="showEditor"
      :script-path="editingScriptPath"
      :tool-name="editingScriptToolName"
    />
  </div>
</template>

<style scoped>
.tools-page { display: flex; flex-direction: column; gap: var(--qss-gap); width: 100%; height: 100%; min-height: 0; container-type: inline-size; }
.exposure-notes { flex-shrink: 0; }
.exposure-help { font-size: var(--qss-text-meta); line-height: 1.5; color: var(--text-muted); }
.exposure-error { flex-shrink: 0; padding: 6px var(--qss-panel-pad); border: 1px solid var(--error); border-radius: var(--qss-ctl-radius); color: var(--error); font-size: var(--qss-text-label); }
.tool-off .tool-name { color: var(--text-secondary); }
.tool-name, .tool-description, .tool-script { overflow-wrap: anywhere; }

.page-header {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap-tight) var(--qss-gap);
  min-height: var(--qss-ctl-h);
}

.page-title-row { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px var(--qss-gap); min-width: 0; }
.page-title { font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.page-subtitle { font-size: var(--qss-text-meta); color: var(--text-muted); font-variant-numeric: tabular-nums; }

.header-actions { display: flex; gap: var(--qss-gap-tight); flex-shrink: 0; }

.btn-secondary {
  height: var(--qss-ctl-h);
  padding: 0 12px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
.btn-secondary:disabled { opacity: 0.5; }

.btn-add {
  height: var(--qss-ctl-h);
  padding: 0 12px;
  background: var(--accent);
  border: none;
  border-radius: var(--qss-ctl-radius);
  color: var(--qss-accent-ink);
  font-size: var(--qss-text-label);
  font-weight: 600;
  transition: all var(--transition);
}

.btn-add:hover { background: var(--accent-hover); }

.tools-body {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--qss-panel-radius);
}

.tool-section + .tool-section { border-top: 1px solid var(--qss-border-subtle); }

/* The group row stays in view, with its switch, while its tools scroll under it. */
.section-heading {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 2px var(--qss-panel-pad);
  background: var(--bg-secondary);
}

.section-label {
  min-width: 0;
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--text-primary);
}

.section-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0;
  background: none;
  border: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  user-select: none;
}

.section-toggle:hover { color: var(--text-primary); }
.section-toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; }

.section-chevron {
  display: inline-block;
  width: 12px;
  color: var(--text-muted);
  font-size: 12px;
  transition: transform var(--transition);
}

.section-chevron.open {
  transform: rotate(90deg);
}

.section-count {
  font-size: var(--qss-text-meta);
  color: var(--text-muted);
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}

.tool-list { display: flex; flex-direction: column; }

.tool-card {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 4px var(--qss-panel-pad) 4px 30px;
  border-top: 1px solid var(--qss-border-subtle);
  transition: background var(--transition);
}

.tool-card:hover { background: var(--bg-hover); }

/* Wide space becomes columns: name, what it does, its parameters. */
.tool-info {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(0, 5fr) auto;
  column-gap: var(--qss-gap);
  align-items: baseline;
}
.tool-name { font-size: var(--qss-text-label); font-weight: 500; font-family: var(--qss-font-mono); }
.tool-description { font-size: var(--qss-text-label); line-height: 1.45; color: var(--text-secondary); }

.tool-meta {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 2px var(--qss-gap-tight);
  max-width: 320px;
  font-size: var(--qss-text-meta);
  color: var(--text-muted);
  font-family: var(--qss-font-mono);
}

.tool-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.btn-icon {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1;
}

.btn-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
.btn-danger:hover { border-color: var(--error); color: var(--error); }

.native-badge,
.ci-badge {
  font-size: 11px;
  color: var(--text-muted);
  background: var(--bg-hover);
  border-radius: 6px;
  padding: 0 5px;
  margin-left: 4px;
  font-family: var(--qss-font-sans);
  font-weight: 400;
  vertical-align: 1px;
}

.ci-tool-card { cursor: pointer; }

.bridge-module-label {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-row-h);
  padding: 2px var(--qss-panel-pad) 2px 30px;
  border-top: 1px solid var(--qss-border-subtle);
  font-size: var(--qss-text-label);
  font-weight: 600;
  font-family: var(--qss-font-mono);
  color: var(--text-secondary);
}

.ci-params {
  grid-column: 1 / -1;
  margin-top: 4px;
  padding-top: 4px;
  border-top: 1px solid var(--qss-border-subtle);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ci-param-row {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  font-size: var(--qss-text-label);
  flex-wrap: wrap;
}

.ci-param-name {
  font-weight: 600;
  font-family: var(--qss-font-mono);
  color: var(--text-primary);
}

.ci-param-type {
  color: var(--accent);
  font-family: var(--qss-font-mono);
  font-size: var(--qss-text-meta);
}

.ci-param-required {
  font-size: 11px;
  color: var(--error);
  border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
  border-radius: 4px;
  padding: 0 4px;
}

.ci-param-default {
  color: var(--text-muted);
  font-family: var(--qss-font-mono);
  font-size: var(--qss-text-meta);
}

.ci-param-desc {
  color: var(--text-muted);
  flex: 1;
}

.ci-param-empty {
  font-size: var(--qss-text-label);
  color: var(--text-muted);
}

@container (max-width: 760px) {
  .tool-info { grid-template-columns: minmax(0, 1fr); row-gap: 2px; }
  .tool-meta { justify-content: flex-start; max-width: none; }
  .tool-card { flex-wrap: wrap; padding-left: var(--qss-panel-pad); }
  .tool-info { flex-basis: 100%; }
  .tool-actions { margin-left: auto; }
  .section-heading { gap: var(--qss-gap-tight); }
}
</style>
