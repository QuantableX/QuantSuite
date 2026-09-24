import type { SystemMeta } from '#systems/types'

/** Stable keys are persisted; only these local SVG paths are rendered. */
export const STRATEGY_ICONS = [
  { id: 'grid', label: 'Grid', path: 'M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z' },
  { id: 'layers', label: 'Layers', path: 'M12 2l9 5-9 5-9-5 9-5z M3 12l9 5 9-5' },
  { id: 'hexagon', label: 'Hexagon', path: 'M12 3l7 4v10l-7 4-7-4V7l7-4z' },
  { id: 'diamond', label: 'Diamond', path: 'M12 3l9 9-9 9-9-9 9-9z' },
  { id: 'circle', label: 'Circle', path: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z' },
  { id: 'triangle', label: 'Triangle', path: 'M12 3l10 18H2L12 3z' },
  { id: 'square', label: 'Square', path: 'M4 4h16v16H4z' },
  { id: 'target', label: 'Target', path: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0z' },
  { id: 'cross', label: 'Cross', path: 'M9 3h6v6h6v6h-6v6H9v-6H3V9h6V3z' },
  { id: 'bars', label: 'Bars', path: 'M5 20V10h3v10H5z M11 20V4h3v16h-3z M17 20v-7h3v7h-3z' },
  { id: 'wave', label: 'Wave', path: 'M2 12h4l3-8 6 16 3-8h4' },
  { id: 'orbit', label: 'Orbit', path: 'M20 12a8 8 0 1 1-8-8 M22 5a3 3 0 1 1-6 0 3 3 0 0 1 6 0z' },
] as const

export function strategyIcon(system?: Pick<SystemMeta, 'id' | 'icon'>) {
  const fallback = system?.id === 'lces' ? 'layers' : system?.id === 'sces' ? 'hexagon' : 'grid'
  return STRATEGY_ICONS.find(icon => icon.id === (system?.icon || fallback)) ?? STRATEGY_ICONS[0]
}
