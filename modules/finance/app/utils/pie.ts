import type { SankeyData } from '../types'
import type { Fund } from '../types/funds'

export interface PieSlice {
  id: string
  label: string
  valueCents: number
  color: string
  detail?: string
  fundId?: string
}

/** Only destinations belong in the pie: income and the hub are the same money. */
export function flowPieSlices(data: SankeyData | null): PieSlice[] {
  return (data?.nodes ?? [])
    .filter((node) => node.layer === 2 && node.valueCents > 0)
    .map((node) => ({
      id: node.id,
      label: node.label,
      valueCents: node.valueCents,
      color: `var(--qf-flow-${node.color})`,
      detail: node.kind === 'saving' ? 'Saving' : node.kind === 'expense' ? 'Fixed' : 'Unallocated',
      fundId: node.fundId ?? undefined,
    }))
}

/** Identity-based colours stay with a fund when its value, name or order changes. */
export function fundPieSlices(funds: Fund[]): PieSlice[] {
  return funds.map((fund) => {
    let hash = 0
    for (const char of fund.id) hash = (Math.imul(hash, 31) + char.charCodeAt(0)) >>> 0
    return {
      id: fund.id,
      fundId: fund.id,
      label: fund.name,
      valueCents: fund.currentValueCents,
      color: `hsl(${hash % 360} 52% 57%)`,
    }
  })
}

/** Two arcs also render a single 100% slice as a complete circle. */
export function layoutPie(slices: PieSlice[]) {
  const positive = slices.filter((slice) => Number.isFinite(slice.valueCents) && slice.valueCents > 0)
  const total = positive.reduce((sum, slice) => sum + slice.valueCents, 0)
  let cumulative = 0
  const point = (fraction: number) => {
    const angle = fraction * Math.PI * 2 - Math.PI / 2
    return `${100 + 90 * Math.cos(angle)} ${100 + 90 * Math.sin(angle)}`
  }
  const segments = positive.map((slice) => {
    const start = cumulative / total
    cumulative += slice.valueCents
    const end = cumulative / total
    const middle = (start + end) / 2
    return {
      ...slice,
      path: positive.length === 1
        ? 'M 100 10 A 90 90 0 0 1 100 190 A 90 90 0 0 1 100 10 Z'
        : `M 100 100 L ${point(start)} A 90 90 0 0 1 ${point(middle)} A 90 90 0 0 1 ${point(end)} Z`,
    }
  })
  return { total, segments }
}
