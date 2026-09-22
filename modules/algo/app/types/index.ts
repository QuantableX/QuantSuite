// ── Strategy ──

export interface Strategy {
  id: string
  name: string
  description: string
  file_path: string
  params_json: string | null
  created_at: string
  updated_at: string
}

// ── Indicators (the Indicator Smithery's registry) ──

export interface IndicatorCertification {
  /** Robustness Score 0–100 — for the overall verdict, the worst track's. */
  score: number
  grade: string
  /** Permutation-test p-value; > 0.10 caps the grade at B. For the overall verdict, the worst track's. */
  perm_p: number | null
  date: string
  /** The report note in the vault's Output/ folder (the worst track's, for the overall verdict). */
  report: string | null
  /** Score ≥ 70 and permutation p ≤ 0.10 — on every track for the overall verdict. */
  certified: boolean
  /** The luck gate failed somewhere: the grade is capped at B. */
  capped?: boolean
  /** Overall verdict only: how many of the tracks have a verdict, and which one is the worst. */
  tracks_run?: number
  tracks?: number
  worst_track?: string
  timeframe?: string
  source?: 'historical' | 'current_run'
  reasons?: string[]
}

export interface IndicatorInfo {
  /** Registry key — what a strategy passes to `self.regime(key)`. */
  key: string
  name: string
  hypothesis: string
  /** Default (certified) hyper-parameters. */
  params: Record<string, unknown>
  /** The region the gauntlet perturbs, per dimensionless parameter. */
  param_space: Record<string, [number, number]>
  warmup_bars: number
  /** THE verdict: the worst track's score, certified only on every track. */
  certification: IndicatorCertification | null
  /** Every certification track — `1d`, `4h`, `1h` — with its own verdict or null. */
  timeframes?: Record<string, (IndicatorCertification & { timeframe: string }) | null>
}

export interface IndicatorRegistry {
  generated_at: string
  package_dir: string
  /** The certification tracks, `1d` first. */
  timeframes?: string[]
  indicators: IndicatorInfo[]
}

/** A certification track, or `all` — every track in turn (gauntlet only). */
export type SmitheryTimeframe = 'all' | '1d' | '4h' | '1h' | '1m'

// ── The Smithery: the forge inside QuantAlgo (PLAN-QUANTALGO §6) ──

/** One cached series of the price shelf (`<EXCHANGE>_<SYMBOL>_<TF>.csv`). */
export interface SmitheryShelfSeries {
  key: string
  exchange: string
  symbol: string
  timeframe: string
  file: string
  size_bytes: number
  modified: string
  bars: number
  first: string | null
  last: string | null
}

/** A gauntlet report in the vault's Output folder, as its header reads. */
export interface SmitheryReportMeta {
  name: string
  file: string
  date: string | null
  indicator: string | null
  /** The certification track the report belongs to (`1d` unless the name says otherwise). */
  timeframe: string
  /** Reduced Monte Carlo counts — a smoke test, never a certification. */
  fast: boolean
  score: number | null
  grade: string | null
  perm_p: number | null
  runtime_s: number | null
  params: string | null
  certified: boolean | null
  modified?: string
}

/** `python -m smithery.forge info`: where the vault is and what it holds. */
export interface SmitheryInfo {
  version: string
  python: string
  package_dir: string
  root: string
  root_exists: boolean
  /** `env` = $SMITHERY_VAULT · `vault` = the Obsidian vault · `private` = ~/.quantsuite/modules/algo/smithery */
  location: 'env' | 'vault' | 'private'
  price_dir: string
  output_dir: string
  docs_dir: string
  ledger: string
  indicators: string[]
  certified: string[]
  /** The tracks the shelf can run — those with a BTC primary series. */
  timeframes?: string[]
  certified_by_timeframe?: Record<string, string[]>
  /** Supported tracks, including those whose shelf has not been fetched yet. */
  supported_timeframes?: string[]
  workers: number
  shelf: SmitheryShelfSeries[]
  reports: SmitheryReportMeta[]
}

export type ForgeKind = 'gauntlet' | 'walkforward' | 'compare' | 'refresh'

export interface ForgeRequest {
  kind: ForgeKind
  /** Registry keys, or `all` / `certified`. */
  indicators: string[]
  /** Reduced Monte Carlo counts — a smoke test, never a certification. */
  fast: boolean
  perm?: number | null
  boot?: number | null
  garch?: number | null
  seed?: number | null
  folds?: number | null
  /** Certification track; the daily reference track when absent. */
  timeframe?: SmitheryTimeframe | null
}

/** One JSON line of a forge job: `job` · `begin` · `contract` · `axis` · `verdict` · `fold` · `walkforward` · `series` · `log` · `error` · `done`. */
export interface ForgeEvent {
  event: string
  indicator?: string
  [key: string]: unknown
}

