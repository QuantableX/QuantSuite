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
   with `offset` and `limit`; rows are compact by default (long series and very
   wide objects are summarized, `compact: false` returns everything).
   `section: "html"` returns the standalone English report, which can be
   several MB.
6. Explicitly select a result and use `research_install(run_id, indicator,
   timeframe)`. This requires certification on that track, verifies evidence,
   stages dependencies, every version file of the indicator and their native
   reports, and refuses differing existing library files.
   Refresh `quantsuite.algo.list_indicators`, then call
   `quantsuite.algo.create_strategy_from_indicator`. No bot is started.

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

Profit runs use `kind: "profit"`, candidate keys, and an explicit saved system
`config` including `startDate`, `endDate` (before today), `feeRate`,
`slippageRate` and the baseline `indicator`. The native rotation/single-asset
engine retains its original execution semantics. Ranking answers are frozen
in memory; all candidates receive identical OHLCV inputs using the longest
required warm-up. Provider failures stop the comparison, preserving partial
evidence. Native indicator errors remain visible per candidate.

Singles are separate. Aggregates allow any 2+ distinct base indicators; variants
of one base cannot cast duplicate votes. Omit `sizes` to cover every feasible
size. `samples_per_size` defaults to 20; small spaces are enumerated, larger
spaces receive seeded finite samples. Runs are capped at 5,000 candidates.
Coverage is reported by size. This reusable native-engine path does not claim
the throughput or exhaustive coverage of earlier specialized tournament runs.
The objective is retrospective final capital, not pristine out-of-sample proof.

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
