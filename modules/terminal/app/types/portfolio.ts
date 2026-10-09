export interface PortfolioAsset {
  id: string
  symbol: string
  name: string
  source: 'manual' | 'market'
  marketName: string
  marketPage: number
  manualPrice: number | null
  updatedAt: string
}
export interface PortfolioTrade {
  id: string
  assetId: string
  kind: 'opening' | 'buy' | 'sell'
  date: string
  quantity: number
  price: number
  fees: number
  notes: string
  order: number
}
export interface PortfolioQuote { price: number; checkedAt: string }
export interface PortfolioSnapshot { at: string; prices: Record<string, number> }
export interface PortfolioDocument {
  version: 1
  revision: string
  assets: PortfolioAsset[]
  trades: PortfolioTrade[]
  quotes: Record<string, PortfolioQuote>
  snapshots: PortfolioSnapshot[]
}
export interface PortfolioHolding {
  asset: PortfolioAsset
  quantity: number
  cost: number
  averageCost: number
  realized: number
  price: number | null
  value: number | null
  unrealized: number | null
}
