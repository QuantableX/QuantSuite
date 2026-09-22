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
      <div>
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

    <p class="exposure-help">Disable tools you don't need to reduce token usage. Group switches keep your individual choices. These settings apply to all QuantMCP clients.</p>
    <p class="exposure-help">Connected clients are notified automatically. Reconnect or start a new chat if your client still shows disabled tools.</p>
    <p v-if="error" class="exposure-error" role="alert">{{ error }} <button class="btn-secondary" :disabled="loading || saving" @click="refresh">Retry</button></p>

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
        <div v-else-if="!loading" class="empty-state">
          <p class="empty-text">No custom tools defined yet.</p>
          <p class="empty-sub">Create tools that will be served by the built-in QuantMCP server.</p>
          <button class="btn-add empty-btn" @click="openAdd">+ Add Tool</button>
        </div>
      </div>
    </section>

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
.tools-page { width: 100%; }
.exposure-help { font-size: 12px; line-height: 1.6; color: var(--text-muted); margin-bottom: 12px; max-width: 900px; }
.exposure-error { padding: 12px; margin: 12px 0; border: 1px solid var(--error); border-radius: var(--radius); color: var(--error); }
.section-heading { display: flex; align-items: center; gap: 12px; padding: 12px 0; border-bottom: 1px solid var(--border); margin-bottom: 8px; }
.section-heading .section-label { margin-bottom: 0; min-width: 0; }
.tool-off .tool-name { color: var(--text-secondary); }
.tool-name, .tool-description, .tool-script { overflow-wrap: anywhere; }
.section-toggle { background: none; border: none; color: inherit; font: inherit; text-align: left; padding: 0; }
.section-toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; }

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 24px;
}

.page-title { font-size: 24px; font-weight: 600; }
.page-subtitle { font-size: 13px; color: var(--text-muted); margin-top: 4px; }

.header-actions { display: flex; gap: 8px; flex-shrink: 0; }

.btn-secondary {
  padding: 8px 16px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 13px;
  transition: all var(--transition);
}

.btn-secondary:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
.btn-secondary:disabled { opacity: 0.5; }

.btn-add {
  padding: 8px 16px;
  background: var(--accent);
  border: none;
  border-radius: var(--radius);
  color: var(--bg-primary);
  font-size: 13px;
  font-weight: 600;
  transition: all var(--transition);
}

.btn-add:hover { background: var(--accent-hover); }

.tool-list { display: flex; flex-direction: column; gap: 8px; }

.tool-card {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  transition: all var(--transition);
}

.tool-card:hover { border-color: var(--text-muted); }

.tool-info { flex: 1; min-width: 0; }
.tool-name { font-size: 15px; font-weight: 600; margin-bottom: 2px; font-family: monospace; }
.tool-description { font-size: 13px; color: var(--text-secondary); margin-bottom: 6px; }

.tool-meta {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
  font-family: monospace;
}

.tool-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.btn-icon {
  padding: 6px 8px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: all var(--transition);
  line-height: 1;
}

.btn-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
.btn-danger:hover { border-color: var(--error); color: var(--error); }

.tool-section { margin-bottom: 24px; }

.section-label {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 10px;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.section-toggle {
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  user-select: none;
}

.section-toggle:hover { color: var(--text-primary); }

.section-chevron {
  display: inline-block;
  font-size: 12px;
  transition: transform var(--transition);
}

.section-chevron.open {
  transform: rotate(90deg);
}

.section-count {
  font-size: 11px;
  color: var(--text-muted);
  font-weight: 400;
  margin-left: 4px;
}

.native-badge {
  font-size: 10px;
  color: var(--text-muted);
  background: var(--bg-hover);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1px 6px;
  margin-left: 6px;
  font-family: sans-serif;
  font-weight: 400;
  vertical-align: middle;
}

.ci-badge {
  font-size: 10px;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  border-radius: 8px;
  padding: 1px 6px;
  margin-left: 6px;
  font-family: sans-serif;
  font-weight: 400;
  vertical-align: middle;
}

.ci-tool-card { cursor: pointer; }

.bridge-module-label {
  display: flex; align-items: center; gap: 12px; padding: 12px 0;
  font-size: 11px;
  font-weight: 600;
  font-family: monospace;
  color: var(--text-muted);
  margin: 8px 0 0;
}
.bridge-module-label:first-child { margin-top: 0; }

.ci-params {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.ci-param-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  flex-wrap: wrap;
}

.ci-param-name {
  font-weight: 600;
  font-family: monospace;
  color: var(--text-primary);
}

.ci-param-type {
  color: var(--accent);
  font-family: monospace;
  font-size: 11px;
}

.ci-param-required {
  font-size: 10px;
  color: var(--error);
  border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
  border-radius: 4px;
  padding: 0 4px;
}

.ci-param-default {
  color: var(--text-muted);
  font-family: monospace;
  font-size: 11px;
}

.ci-param-desc {
  color: var(--text-muted);
  flex: 1;
}

.ci-param-empty {
  font-size: 12px;
  color: var(--text-muted);
  font-style: italic;
}

.empty-state { text-align: center; padding: 60px 0; }
.empty-text { font-size: 16px; color: var(--text-secondary); }
.empty-sub { font-size: 14px; color: var(--text-muted); margin-top: 4px; }
.empty-btn { margin-top: 20px; }
@media (max-width: 600px) {
  .page-header { flex-wrap: wrap; gap: 12px; }
  .tool-card { flex-wrap: wrap; padding: 12px; }
  .tool-info { flex-basis: 100%; }
  .tool-actions { margin-left: auto; }
  .section-heading { gap: 8px; }
}

</style>
