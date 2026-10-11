import type { PortfolioCashEntry } from '../types/portfolio'

export type PortfolioEditor = {
  kind: 'asset' | 'trade' | 'cash'
  assetId?: string
  tradeId?: string
  cashEntryId?: string
  cashAction?: PortfolioCashEntry['kind']
  side?: 'buy' | 'sell'
  initialEntry?: 'opening' | 'buy'
  focusNotes?: boolean
}

// Shared by the kept-alive page and its layout panel; never persisted as portfolio data.
export function usePortfolioPanel() {
  const view = useState<'holdings' | 'history' | 'cash'>('terminal-portfolio-view', () => 'holdings')
  const selectedHolding = useState<string>('terminal-portfolio-selection', () => '')
  const selectedAsset = useState<string>('terminal-portfolio-history-asset', () => '')
  const selectedKind = useState<string>('terminal-portfolio-history-kind', () => '')
  const editor = useState<PortfolioEditor | null>('terminal-portfolio-editor', () => null)
  const deleting = useState<{ kind: 'asset' | 'trade' | 'cash'; id: string; name: string } | null>('terminal-portfolio-delete', () => null)
  function historyFor(id: string) {
    selectedAsset.value = id
    selectedKind.value = ''
    view.value = 'history'
  }
  return { view, selectedHolding, selectedAsset, selectedKind, editor, deleting, historyFor }
}
