/**
 * The rules of QParamForm, pure — the same as the forge's
 * TrendIndicator.check_params (sidecars/python/smithery/contract.py), so a
 * form can say what is wrong before the engine refuses it.
 */
import type { SmitheryParamSchema } from '@quantsuite/core'

export type ParamValues = Record<string, unknown>

function finite(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v)
}

/** Problems per parameter name; empty when every value can be used. */
export function paramProblems(schema: Record<string, SmitheryParamSchema>, values: ParamValues): Record<string, string> {
  const out: Record<string, string> = {}
  for (const [name, value] of Object.entries(values)) {
    const entry = schema[name]
    if (!entry) {
      out[name] = 'unknown parameter'
      continue
    }
    switch (entry.type) {
      case 'bool':
        if (typeof value !== 'boolean') out[name] = 'expected true or false'
        break
      case 'choice':
        if (!(entry.choices ?? []).some(c => JSON.stringify(c) === JSON.stringify(value))) out[name] = 'pick one of the choices'
        break
      case 'list': {
        const n = Array.isArray(entry.default) ? entry.default.length : 0
        if (!Array.isArray(value) || value.length !== n || !value.every(finite)) out[name] = `expected ${n} numbers`
        break
      }
      default:
        if (!finite(value)) out[name] = 'expected a number'
        else if (entry.type === 'int' && !Number.isInteger(value)) out[name] = 'expected a whole number'
        else if (entry.min != null && value < entry.min) out[name] = `at least ${entry.min}`
        else if (entry.max != null && value > entry.max) out[name] = `at most ${entry.max}`
    }
  }
  return out
}

/** The entries of `values` that differ from `base` — what an override stores. */
export function paramDiff(values: ParamValues, base: ParamValues): ParamValues {
  return Object.fromEntries(
    Object.entries(values).filter(([name, value]) => JSON.stringify(value) !== JSON.stringify(base[name])),
  )
}
