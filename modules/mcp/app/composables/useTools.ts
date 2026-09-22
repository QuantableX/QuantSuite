import type { Ref } from 'vue'
import { bus, isModuleEnabled } from '@quantsuite/core'

export interface ToolDef {
  id: string
  name: string
  description: string
  input_schema: Record<string, any>
  script_path: string
  interpreter?: string | null
  timeout_secs?: number | null
  enabled: boolean
  native: boolean
}

export interface ToolFormData {
  name: string
  description: string
  input_schema: Record<string, any>
  script_path: string
  interpreter?: string | null
  /** Seconds before the script is killed; null means the crate's default. */
  timeout_secs?: number | null
}

export interface CodebaseIndexToolParam {
  name: string
  param_type: string
  description: string
  required: boolean
  default_value: string | null
}

/** A built-in tool (codebase index, AgentOS, kanban, worktrees) as the crate describes it. */
export interface CodebaseIndexToolInfo {
  name: string
  description: string
  parameters: CodebaseIndexToolParam[]
}

/** A `quantsuite.<module>.<name>` capability from the MCP bridge catalogue. */
export interface BridgeToolInfo {
  tool: string
  module: string
  name: string
  description: string
  sideEffects: 'read' | 'compute' | 'write' | 'external'
}

export interface ToolPreferences {
  disabledGroups: string[]
  disabledTools: string[]
}

let preferencesSubscribed = false
let preferencesRevision = 0

