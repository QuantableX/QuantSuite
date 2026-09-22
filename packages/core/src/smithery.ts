import { invoke } from '@tauri-apps/api/core'

/** Public Smithery catalog shared by consumers; the Python registry owns scores. */
export interface SmitheryVerdict {
  score: number
  grade: string
  certified: boolean
  source?: 'historical' | 'current_run'
}

export interface SmitheryCatalogEntry {
  base_key?: string
  variant?: { base_key: string; label: string; status: string } | null
  key: string
  name: string
  certification: SmitheryVerdict | null
  timeframes?: Record<string, SmitheryVerdict | null>
}

export function loadSmitheryCatalog(refresh = false): Promise<{ indicators: SmitheryCatalogEntry[] }> {
  return invoke('plugin:algo|list_indicators', { refresh })
}
