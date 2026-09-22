export type Theme = 'dark' | 'light' | 'system'

export type SystemId = string

export type SystemStatus = 'ready' | 'planned'

export interface SystemMeta {
  id: SystemId
  name: string
  short: string
  status: SystemStatus
  description: string
}

export interface AppSettings {
  theme: Theme
  fontSize: number
  activeSystemId: SystemId
}

export type Cadence = '1m' | '1h' | '4h' | '12h' | 'daily' | 'weekly' | 'monthly'
export type RankingSource = 'cmc' | 'local' | 'auto'
export type PriceSource = 'open' | 'high' | 'low' | 'close' | 'hl2' | 'hlc3' | 'ohlc4'

export interface EmaCrossConfig {
  src: PriceSource
  fastLength: number
  slowLength: number
}

/** Pairwise trend-signal engine. 'ema_cross' is the classic 12/21 band
 *  cross; the rest are Smithery indicators running at their
 *  gauntlet-certified defaults. One score per indicator — the worst of its
 *  1d / 4h / 1h tracks; certified on every track as of 2026-09-08:
 *  rankbreak, ensemble, scale, extremes, bocpd (smithery.registry). */
export type TrendKind =
  | 'aroon'
  | 'dmi'
  | 'vortex'
  | 'tsi'
  | 'hull'
  | 'alma'
  | 'frama'
  | 'vidya'
  | 'regression'
  | 'median_mad'
  | 'bollinger_trend'
  | 'keltner_risk'
  | 'ichimoku'
  | 'rsi_trend'
  | 'stochastic_trend'
  | 'chandelier'
  | 'roc_trend'
  | 'efficiency_breakout'
  | 'bq_adaptive_envelope'
  | 'lyro_rmd'
  | 'supertrend' | 'donchian' | 'kama_band' | 'ema_atr' | 'dual_momentum'
  | 'defensive_trend' | 'bq_volatility_gate' | 'lyro_ha'
  | 'ema_cross' | 'kalman' | 'hmm' | 'wavelet' | 'hilbert' | 'vratio' | 'council'
  | 'sprt' | 'page' | 'bocpd' | 'fractal' | 'slopes' | 'extremes' | 'ordinal'
  | 'imm' | 'ensemble' | 'mk' | 'bvc' | 'dc' | 'rkalman' | 'runs' | 'council2'
  | 'dcl' | 'bretrace' | 'consensus' | 'rankbreak'
  | 'scale' | 'swing' | 'consensus2'
  | 'ensemble_original' | 'ensemble_wf' | 'council2_original' | 'scale_original' | 'robust'
  /** The average of the `IndicatorConfig.aggregate` members' signals. */
  | 'aggregate'

export interface IndicatorConfig {
  trend: TrendKind
  emaCross: EmaCrossConfig
  /** Members of the `aggregate` trend: their ±1 signals are averaged, the
   *  pair is bullish above 0, bearish below 0, unchanged at exactly 0. */
  aggregate: TrendKind[]
}

/** A market-only regime; never participates in the coin ranking. */
export interface TotalBreakoutConfig {
  trend: 'total_breakout'
  trendLength: number
  entryLength: number
  exitLength: number
}

export type MarketIndicatorConfig = IndicatorConfig | TotalBreakoutConfig

/** One row of the indicator pickers: a trend kind with its Smithery verdict
 *  on the score track of the current cadence. `score` is null for the EMA
 *  cross and for indicators without a verdict on that track. */
export interface IndicatorOption {
  value: TrendKind
  name: string
  score: number | null
  grade: string | null
  /** Provenance of the score ("1d certified", "1d reference", "research"). */
  tag?: string
}

export interface RunConfig {
  topN: number
  excludeTopN: number
  cadence: Cadence
  startDate: string
  endDate: string
  rankingSource: RankingSource
  excludeStablecoins: boolean
  excludeWrapped: boolean
  includeUsd: boolean
  feeRate: number
  slippageRate: number
  minRequestInterval: number
  indicator: IndicatorConfig
  /** Further trend signals a backtest runs next to `indicator`, on the same
   *  universe and candles, so their curves and metrics sit side by side. */
  compareTrends: TrendKind[]
  /** Positions only while TOTAL is bullish, otherwise USD. */
  marketFilter: boolean
  /** Independent TOTAL signal. Null/missing preserves each run's own signal. */
  marketIndicator?: MarketIndicatorConfig | null
}

export interface EngineStatus {
  status: 'running' | 'stopped'
  pid?: number | null
}

export interface RankedCoin {
  rank: number
  cgId: string | null
  symbol: string
  name: string | null
  marketCap: number | null
  price: number | null
  excluded?: boolean
  score?: number | null
  hasData?: boolean
}

export interface LiveResult {
  asOf: string
  provider: string
  universe: RankedCoin[]
  symbols: string[]
  scoreMatrix: (number | null)[][]
  scores: Record<string, number>
  best: string | null
  /** The higher filter as of `asOf`; `bullish` is null while it is off. */
  marketFilter?: { enabled: boolean; bullish: boolean | null }
}

export interface EquityPoint {
  // `YYYY-MM-DD` for daily bars, or a UNIX timestamp (seconds, UTC) for intraday.
  time: string | number
  value: number
}

export interface PerformanceMetrics {
  meanAllPct: number | null
  meanPosPct: number | null
  meanNegPct: number | null
  stddevAllPct: number | null
  stddevPosPct: number | null
  stddevNegPct: number | null
  sharpe: number | null
  sortino: number | null
  omega: number | null
  maxDrawdownPct: number | null
  netReturnMultiplier: number | null
}

/** The rotation simulated with one trend signal. */
export interface StrategyRun {
  key: TrendKind
  /** Engine label: the Smithery class name, or `EMA 12/21`. */
  label: string
  equityStrategy: EquityPoint[]
  heldAsset: { time: string; symbol: string | null }[]
  metricsStrategy: PerformanceMetrics
  forcedRotations: string[]
}

export interface BacktestResult {
  /** Successful runs in requested order. An empty array means all skipped.
   *  Absent only on legacy engines; scalar fields represent the configured
   *  primary and remain empty when it could not be evaluated. */
  strategies?: StrategyRun[]
  /** Variants that could not run with the available indicator data. */
  skippedStrategies?: { key: TrendKind; label: string; reason: string }[]
  equityStrategy: EquityPoint[]
  heldAsset: { time: string; symbol: string | null }[]
  buyAndHold: Record<string, EquityPoint[]>
  benchmarks: Record<string, EquityPoint[]>
  metricsStrategy: PerformanceMetrics
  metricsBuyAndHold: Record<string, PerformanceMetrics>
  metricsBenchmarks: Record<string, PerformanceMetrics>
  metricLabels: string[]
  forcedRotations: string[]
  notes: string[]
  coins: RankedCoin[]
}

export interface CacheStats {
  path: string
  sizeBytes: number
  rankings: number
  ohlcv: number
  coins: number
}

export type EngineView = 'settings' | 'live' | 'backtest'
