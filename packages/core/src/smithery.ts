import { invoke } from '@tauri-apps/api/core'

/** Public Smithery catalog shared by consumers; the Python registry owns scores. */
export interface SmitheryVerdict {
  score: number
  grade: string
  certified: boolean
  source?: 'historical' | 'current_run' | 'release'
}

/** An indicator's fixed version slots: the base (standard), the general
 *  optimization and the 1h / 4h / 1d optimizations. */
export type SmitheryVersionRole = 'standard' | 'optimized' | 'optimized_1h' | 'optimized_4h' | 'optimized_1d'

export interface SmitheryParamSchema {
  type: 'int' | 'float' | 'bool' | 'choice' | 'list'
  default: unknown
  label: string
  help: string
  min?: number
  max?: number
  step?: number
  choices?: unknown[]
  tested?: [number, number]
}

export interface SmitheryCatalogEntry {
  base_key?: string
  variant?: { base_key: string; label: string; status: string; role?: SmitheryVersionRole } | null
  /** `standard` for a base, the role of a version child, null otherwise. */
  role?: SmitheryVersionRole | null
  /** A base's version slots (role → key, null while empty); null on a child. */
  versions?: Record<SmitheryVersionRole, string | null> | null
  key: string
  name: string
  params?: Record<string, unknown>
  schema?: Record<string, SmitheryParamSchema>
  certification: SmitheryVerdict | null
  timeframes?: Record<string, SmitheryVerdict | null>
}

export function loadSmitheryCatalog(refresh = false): Promise<{ indicators: SmitheryCatalogEntry[] }> {
  return invoke('plugin:algo|list_indicators', { refresh })
}
