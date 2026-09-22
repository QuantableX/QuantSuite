export type MetricsSourceFilter = 'all' | 'exchange' | 'public' | 'derived'
export type MetricsFavoriteFilter = 'all' | 'favorites' | 'non-favorites'
export type MetricsToneFilter = 'all' | 'bullish' | 'neutral' | 'caution' | 'bearish'

interface MetricsFiltersState {
  search: string
  source: MetricsSourceFilter
  favorite: MetricsFavoriteFilter
  tone: MetricsToneFilter
  symbol: string
  category: string
}

interface FilterOption<T extends string = string> {
  label: string
  value: T
}

const DEFAULT_FILTERS: MetricsFiltersState = {
  search: '',
  source: 'all',
  favorite: 'all',
  tone: 'all',
  symbol: 'all',
  category: 'all',
}

const sourceOptions: FilterOption<MetricsSourceFilter>[] = [
  { label: 'All sources', value: 'all' },
  { label: 'Exchange observations', value: 'exchange' },
  { label: 'Public observations', value: 'public' },
  { label: 'Calculated from observations', value: 'derived' },
]

const favoriteOptions: FilterOption<MetricsFavoriteFilter>[] = [
  { label: 'Show all', value: 'all' },
  { label: 'Favorites', value: 'favorites' },
  { label: 'Non-favorites', value: 'non-favorites' },
]

const toneOptions: FilterOption<MetricsToneFilter>[] = [
  { label: 'All readings', value: 'all' },
  { label: 'Contextual / neutral', value: 'neutral' },
  { label: 'Elevated / caution', value: 'caution' },
  { label: 'Extreme', value: 'bearish' },
]

const symbolOptions: FilterOption[] = [
  { label: 'All assets', value: 'all' },
  { label: 'BTC', value: 'BTC' },
  { label: 'BTC Perps', value: 'BTC Perps' },
  { label: 'BTC Futures', value: 'BTC Futures' },
  { label: 'BTC Options', value: 'BTC Options' },
  { label: 'Stablecoins', value: 'Stablecoins' },
  { label: 'All Chains', value: 'All Chains' },
  { label: 'All Protocols', value: 'All Protocols' },
  { label: 'DeFi Yields', value: 'DeFi Yields' },
  { label: 'DeFi Credit', value: 'DeFi Credit' },
  { label: 'Projects', value: 'Projects' },
  { label: 'ERC20', value: 'ERC20' },
]

const categoryOptions: FilterOption[] = [
  { label: 'All categories', value: 'all' },
  { label: 'Age Distribution', value: 'Age Distribution' },
  { label: 'Coin Age', value: 'Coin Age' },
  { label: 'Concentration', value: 'Concentration' },
  { label: 'Cost Basis', value: 'Cost Basis' },
  { label: 'Cycle Risk', value: 'Cycle Risk' },
  { label: 'DeFi', value: 'DeFi' },
  { label: 'Derivatives', value: 'Derivatives' },
  { label: 'Ecosystem', value: 'Ecosystem' },
  { label: 'Exchange Flows', value: 'Exchange Flows' },
  { label: 'Holder Behavior', value: 'Holder Behavior' },
  { label: 'Holder Cohorts', value: 'Holder Cohorts' },
  { label: 'Holder Profitability', value: 'Holder Profitability' },
  { label: 'Liquidations', value: 'Liquidations' },
  { label: 'Liquidity', value: 'Liquidity' },
  { label: 'Miner Data', value: 'Miner Data' },
  { label: 'Miner Flows', value: 'Miner Flows' },
  { label: 'Miner Revenue', value: 'Miner Revenue' },
  { label: 'Network Activity', value: 'Network Activity' },
  { label: 'Network Value', value: 'Network Value' },
  { label: 'Options', value: 'Options' },
  { label: 'Realized PnL', value: 'Realized PnL' },
  { label: 'Regional Premium', value: 'Regional Premium' },
  { label: 'Sentiment', value: 'Sentiment' },
  { label: 'Spent Profit', value: 'Spent Profit' },
  { label: 'Stablecoin Flows', value: 'Stablecoin Flows' },
  { label: 'Stablecoin Liquidity', value: 'Stablecoin Liquidity' },
  { label: 'UTXO Profitability', value: 'UTXO Profitability' },
  { label: 'Valuation', value: 'Valuation' },
  { label: 'Volatility', value: 'Volatility' },
  { label: 'Whale Activity', value: 'Whale Activity' },
  { label: 'Whale Flow', value: 'Whale Flow' },
]

function createDefaultFilters(): MetricsFiltersState {
  return { ...DEFAULT_FILTERS }
}

export function useMetricsFilters() {
  const filters = useState<MetricsFiltersState>('metrics-filters', createDefaultFilters)
  const favoriteIds = useState<string[]>('metrics-favorite-ids', () => [])

  const favoriteSet = computed(() => new Set(favoriteIds.value))
  const favoriteCount = computed(() => favoriteIds.value.length)
  const activeFilterCount = computed(() => {
    let count = 0
    if (filters.value.search.trim()) count += 1
    if (filters.value.source !== 'all') count += 1
    if (filters.value.favorite !== 'all') count += 1
    if (filters.value.tone !== 'all') count += 1
    if (filters.value.symbol !== 'all') count += 1
    if (filters.value.category !== 'all') count += 1
    return count
  })

  function isFavorite(id: string) {
    return favoriteSet.value.has(id)
  }

  function toggleFavorite(id: string) {
    if (favoriteSet.value.has(id)) {
      favoriteIds.value = favoriteIds.value.filter(item => item !== id)
      return
    }

    favoriteIds.value = [...favoriteIds.value, id]
  }

  function resetFilters() {
    filters.value = createDefaultFilters()
  }

  return {
    filters,
    favoriteIds,
    favoriteCount,
    activeFilterCount,
    isFavorite,
    toggleFavorite,
    resetFilters,
    sourceOptions,
    favoriteOptions,
    toneOptions,
    symbolOptions,
    categoryOptions,
  }
}