export type ForgeJobStatus = 'running' | 'done' | 'failed' | 'cancelled'

export interface ForgeJob {
  id: string
  kind: ForgeKind
  indicators: string[]
  request: ForgeRequest
  status: ForgeJobStatus
  started_at: string
  finished_at: string | null
  exit_code: number | null
  error: string | null
  /** Every event, in order — only with the running job and the one asked for in detail. */
  events: ForgeEvent[]
  /** Verdicts, walk-forward results, refreshed series, errors — always present. */
  summary: ForgeEvent[]
  /** Plain stdout/stderr lines (stderr prefixed `! `). */
  log: string[]
  command: string
}

// ── Trade ──

export type TradeSide = 'long' | 'short'

export interface Trade {
  id: string
  /** The bot that made the trade; null on backtest rows and single-bot-era rows. */
  bot_id?: string | null
  /** The bot's mode when the row was read (single-bot-era rows count as paper); absent on backtest rows. */
  trading_mode?: TradingMode | null
  strategy_id: string
  exchange: string
  pair: string
  side: TradeSide
  entry_price: number
  exit_price: number | null
  quantity: number
  entry_time: string
  exit_time: string | null
  pnl: number | null
  pnl_pct: number | null
  fee: number
  is_backtest: boolean
  backtest_id: string | null
  notes: string | null
  created_at: string
}

export interface TradeFilters {
  bot_id?: string
  /** paper | live — backtest rows never match. */
  trading_mode?: TradingMode
  strategy_id?: string
  exchange?: string
  pair?: string
  side?: TradeSide
  /** Bound against `entry_time`. */
  from_date?: string
  to_date?: string
  /** Bound against `exit_time` — trades still open drop out entirely. */
  exited_from?: string
  exited_to?: string
  min_pnl?: number
  is_backtest?: boolean
  backtest_id?: string
  limit?: number
  offset?: number
}

/** Distinct exchange / pair values in the journal — the filter dropdowns' options. */
export interface TradeFacets {
  /** Connected exchange ids on bot trades, provider names on old backtest rows. */
  exchanges: string[]
  pairs: string[]
}

export interface TradeStats {
  total_trades: number
  win_rate: number
  avg_win: number
  avg_loss: number
  profit_factor: number
  expectancy: number
  best_trade: number
  worst_trade: number
  total_pnl: number
  total_pnl_pct: number
  avg_duration_secs: number
}

/** One bot's share of `get_trade_stats_breakdown`; `bot_id` is null for the catch-all groups. */
export interface BotTradeStats {
  bot_id: string | null
  name: string
  trading_mode: TradingMode
  stats: TradeStats
}

/** The journal's statistics split by mode and by bot. */
export interface TradeStatsBreakdown {
  paper: TradeStats
  live: TradeStats
  bots: BotTradeStats[]
}

/** One calendar day of realised PnL in the user's zone (`get_daily_pnl`). */
export interface DailyPnl {
  date: string
  pnl: number
  trades: number
}

// ── Backtest ──

export interface BacktestConfig {
  strategy_id: string
  /** The data provider (`binance`, …) — derived from the connected exchange. */
  exchange: string
  /** The connected exchange the candles come from; the form only offers these. */
  exchange_id?: string
  pair: string
  timeframe: string
  start_date: string
  end_date: string
  initial_capital: number
  commission: number
  strategy_params?: Record<string, unknown>
}

export interface BacktestStats {
  total_return: number
  total_return_pct: number
  sharpe_ratio: number
  max_drawdown: number
  max_drawdown_pct: number
  win_rate: number
  profit_factor: number
  total_trades: number
  avg_trade_duration_secs: number
}

export interface EquityPoint {
  time: string
  equity: number
}

export interface BacktestResult {
  id: string
  name: string
  strategy_id: string
  config: BacktestConfig
  stats: BacktestStats
  equity_curve: EquityPoint[]
  trades: Trade[]
  created_at: string
}

export interface BacktestSummary {
  id: string
  name: string
  strategy_id: string
  config_json: string
  stats_json: string
  created_at: string
}

// ── Exchange ──

export type ExchangeType = 'cex' | 'dex'

export type ExchangeProvider =
  | 'binance' | 'bybit' | 'okx' | 'coinbase' | 'kraken' | 'kucoin'
  | 'uniswap' | 'jupiter' | 'hyperliquid'

export interface Exchange {
  id: string
  name: string
  exchange_type: ExchangeType
  provider: ExchangeProvider
  is_active: boolean
  /** Private calls go to the venue's testnet / demo environment (Binance, Bybit). */
  sandbox: boolean
  created_at: string
  updated_at: string
}

