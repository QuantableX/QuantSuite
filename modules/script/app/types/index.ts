// ── The registry's view of a script (python -m smithery.quantscript list) ──

export interface IndicatorCertification {
  /** Robustness Score 0–100 — for the overall verdict, the worst track's. */
  score: number
  grade: string
  perm_p: number | null
  date: string
  report: string | null
  /** Score ≥ 70 and permutation p ≤ 0.10 — on every track for the overall verdict. */
  certified: boolean
  capped?: boolean
  tracks_run?: number
  tracks?: number
  worst_track?: string
  timeframe?: string
  /** `release` = the evidence a version file carries (smithery/variants.py). */
  source?: 'historical' | 'current_run' | 'release'
  reasons?: string[]
}

/**
 * An indicator's fixed version slots (smithery/variants.py): `standard` is the
 * base itself, `optimized` the general optimization (`<base>_opt`), the
 * timeframe roles `<base>_opt_1h` / `_opt_4h` / `_opt_1d`.
 */
export type VersionRole = 'standard' | 'optimized' | 'optimized_1h' | 'optimized_4h' | 'optimized_1d'

/** role → registry key, null while the slot is empty. */
export type IndicatorVersions = Record<VersionRole, string | null>

/** How an editor presents one parameter (contract.TrendIndicator.schema()). */
export interface ParamSchemaEntry {
  type: 'int' | 'float' | 'bool' | 'choice' | 'list'
  default: unknown
  label: string
  help: string
  /** Hard bounds the script declares. */
  min?: number
  max?: number
  step?: number
  choices?: unknown[]
  /** The range the gauntlet perturbs — evidence exists inside it. */
  tested?: [number, number]
}

/** One top-level class of a script — a registered indicator when `key` is set. */
export interface ScriptClass {
  class_name: string
  line: number
  bases: string[]
  /** Registry key; null for a base class, a helper or an unregistered class. */
  key: string | null
  name?: string
  hypothesis?: string
  params?: Record<string, unknown>
  param_space?: Record<string, [number, number]>
  schema?: Record<string, ParamSchemaEntry>
  /** `standard` for a base; the role of a version child; null for legacy keys and research subversions. */
  role?: VersionRole | null
  /** A base's version slots; null on a child. */
  versions?: IndicatorVersions | null
  /** Scripts the class's script declares in REQUIRES. */
  requires?: string[]
  warmup_bars?: number
  certification?: IndicatorCertification | null
  timeframes?: Record<string, (IndicatorCertification & { timeframe: string }) | null>
  error?: string
}

/** `script` = an indicator module · `registry` = indicators/__init__.py · `reference` = read-only (the contract). */
export type ScriptKind = 'script' | 'registry' | 'reference'

/**
 * All indicator scripts register through REGISTER (`discovered`).
 * `explicit` is retained for old backend listings; `none` registers nothing,
 * and `unknown` means the registry could not be read.
 */
export type ScriptRegistration = 'explicit' | 'discovered' | 'none' | 'unknown'

export interface ScriptVersions {
  latest: number | null
  count: number
  /** The file no longer matches its latest recorded version — changed outside QuantScript. */
  external: boolean
}

export interface ScriptEntry {
  file: string
  stem: string
  path: string
  size: number
  modified: string | null
  sha256: string
  kind: ScriptKind
  editable: boolean
  /** First paragraph of the module docstring. */
  summary: string
  syntax_error: string | null
  classes: ScriptClass[]
  /** Registered indicator classes in the file. */
  registered: number
  /** The script could not register (import failed, key taken) — see indicators/_discover.py. */
  discovery_error: string | null
  registration: ScriptRegistration
  versions: ScriptVersions
}

/** A script only the version history still knows — deleted, or gone from disk. */
export interface ArchivedScript {
  file: string
  latest: number | null
  count: number
  /** The last version is a `delete` record (QuantScript deleted it). */
  deleted: boolean
  at: string | null
}

export interface PythonInfo {
  command: string
  ok: boolean
  error: string | null
  /** `QUANTSCRIPT_PYTHON` | `QUANTSYSTEMS_PYTHON` | `probe`. */
  source: string
}

