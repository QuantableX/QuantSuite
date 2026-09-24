import type { Cadence, MarketContext, RunConfig, SingleAssetConfig } from '#systems/types'

export function defaultSingleAsset(): SingleAssetConfig {
  return { exchange: 'coinbase', pair: 'BTC/USD', timeframe: '1d', direction: 'long_cash' }
}

export function effectiveCadence(config: RunConfig): Cadence {
  if (config.mode !== 'single_asset') return config.cadence
  return config.singleAsset.timeframe === '1d' ? 'daily' : config.singleAsset.timeframe
}

/** A previous rotation or another pair must never look like the current result. */
export function matchesMarket(result: MarketContext, config: RunConfig): boolean {
  if ((result.mode ?? 'rotation') !== config.mode) return false
  if (config.mode !== 'single_asset') return true
  return !!result.singleAsset && (['exchange', 'pair', 'timeframe', 'direction'] as const)
    .every(key => result.singleAsset![key] === config.singleAsset[key])
}