export function useTools() {
  // Scripted tools (`list_tools`): the native manifest + user scripts. This is
  // only a slice of what QuantMCP serves — the built-in groups below make up
  // the bulk of its `tools/list`, so any count shown as "QuantMCP tools" has
  // to include them or it reads 0 while the server exposes dozens.
  const tools = useState<ToolDef[]>('tools', () => [])
  const loading = useState('tools-loading', () => false)
  const preferences = useState<ToolPreferences | null>('tool-preferences', () => null)
  const preferencesReady = useState('tool-preferences-ready', () => false)
  const saving = useState('tool-preferences-saving', () => false)
  const error = useState<string | null>('tool-preferences-error', () => null)
  const controlsDisabled = computed(() => !preferencesReady.value || saving.value || loading.value)

  if (import.meta.client && !preferencesSubscribed) {
    preferencesSubscribed = true
    bus.on<{ scope: string; key: string; value: ToolPreferences }>('core.setting.changed', ({ payload }) => {
      if (payload.scope === 'mcp' && payload.key === 'tool_preferences') {
        preferencesRevision++
        preferences.value = payload.value
      }
    })
  }

  function groupSelected(group: string): boolean {
    return !preferences.value?.disabledGroups.includes(group)
  }

  function groupEnabled(group: string): boolean {
    return preferencesReady.value && groupSelected(group)
      && (!group.startsWith('suite.') || groupSelected('suite'))
  }

  function toolSelected(name: string): boolean {
    return !preferences.value?.disabledTools.includes(name)
  }

  function selectedCount(group: string, items: { name?: string; tool?: string; module?: string; enabled?: boolean }[]): number {
    if (!groupEnabled(group)) return 0
    return items.filter(t => t.enabled !== false && toolSelected(t.tool ?? t.name ?? '')
      && (!t.module || groupEnabled(`suite.${t.module}`))).length
  }

  async function refreshPreferences() {
    const startedAt = preferencesRevision
    try {
      if (!window.__TAURI_INTERNALS__) throw new Error('Tool settings require the desktop app.')
      const { invoke } = await import('@tauri-apps/api/core')
      const result = await invoke<ToolPreferences>('plugin:mcp|get_tool_preferences')
      if (startedAt === preferencesRevision) preferences.value = result
      preferencesReady.value = true
      error.value = null
    } catch (e) {
      preferencesReady.value = false
      error.value = `Could not load tool settings: ${String(e)}`
    }
  }

  async function setPreference(kind: 'group' | 'tool', name: string, enabled: boolean) {
    if (controlsDisabled.value) return
    saving.value = true
    error.value = null
    const startedAt = ++preferencesRevision
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const result = await invoke<ToolPreferences>('plugin:mcp|set_tool_preference', { kind, name, enabled })
      if (startedAt === preferencesRevision) preferences.value = result
    } catch (e) {
      error.value = `Could not save tool settings: ${String(e)}`
    } finally {
      saving.value = false
    }
  }
  const codebaseIndexTools = useState<CodebaseIndexToolInfo[]>('tools-codebase-index', () => [])
  const agentosTools = useState<CodebaseIndexToolInfo[]>('tools-agentos', () => [])
  const kanbanTools = useState<CodebaseIndexToolInfo[]>('tools-kanban', () => [])
  const worktreeTools = useState<CodebaseIndexToolInfo[]>('tools-worktree', () => [])
  const bridgeCatalogue = useState<BridgeToolInfo[]>('tools-bridge', () => [])
  const bridgeTools = computed(() => bridgeCatalogue.value.filter((tool) => isModuleEnabled(tool.module)))

  const nativeTools = computed(() => tools.value.filter(t => t.native))
  const customTools = computed(() => tools.value.filter(t => !t.native))
  const bridgeModules = computed(() => {
    const groups = new Map<string, BridgeToolInfo[]>()
    for (const t of bridgeTools.value) {
      if (!groups.has(t.module)) groups.set(t.module, [])
      groups.get(t.module)!.push(t)
    }
    return [...groups.entries()].map(([module, tools]) => ({ module, tools }))
  })

  // Mirrors the server's tools/list: built-in groups and the bridge catalogue
  // follow app activation, scripted tools additionally have their own toggle.
  const builtinToolCount = computed(() =>
    codebaseIndexTools.value.length
    + agentosTools.value.length
    + kanbanTools.value.length
    + worktreeTools.value.length
    + bridgeTools.value.length,
  )
  const quantmcpToolCount = computed(() => tools.value.length + builtinToolCount.value)
  const enabledQuantmcpToolCount = computed(() =>
    (isModuleEnabled('mcp') ? selectedCount('native', nativeTools.value)
      + selectedCount('custom', customTools.value)
      + selectedCount('codebase', codebaseIndexTools.value)
      + selectedCount('agentos', agentosTools.value)
      + selectedCount('kanban', kanbanTools.value)
      + selectedCount('worktree', worktreeTools.value) : 0)
    + selectedCount('suite', bridgeTools.value),
  )

  async function refreshScripted() {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        tools.value = await invoke<ToolDef[]>('plugin:mcp|list_tools')
      }
    } catch (e) {
      console.error('Failed to load tools:', e)
    }
  }

  async function loadCatalog<T>(target: Ref<T[]>, command: string, label: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        target.value = await invoke<T[]>(`plugin:mcp|${command}`)
      }
    } catch (e) {
      console.error(`Failed to load ${label}:`, e)
    }
  }

  function refreshBuiltin() {
    return Promise.all([
      loadCatalog(codebaseIndexTools, 'list_codebase_index_tools', 'codebase index tools'),
      loadCatalog(agentosTools, 'list_agentos_tools', 'AgentOS tools'),
      loadCatalog(kanbanTools, 'list_kanban_tools', 'kanban tools'),
      loadCatalog(worktreeTools, 'list_worktree_tools', 'worktree tools'),
      loadCatalog(bridgeCatalogue, 'list_bridge_tools', 'bridge tools'),
    ])
  }

  /** Everything QuantMCP serves: scripted tools plus the built-in catalogue. */
  async function refresh() {
    if (saving.value || loading.value) return
    loading.value = true
    await Promise.all([refreshScripted(), refreshBuiltin(), refreshPreferences()])
    loading.value = false
  }

  async function addTool(data: ToolFormData) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|add_tool', {
          name: data.name,
          description: data.description,
          inputSchema: data.input_schema,
          scriptPath: data.script_path,
          interpreter: data.interpreter || null,
          timeoutSecs: data.timeout_secs ?? null,
        })
        await refreshScripted()
      }
    } catch (e) {
      console.error('Failed to add tool:', e)
    }
  }

  async function updateTool(id: string, data: ToolFormData) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|update_tool', {
          id,
          name: data.name,
          description: data.description,
          inputSchema: data.input_schema,
          scriptPath: data.script_path,
          interpreter: data.interpreter || null,
          timeoutSecs: data.timeout_secs ?? null,
        })
        await refreshScripted()
      }
    } catch (e) {
      console.error('Failed to update tool:', e)
    }
  }

  async function toggleTool(id: string, enabled: boolean) {
    if (controlsDisabled.value) return
    saving.value = true
    error.value = null
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('plugin:mcp|toggle_tool', { id, enabled })
      tools.value = tools.value.map(t => t.id === id ? { ...t, enabled } : t)
    } catch (e) {
      error.value = `Could not save tool settings: ${String(e)}`
    } finally {
      saving.value = false
    }
  }

  async function deleteTool(id: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|delete_tool', { id })
        await refreshScripted()
      }
    } catch (e) {
      console.error('Failed to delete tool:', e)
    }
  }

  async function testTool(id: string, input: Record<string, any>): Promise<string> {
    if (window.__TAURI_INTERNALS__) {
      const { invoke } = await import('@tauri-apps/api/core')
      return invoke<string>('plugin:mcp|test_tool', { id, input })
    }
    return 'Tauri not available'
  }

  return {
    tools,
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
    refresh,
    nativeTools,
    customTools,
    codebaseIndexTools,
    agentosTools,
    kanbanTools,
    worktreeTools,
    bridgeTools,
    bridgeModules,
    quantmcpToolCount,
    enabledQuantmcpToolCount,
    addTool,
    updateTool,
    deleteTool,
    toggleTool,
    testTool,
  }
}