export interface ScriptListing {
  generated_at: string
  version?: string
  /** The contract generation of the engine (contract.CONTRACT_VERSION). */
  contract_version?: number
  python: PythonInfo
  package_dir: string
  indicators_dir: string
  db_path: string
  /** The registry could not be imported — the files are listed without verdicts. */
  registry_error: string | null
  registry_keys: number
  /** Keys certified on every track (the overall verdict). */
  certified: string[]
  discovery_errors: Record<string, string>
  scripts: ScriptEntry[]
  reference: ScriptEntry[]
  archived: ArchivedScript[]
  /** No interpreter: the Rust listing of the files alone. */
  fallback?: boolean
}

// ── Versions ──

export interface VersionMeta {
  id: number
  file: string
  version: number
  sha256: string
  size: number
  message: string
  /** `baseline` | `user` | `external` | `restore` | `template` | `delete`. */
  author: string
  checked: boolean
  created_at: string
}

export interface ScriptDocument {
  file: string
  path: string
  kind: ScriptKind
  editable: boolean
  content: string
  sha256: string
  size: number
  modified: string | null
  version: VersionMeta | null
  /** Opening recorded a version: the baseline, or a change made outside QuantScript. */
  recorded: 'baseline' | 'external' | null
}

// ── The sandbox check ──

export type CheckDepth = 'quick' | 'full'

export interface CheckLaw {
  law: 'contract' | 'causality' | 'scale'
  ok: boolean
  warning?: boolean
  message: string
}

export interface CheckIndicator {
  key: string
  class_name: string
  name: string
  warmup_bars: number
  checks: CheckLaw[]
  ok: boolean
  committed_at: number | null
  bars: number | null
  error: string | null
  traceback?: string
  elapsed_s?: number
}

export interface CheckResult {
  file: string
  depth: CheckDepth
  syntax: { ok: true } | { ok: false; line: number | null; column: number | null; message: string; text: string }
  import: { ok: true } | { ok: false; message: string; traceback?: string }
  discovery_errors: Record<string, string>
  indicators: CheckIndicator[]
  registry_keys: number | null
  sandbox: string | null
  stderr: string
  /** Everything passed. */
  ok: boolean
  /** Must not be saved: a syntax error, a failing import, a script that cannot register. */
  blocking: boolean
  elapsed_s?: number
}

export interface SaveResult {
  saved: boolean
  unchanged: boolean
  checked: boolean
  check: CheckResult | null
  version: VersionMeta | null
  sha256: string
  note: string | null
}

export interface NewScript {
  key: string
  class_name: string
  name?: string
}

// ── The editor's diagnostics (python -m smithery.quantscript lint) ──

export interface LintMarker {
  severity: 'error' | 'warning'
  line: number
  column: number
  end_line: number | null
  end_column: number | null
  message: string
  source: string
}

export interface LintResult {
  ok: boolean
  markers: LintMarker[]
}

// ── The forge (backend: plugin:algo|smithery_* and list_indicators) ──

export interface IndicatorVariant {
  base_key: string
  label: string
  status: string
  /** Format 2 role versions only. */
  role?: VersionRole
  format?: number
  path?: string
  source_sha256?: string
  params?: Record<string, unknown>
  evidence?: Record<string, unknown>
}

export interface IndicatorInfo {
  base_key?: string
  variant?: IndicatorVariant | null
  /** `standard` for a base; the role of a version child; null for legacy keys and research subversions. */
  role?: VersionRole | null
  /** A base's version slots; null on a child. */
  versions?: IndicatorVersions | null
  /** Registry key — what a strategy passes to `self.regime(key)`. */
  key: string
  name: string
  hypothesis: string
  params: Record<string, unknown>
  param_space: Record<string, [number, number]>
  schema?: Record<string, ParamSchemaEntry>
  script?: string | null
  requires?: string[]
  warmup_bars: number
  /** THE verdict: the worst track's score, certified only on every track. */
  certification: IndicatorCertification | null
  timeframes?: Record<string, (IndicatorCertification & { timeframe: string }) | null>
}

export interface IndicatorRegistry {
  unavailable_variants?: { key: string; base_key: string; label: string; role?: VersionRole | null; error: string; path: string }[]
  generated_at: string
  /** The contract generation of the engine (contract.CONTRACT_VERSION). */
  contract_version?: number
  package_dir: string
  timeframes?: string[]
  indicators: IndicatorInfo[]
}

