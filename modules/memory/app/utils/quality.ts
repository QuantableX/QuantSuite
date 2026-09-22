import type { MemoryQuality } from '#memory/types'

/** Number inputs can emit numbers even without v-model.number. Normalize once. */
export function qualityFromDraft(draft: {
  basis: MemoryQuality['basis']; confidence: string | number; sources: string;
  conflicts: string; replacement: string; lastVerified: string | null;
}): MemoryQuality {
  const confidenceText = String(draft.confidence).trim()
  const confidence = confidenceText ? Number(confidenceText) : null
  if (confidence !== null && (!Number.isFinite(confidence) || confidence < 0 || confidence > 1)) {
    throw new Error('Confidence must be between 0 and 1.')
  }
  const lines = (value: string) => [...new Set(value.split('\n').map(s => s.trim()).filter(Boolean))]
  const sources = lines(draft.sources)
  if (draft.basis === 'observed' && !sources.length) throw new Error('Observed memories need a source.')
  return { basis: draft.basis, confidence, sources, conflictsWith: lines(draft.conflicts), supersededBy: draft.replacement || null, lastVerified: draft.lastVerified, reviewed: false }
}
