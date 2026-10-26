# QuantScript Research

Open **QuantScript → Forge → Research** to import an offline report, create a
research draft, or inspect durable results. The app and MCP use the same private
storage at `<QUANTSUITE_HOME>/modules/script/research/<run-id>`.

All report templates and generated HTML use English. The runtime ships generic
orchestration and rendering code only. Private scripts, parameters, results,
imported HTML and market snapshots never enter the repository or installer.

## AI workflow

Tell an AI connected to QuantMCP: “Create more trend indicators for my private
QuantScript library and test them through the full Gauntlet.”

1. `quantsuite.script.research_context` returns the actual contract, a template,
   saved system configuration, protocol, private paths and workflow.
2. `research_create` takes a `request` with `title`, `kind: "gauntlet"` and
   optional existing base `indicators`. An empty list is allowed while authoring.
3. `research_candidate` takes `run_id`, a new Python `file`, full `content`, and
   its registered base `keys`. The native full contract check runs before the
   candidate is staged. Failures and previous attempts remain available.
4. `quantsuite.algo.research_run` starts the draft asynchronously. Poll
   `quantsuite.script.research_read` and `quantsuite.algo.forge_jobs`.
5. Inspect every track and failure. `research_read(section: "results")` pages
   with `offset` and `limit`; rows and the other result fields are compact by
   default (long series and very wide objects are summarized, `compact: false`
   returns everything). A library overview's page carries only the native
   reports its rows name.
   `section: "html"` returns the standalone English report, which can be
   several MB.
6. Explicitly select a result and use `research_install(run_id, indicator,
   timeframe)`. This requires certification on that track, verifies evidence,
   stages dependencies, every version file of the indicator and their native
   reports, and refuses differing existing library files.
   Refresh `quantsuite.algo.list_indicators`, then call
   `quantsuite.algo.create_strategy_from_indicator`. No bot is started.

## Report tools

Each report has one English template (light and dark, offline, evidence
embedded, CSV export from the page). A report tool creates the research run and
starts it as a forge job in one call; the run and its report appear in the
Research tab, and `research_read` returns the results.

- `quantsuite.algo.research_gauntlet_overview(title?, indicators?)` — the
  Gauntlet report of the library's existing native evidence, compiled from a
  library snapshot and the vault's reports without re-testing: Standard and
  selected-per-track verdicts on 1h/4h/1d, the worst track as the overall
  verdict, permutation p-values, five axes, walk-forward, parameters, and the
  native reports it read (their hashes in `report-hashes.json`).
- Gauntlet research runs (new candidates) render with the same template.
- `quantsuite.algo.research_profit_overview(system, title?, indicators?,
  start_date?, end_date?, top?, budget?, sizes?, seed?)` — the most profitable
  singles and aggregates in a saved system against its current setup:
  rankings with capital from a chosen start amount, capital curves (log,
  linear, drawdown), profit against drawdown, annual returns, coverage and
  method. See "Evidence and comparisons" for the search and verification.
- `quantsuite.algo.research_aggregate_comparison(system, title?, reference?,
  indicators?, start_date?, end_date?, top?, budget?, sizes?, restarts?, seed?)`
  — a variable-size search against the system's current aggregate (or
  `reference`): the reference itself, all its add/remove/swap neighbours, two
  additions to it, all pairs, balanced samples of every size and four
  local-search rounds (default budget 200,000). The report shows the new leader
  against the current aggregate (capital and drawdown), the top 10, the best
  aggregate of every size with its test count, and independent restarts
  (default 2023-01-01 and 2025-01-01, run natively from a fresh start). The top
  10 and the best of sizes 2 to 5 are re-run natively.

## Bulk evaluation in a saved system

Large single and aggregate searches use `smithery/lab_fast.py`. The unchanged
engine runs up to its simulation step once per warm-up group (the fetch start a
candidate's warm-up implies) and hands over its universe snapshots, candles and
TOTAL gate. Each candidate's +1/-1/0 vote on every pair is computed once per
group with the engine's signal code; singles and aggregates (sign of the vote
sum, previous verdict kept on an even split) are then simulated in bulk with
the engine's bar loop and float order. `verify` re-runs a candidate natively
and requires identical holdings on every bar. On the frozen LCES tournament
data it reproduces the original profit overview's numbers exactly. Systems whose
market filter follows the candidate's own indicator are not supported; volume
indicators cannot vote on synthetic coin ratios and are reported, not ranked.

Every research command takes snake_case arguments (`run_id`), the same names
the MCP tools advertise, because the agent broker passes arguments unchanged.
The suite's normal capability approval gate applies to writes. An MCP tool
does not contain a separate language model: the connected AI authors the source,
and the native service checks, evaluates and stores it.

## Evidence and comparisons

Gauntlet runs snapshot the engine, private library and Forge shelf. They validate
causality and scale invariance on historical series, then invoke unchanged native
optimization on 1h, 4h and 1d with seed 42, 120 permutations, 200 bootstraps,
100 GARCH paths and four anchored walk-forward folds. The worst-track overall
rule is unchanged. A completed evaluation can contain uncertified candidates.

Profit runs (`kind: "profit"`) take a saved `system` (name or id) or an explicit
`config`; dates default to the saved start and yesterday (closed history only).
Empty `indicators` means every base indicator in its version for the system's
track (`<key>_opt_1d` for a daily system), else Standard. Profit runs and
aggregate comparisons use the saved price data only: the copy of QuantSystems'
price cache taken at the start (daily candles and CoinMarketCap rankings).
Nothing is downloaded and nothing is rewritten; warm-up history before the first
saved day is simply absent, and the run ends on the last day the saved data
covers (the report says so). Ranking answers and candles are frozen on first
use; every candidate receives identical inputs from the longest warm-up. The
current system runs natively as the reference line.

The search is fixed before results are seen (`plan.json`, `seed`): every
candidate alone, all pairs and triples of distinct base indicators where they
fit the budget (else seeded samples), seeded samples of every larger size, and
three local-search rounds around the leaders (add, remove or swap one member).
`budget` caps the aggregates tested (default 50,000), `sizes` restricts them,
`top` sets how many aggregates and singles the report shows (one version per
base among singles). Each base casts at most one vote, and versions with the same
script and settings count once (the reference's or the current system's key is
kept). Before ranking, the bulk evaluator is checked against a native run; afterwards every shown candidate is
re-run natively and must match bar for bar, or the run fails with its evidence
kept (`verification.json`). Systems the bulk evaluator cannot express run
natively within 400 aggregates. The objective is retrospective final capital,
not out-of-sample proof or a global optimum.

Drafts are editable; evaluated inputs are immutable. Cancellation terminates the
Forge process tree. A stopped worker is shown as interrupted; its partial files
remain. Start a new draft for another evaluation. This release does not resume
an interrupted evaluation or publish packages to a Collection repository.

`research_import(source, title?)` retains the original HTML and its SHA-256,
parses embedded JSON without executing it, and embeds neighboring CSV downloads.
`research_read(section: "legacy")` pages through one list of that data
(`field`, defaulting to the longest list of records) and describes the other
fields. The UI renders reports in an opaque iframe with network access disabled.
Import does not rerun or recertify historical evidence. Translate a report
through its own template before importing it if it uses another language.
