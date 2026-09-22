export interface AgentRecipient {
  id: string
  name: string
  installed: boolean
  instructions_manual: boolean
  instructions_note: string | null
  instructions_paths?: string[]
  custom?: boolean
  detection?: string | null
  mcp_support?: 'native' | 'extension' | 'custom'
  note?: string | null
  manual?: boolean
  configured?: boolean
  config_path?: string | null
  restart?: boolean
  mcp_selection_supported?: boolean
}

export interface CustomRecipient {
  id: string
  name: string
  path: string
}
