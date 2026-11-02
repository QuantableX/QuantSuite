/**
 * Unsent answers to waiting requests (parity card 05): what the user has
 * picked or typed into a question or an MCP form so far. Kept per thread and
 * request id, so streaming, switching threads or a failed submit never loses
 * it, and an answer can never land on another request. Memory only — a secret
 * answer is never written to disk; a restarted app has no waiting requests
 * (their process is gone) and so needs no drafts.
 */
import { reactive } from 'vue'

export interface RequestDraft {
  /** Question text → picked option labels. */
  picked: Record<string, string[]>
  /** Question text → free-text answer. */
  other: Record<string, string>
  /** The question step shown. */
  step: number
  /** Form field key → value. */
  values: Record<string, unknown>
  /** A web-page step was opened or its link copied. */
  opened: boolean
  /** The last refusal of a submit, shown with the form. */
  error: string | null
}

const drafts = reactive(new Map<string, RequestDraft>())
const key = (threadId: string, requestId: string) => `${threadId}\u0000${requestId}`

export function requestDraft(threadId: string, requestId: string): RequestDraft {
  const k = key(threadId, requestId)
  let d = drafts.get(k)
  if (!d) {
    drafts.set(k, { picked: {}, other: {}, step: 0, values: {}, opened: false, error: null })
    d = drafts.get(k)!
  }
  return d
}

/** Drop the drafts of a thread's requests that no longer wait. */
export function pruneRequestDrafts(threadId: string, waiting: string[]) {
  const keep = new Set(waiting.map((r) => key(threadId, r)))
  for (const k of [...drafts.keys()]) {
    if (k.startsWith(`${threadId}\u0000`) && !keep.has(k)) drafts.delete(k)
  }
}

// ── MCP forms ──

export interface FormOption {
  value: string
  label: string
}

export interface FormField {
  key: string
  title: string
  description?: string | null
  required: boolean
  kind: 'text' | 'number' | 'integer' | 'boolean' | 'choice' | 'multi'
  default?: unknown
  options?: FormOption[]
  minLength?: number | null
  maxLength?: number | null
  format?: 'email' | 'uri' | 'date' | 'date-time'
  minimum?: number | null
  maximum?: number | null
  minItems?: number | null
  maxItems?: number | null
}

/** A field's starting value: the server's default, else empty. */
export function initialValue(f: FormField): unknown {
  if (f.default !== undefined && f.default !== null) {
    if (f.kind === 'text' && f.format === 'date-time' && typeof f.default === 'string') return toLocalInput(f.default)
    return Array.isArray(f.default) ? [...f.default] : f.default
  }
  return f.kind === 'multi' ? [] : f.kind === 'boolean' ? null : ''
}

/** `2026-11-02T14:30:00Z` → the `datetime-local` input's local `2026-11-02T15:30`. */
export function toLocalInput(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`
}

function blank(v: unknown): boolean {
  return v === null || v === undefined || v === '' || (Array.isArray(v) && v.length === 0)
}

/** What the backend checks again: the first problem with a field's value. */
export function fieldError(f: FormField, v: unknown): string | null {
  if (blank(v)) return f.required ? 'Required' : null
  switch (f.kind) {
    case 'text': {
      const s = String(v)
      const n = [...s].length
      if (f.minLength != null && n < f.minLength) return `At least ${f.minLength} characters`
      if (f.maxLength != null && n > f.maxLength) return `At most ${f.maxLength} characters`
      if (f.format === 'email' && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(s)) return 'Not an email address'
      if (f.format === 'uri') {
        try {
          new URL(s)
        } catch {
          return 'Not a web address'
        }
      }
      if (f.format === 'date' && !/^\d{4}-\d{2}-\d{2}$/.test(s)) return 'Not a date'
      if (f.format === 'date-time' && Number.isNaN(new Date(s).getTime())) return 'Not a date and time'
      return null
    }
    case 'number':
    case 'integer': {
      const n = typeof v === 'number' ? v : Number(v)
      if (!Number.isFinite(n)) return 'Not a number'
      if (f.kind === 'integer' && !Number.isInteger(n)) return 'A whole number'
      if (f.minimum != null && n < f.minimum) return `At least ${f.minimum}`
      if (f.maximum != null && n > f.maximum) return `At most ${f.maximum}`
      return null
    }
    case 'choice':
      return f.options?.some((o) => o.value === v) ? null : 'Pick one of the options'
    case 'multi': {
      const list = Array.isArray(v) ? v : []
      if (f.minItems != null && list.length < f.minItems) return `Pick at least ${f.minItems}`
      if (f.maxItems != null && list.length > f.maxItems) return `Pick at most ${f.maxItems}`
      return null
    }
    case 'boolean':
      return typeof v === 'boolean' ? null : 'Yes or no'
  }
}

/** The form's answer as sent: typed values, blank optional fields left out. */
export function formContent(fields: FormField[], values: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const f of fields) {
    const v = values[f.key]
    if (blank(v)) continue
    if (f.kind === 'number' || f.kind === 'integer') out[f.key] = typeof v === 'number' ? v : Number(v)
    else if (f.kind === 'text' && f.format === 'date-time') out[f.key] = new Date(String(v)).toISOString()
    else out[f.key] = v
  }
  return out
}
