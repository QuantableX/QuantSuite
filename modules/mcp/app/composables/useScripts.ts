export type ScriptLanguage = 'Python' | 'Javascript' | 'Bash' | 'Powershell'

export interface ScriptEntry {
  id: string
  name: string
  language: ScriptLanguage
  content: string
  created_at: number
}

export interface ScriptFormData {
  name: string
  language: ScriptLanguage
  content: string
}

export function useScripts() {
  const scripts = useState<ScriptEntry[]>('scripts', () => [])
  const loading = useState('scripts-loading', () => false)

  async function refresh() {
    loading.value = true
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        scripts.value = await invoke<ScriptEntry[]>('plugin:mcp|list_scripts')
      }
    } catch (e) {
      console.error('Failed to load scripts:', e)
    } finally {
      loading.value = false
    }
  }

  async function addScript(data: ScriptFormData): Promise<ScriptEntry | null> {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        const entry = await invoke<ScriptEntry>('plugin:mcp|add_script', {
          name: data.name,
          language: data.language,
          content: data.content,
        })
        await refresh()
        return entry
      }
    } catch (e) {
      console.error('Failed to add script:', e)
    }
    return null
  }

  async function updateScript(id: string, data: ScriptFormData): Promise<ScriptEntry | null> {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        const entry = await invoke<ScriptEntry>('plugin:mcp|update_script', {
          id,
          name: data.name,
          language: data.language,
          content: data.content,
        })
        await refresh()
        return entry
      }
    } catch (e) {
      console.error('Failed to update script:', e)
    }
    return null
  }

  async function deleteScript(id: string) {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('plugin:mcp|delete_script', { id })
        await refresh()
      }
    } catch (e) {
      console.error('Failed to delete script:', e)
    }
  }

  async function getScriptPath(id: string): Promise<string | null> {
    try {
      if (window.__TAURI_INTERNALS__) {
        const { invoke } = await import('@tauri-apps/api/core')
        return await invoke<string>('plugin:mcp|get_script_path', { id })
      }
    } catch (e) {
      console.error('Failed to get script path:', e)
    }
    return null
  }

  return { scripts, loading, refresh, addScript, updateScript, deleteScript, getScriptPath }
}