/** A certification track, or `all` — every track in turn (gauntlet only). */
export type SmitheryTimeframe = 'all' | '1d' | '4h' | '1h'

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
  timeframe: string
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
  timeframes?: string[]
  certified_by_timeframe?: Record<string, string[]>
  supported_timeframes?: string[]
  workers: number
  shelf: SmitheryShelfSeries[]
  reports: SmitheryReportMeta[]
}

export type ForgeKind = 'gauntlet' | 'walkforward' | 'optimize' | 'compare' | 'refresh'

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
  /** `optimize` only: skip indicators whose timeframe versions are current. */
  resume?: boolean
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

/** What QuantAlgo returns for a strategy made from an indicator. */
export interface AlgoStrategy {
  id: string
  name: string
}

// ─── The Collection (plugin:script|collection_*) ───────────────────────────────────────

/** A catalog source; the token itself never leaves the credential store. */
export interface CollectionSource {
  id: string
  kind: 'github' | 'folder'
  name: string
  owner: string
  repo: string
  branch: string
  /** github: a folder inside the repository; folder: the checkout. */
  path: string
  enabled: boolean
  last_commit: string | null
  last_checked: string | null
  has_token: boolean
}

export interface CollectionSourceInput {
  id?: string
  kind: 'github' | 'folder'
  name: string
  owner?: string
  repo?: string
  branch?: string
  path?: string
  enabled?: boolean
}

export interface CollectionVerdict {
  score: number
  grade: string
  certified: boolean
  perm_p?: number | null
  date?: string | null
  evaluation_version?: string | null
}

/** The library's state of one catalog item. */
export interface CollectionLocal {
  installed_version: string | null
  installed_commit: string | null
  update: boolean
  /** Files the Collection installed were changed in the library since. */
  modified: boolean
  /** A script of that name the Collection did not install. */
  present: boolean
  conflict: boolean
  /** The engine's contract is older than the package's. */
  too_new: boolean
}

export interface CollectionCatalogItem {
  type: 'indicator' | 'library'
  key: string
  name: string
  summary: string
  tags: string[]
  version: string
  contract: number
  requires: string[]
  /** role → track → verdict */
  scores: Record<string, Record<string, CollectionVerdict>>
  manifest: { path: string; sha256: string }
  local: CollectionLocal
}

export interface CollectionCatalog {
  source: CollectionSource
  commit: string
  generated_at: string | null
  engine_contract: number | null
  items: CollectionCatalogItem[]
}

export interface CollectionManifestVersion {
  key: string
  label: string
  params: Record<string, unknown>
  decision: string | null
  file: string | null
  evidence: Record<string, CollectionVerdict>
}

export interface CollectionManifest {
  format: number
  type: 'indicator' | 'library'
  key: string
  name: string
  summary: string
  description: string
  tags: string[]
  version: string
  released_at: string
  author: string
  license: string
  contract: number
  requires: string[]
  registers?: string[]
  warmup_bars?: number
  legacy?: { key: string; label: string }[]
  schema?: Record<string, ParamSchemaEntry>
  versions?: Partial<Record<VersionRole, CollectionManifestVersion>>
  files: { path: string; sha256: string; role: string }[]
  changelog: { version: string; date: string; change: string; note: string }[]
}

export interface CollectionItemDetail {
  commit: string
  item: Omit<CollectionCatalogItem, 'local'>
  manifest: CollectionManifest
  readme: string | null
  local: CollectionLocal | null
}

export interface CollectionPlanItem {
  key: string
  name: string
  type: 'indicator' | 'library'
  version: string
  installed_version: string | null
  action: 'create' | 'replace' | 'skip' | 'conflict'
  reason: string | null
  /** Chosen by the user; false = pulled in as a requirement. */
  requested: boolean
  files: { path: string; action: 'create' | 'replace' | 'same' | 'remove' }[]
}

export interface CollectionPlan {
  source: string
  commit: string
  items: CollectionPlanItem[]
  conflicts: number
}

export interface CollectionInstallOutcome {
  plan: CollectionPlan
  written: { key: string; file: string; created: boolean; version: number | null }[]
  check: unknown
}

export interface CollectionRemoved {
  key: string
  file: string
  keys: string[]
  version: number | null
  files: string[]
}