export interface ExchangeConfig {
  name: string
  exchange_type: ExchangeType
  provider: ExchangeProvider
  sandbox?: boolean
  api_key?: string
  api_secret?: string
  passphrase?: string
  wallet_address?: string
  private_key?: string
  rpc_endpoint?: string
}

export interface ConnectionResult {
  success: boolean
  message: string
  latency_ms?: number
}

export interface Balance {
  asset: string
  total: number
  available: number
  in_positions: number
}

// ── Bots (PLAN-QUANTALGO §3) ──

export type BotStatusType = 'running' | 'stopped' | 'error'
export type TradingMode = 'paper' | 'live'

/** One bot row: a strategy on a connected exchange, pair, timeframe and mode. */
export interface Bot {
  id: string
  name: string
  strategy_id: string
  exchange_id: string
  pair: string
  timeframe: string
  trading_mode: TradingMode
  /** Quote-currency cash the bot may use. */
  budget: number
  status: BotStatusType
  started_at: string | null
  stopped_at: string | null
  last_error: string | null
  config_json: string | null
  created_at: string
  updated_at: string
}

/** A bot row plus what its runtime says right now (`list_bots`). */
export interface BotSnapshot extends Bot {
  equity: number
  balance: number
  last_price: number
  open_positions: number
  process_alive: boolean
}

/** What the create form sends (`create_bot`, `validate_bot_deploy { draft }`). */
export interface BotDraft {
  name?: string | null
  strategy_id: string
  exchange_id: string
  pair: string
  timeframe: string
  trading_mode: TradingMode
  budget: number
  config?: Record<string, unknown> | null
}

export interface BotStatus {
  status: BotStatusType
  strategy_id: string | null
  exchange_id: string | null
  pair: string | null
  started_at: string | null
  config_json: string | null
  trading_mode: TradingMode
}

export interface LogEntry {
  timestamp: string
  level: 'info' | 'trade' | 'warn' | 'error'
  message: string
  /** The bot the line belongs to; module-level lines have none. */
  bot_id?: string | null
}

// ── Deploy / Preflight ──

export interface DeployConfig {
  strategy_id: string
  exchange_id: string
  pair: string
  trading_mode: TradingMode
  timeframe: string
  initial_balance: number
  risk_per_trade: number
  max_positions: number
  slippage: number
  fee: number
}

export type PreflightSeverity = 'ok' | 'warn' | 'error'

export interface PreflightCheck {
  id: string
  label: string
  status: PreflightSeverity
  message: string
}

export interface PreflightResult {
  checks: PreflightCheck[]
  can_start: boolean
}

// ── Settings ──

export interface AppSettings {
  theme: 'dark' | 'light'
  font_size: number
  /** The connected exchange every exchange dropdown starts on. */
  default_exchange_id: string | null
  default_pair: string
  default_timeframe: string
  python_path: string
  strategy_dir: string
  backtest_dir: string
  risk_per_trade: number
  max_concurrent_positions: number
  slippage_tolerance: number
  paper_fee_pct: number
  /** A new bot's budget; the backtest starts with the same capital. */
  default_budget: number
  default_warmup_candles: number
  /** Which set of bot defaults the config has received; sent back untouched. */
  defaults_version: number
  notify_on_trade: boolean
  notify_on_error: boolean
  notify_on_daily_summary: boolean
}

/**
 * What a new bot and a backtest start from: the Settings values with their
 * fallbacks (`useAppStore().botDefaults`) — the one source the create modal,
 * the backtest form and the Settings page read, so they can never disagree.
 */
export interface BotDefaults {
  exchange_id: string | null
  pair: string
  timeframe: string
  budget: number
  risk_per_trade: number
  max_positions: number
  slippage: number
  fee: number
  warmup_candles: number
}

// ── Events ──

export interface BotLogEvent {
  timestamp: string
  level: string
  message: string
  bot_id?: string | null
}

export interface BotTradeEvent {
  bot_id?: string | null
  trade: Trade
}

export interface BotStatusEvent {
  bot_id?: string
  status: BotStatusType
  strategy_id: string | null
  exchange_id: string | null
  pair: string | null
  started_at: string | null
  trading_mode: TradingMode
  last_error?: string | null
}

export interface BotEquityEvent {
  bot_id?: string
  timestamp: string
  equity: number
  last_price?: number
  pair?: string
  balance?: number
  open_position_count?: number
  trading_mode?: TradingMode
}

export interface BotErrorEvent {
  bot_id?: string | null
  message: string
  details: string | null
}

export interface BacktestProgressEvent {
  pct: number
  message: string
}

export interface BacktestCompleteEvent {
  result: BacktestResult
}
