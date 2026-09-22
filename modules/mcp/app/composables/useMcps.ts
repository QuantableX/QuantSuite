export interface McpEntry {
  id: string
  name: string
  description: string
  enabled: boolean
  transport: 'stdio' | { http: { port: number } }
  command?: string | null
  args: string[]
  env: Record<string, string>
  working_dir?: string | null
}

export type McpTransportInput = 'stdio' | { http: { port: number } }

export type McpProcessStatus =
  | 'running'
  | 'stopped'
  | { error: { message: string } }

export interface McpFormData {
  name: string
  description: string
  transport: McpTransportInput
  command?: string | null
  args: string[]
  env: Record<string, string>
  working_dir?: string | null
}

export function useMcps() {
  const mcps = useState<McpEntry[]>('mcps', () => [])
  const loading = useState('mcps-loading', () => false)
  const statuses = useState<Record<string, McpProcessStatus>>('mcp-statuses', () => ({}))

  async function refresh() {
    loading.value = true
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        mcps.value = await invoke<McpEntry[]>('plugin:mcp|list_mcps')
      }
    } catch (e) {
      console.error('Failed to load MCPs:', e)
    } finally {
      loading.value = false
    }
  }

  async function toggle(id: string, enabled: boolean) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|toggle_mcp', { id, enabled })
        await refresh()
      }
    } catch (e) {
      console.error('Failed to toggle MCP:', e)
    }
  }

  async function addMcp(data: McpFormData) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|add_mcp', {
          name: data.name,
          description: data.description,
          transport: data.transport,
          command: data.command || null,
          args: data.args,
          env: data.env,
          workingDir: data.working_dir || null,
        })
        await refresh()
      }
    } catch (e) {
      console.error('Failed to add MCP:', e)
    }
  }

  async function updateMcp(id: string, data: McpFormData) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|update_mcp', {
          id,
          name: data.name,
          description: data.description,
          transport: data.transport,
          command: data.command || null,
          args: data.args,
          env: data.env,
          workingDir: data.working_dir || null,
        })
        await refresh()
      }
    } catch (e) {
      console.error('Failed to update MCP:', e)
    }
  }

  async function deleteMcp(id: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|delete_mcp', { id })
        await refresh()
      }
    } catch (e) {
      console.error('Failed to delete MCP:', e)
    }
  }

  async function startMcp(id: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|start_mcp', { id })
        await refreshStatuses()
      }
    } catch (e) {
      console.error('Failed to start MCP:', e)
    }
  }

  async function stopMcp(id: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|stop_mcp', { id })
        await refreshStatuses()
      }
    } catch (e) {
      console.error('Failed to stop MCP:', e)
    }
  }

  async function refreshStatuses() {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        statuses.value = await invoke<Record<string, McpProcessStatus>>('plugin:mcp|get_all_mcp_statuses')
      }
    } catch (e) {
      console.error('Failed to refresh statuses:', e)
    }
  }

  return {
    mcps, loading, statuses,
    refresh, toggle, addMcp, updateMcp, deleteMcp,
    startMcp, stopMcp, refreshStatuses,
  }
}
