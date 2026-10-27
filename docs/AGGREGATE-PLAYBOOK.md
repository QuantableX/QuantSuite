# Aggregate playbook: finding the strongest combinations

How to find the best indicator aggregates for a saved QuantSystems system with
the research tools, and how to put them into saved systems. This is the method
only: results, indicator names and run ids stay private (QuantScript Research
store and the private vault). The tools themselves are described in
[QUANTSCRIPT-RESEARCH.md](QUANTSCRIPT-RESEARCH.md).

## What is searched

An aggregate is a set of indicators that vote together: every member votes
+1/-1/0 on each pair, the sign of the vote sum decides, and an even split keeps
the previous verdict. Each base indicator appears at most once.

"Best" means the highest final capital of the saved system over the chosen
dates, with everything else of that system unchanged (universe, Top N, cadence,
market filter, fees and slippage). Maximum drawdown is shown next to it but does
not rank. All numbers follow the data rules in QUANTSCRIPT-RESEARCH.md: daily
Top N, clean exchange data, bounded indicator history (a backtest and the live
tracker give the same vote on the same day).

Candidates are, per base indicator, its general version and its version for the
system's track (`<key>_opt_1d` for a daily system). Standard is used only for an
indicator without any optimized version, and versions with identical settings
count once. Volume indicators cannot vote on synthetic coin ratios; the report
lists them as not evaluable.

## Why the search is staged

Testing every combination is impossible. With about 660 base indicators there
are about 217,000 pairs, 48 million triples (about a day at ~500 aggregates per
second) and 7.8 billion sets of four. So one comparison runs fixed stages,
written to `plan.json` before any result is seen:

1. **Control:** the reference (the system's current aggregate) and the user's
   saved aggregates with the same evaluation settings, ranked as they are.
2. **Neighbours** of the saved aggregates and the reference: every single add,
   remove or swap, and every two additions to the reference.
3. **Leaders of earlier comparisons:** the top 20 of every finished profit or
   aggregate run whose evaluation settings match (universe, cadence, ranking
   resolution, filter and costs; dates do not matter). Nothing found earlier is
   lost.
4. **All pairs**, exhaustively.
5. **Growth by size:** from triples on, every one-indicator extension of the
   best 200 aggregates of the size before, while bigger sizes still improve
   (stops after 4 sizes without a better best). Skipped when `sizes` is given.
6. **Balanced samples** of every allowed size (10% of the budget left).
7. **Local search until stable:** every untested single add, remove or swap of
   the top 25, of the best aggregate of each size up to 16 and of the saved and
   earlier aggregates, round after round, until a full round leaves the top 25
   unchanged, or the budget ends.

"Stable" means each of the top 25 is a local optimum for single steps: no one
change improves it. It is not proof that no better aggregate exists elsewhere.

## Step by step

### 1. Prepare

- Install the newest private library (QuantScript Collection) and the newest
  QuantSuite release. A running comparison uses frozen copies of the engine,
  the library and the prices, so later installs do not change it.
- Decide the system whose settings define the evaluation (for example the
  current Rank 1 of a family) and the end date (default: yesterday, only closed
  days).
- A comparison is a heavy job. Run one at a time; let it finish before closing
  the app if it was started from the app.

### 2. Run the main search

Ask the connected AI, or call the tool directly:

```text
quantsuite.algo.research_aggregate_comparison(
  system: "<saved system>", title: "<system> · aggregate top 10", budget: 5000000)
```

The default budget is also the maximum (5,000,000 aggregates per run). On a
32-core machine with about 660 bases, expect roughly: capturing the engine's
inputs 1 to 1.5 hours, the stages up to growth by size 2.5 hours, then the local
search. Progress shows in QuantScript → Forge → Research and in
`quantsuite.script.research_list` / `quantsuite.algo.forge_jobs`.

### 3. Read the coverage

The report's coverage tiles say how far it got:

- **Growth by size: up to N** — the largest size growth reached.
- **Local search: stable** — done. **Local search: budget** — the budget ended
  before a full round stayed unchanged; continue with step 4.

### 4. Follow up until stable

Start the same comparison again with an explicit size list, which skips growth
by size (the first run already did it), and the full budget:

```text
quantsuite.algo.research_aggregate_comparison(
  system: "<same system>", end_date: "<same end date>", budget: 5000000,
  sizes: [2, 3, ..., <number of bases>], title: "<system> · follow-up until stable")
```

Its local search starts from the earlier run's top 20 (stage 3) and continues
round by round. Repeat until the coverage says stable. Typical follow-ups need
a few rounds of 100,000 to 450,000 aggregates each.

### 5. Take the top 10 distinct results

Two aggregates with exactly the same final capital and drawdown (to every
digit) trade identically in practice: one of them contains a member that never
changes the vote sum's sign. Count such twins once and keep the smallest set. The ranked list can hold
many twins, so the 10 best distinct results can reach down to row 20 or
further.

### 6. Verify natively

Each run re-runs its raw top 10 and the best of sizes 2 to 5 in the native
engine (`verification.json`; holdings identical on every bar). A distinct result
outside that list must be re-run natively on the same frozen data before it is
used: copies of the run folder's `cache.sqlite` (as `ROTATION_LAB_CACHE`) and
`data/`, the library snapshot in `library/` (as `QUANTSCRIPT_INDICATORS_DIR`),
and the run's candidate keys, so the shared warm-up start matches. Its final capital must equal the ranked value. Engine
scripts that use worker processes need an `if __name__ == "__main__":` guard,
or every worker re-runs the script.

### 7. Put them into saved systems

- Close QuantSuite first: it keeps the systems in memory and writes them back
  on exit, which would undo the change.
- Back up `<QUANTSUITE_HOME>/modules/systems/config.json`.
- Per rank system change only the aggregate members and the description. The
  description is one short line of what the system does (no indicator list, the
  app shows it; no returns or dates, they go stale).
- Start QuantSuite and check the systems list and one backtest.

### 8. Sanity checks

- **Independent restarts** (2023-01-01 and 2025-01-01 by default): the new
  leader should also beat the reference from later starts.
- **In-sample:** the leader is the best of millions of tries on the same dates,
  so its numbers flatter it. Expect less live. Prefer leaders that also win the
  restarts and have lower drawdowns.
- Compare numbers only within the same data rules and settings. Results from
  before a rule change (for example weekly Top N) are not comparable.

## Building stronger combinations

- **More indicators:** new indicators through the Gauntlet workflow
  (QUANTSCRIPT-RESEARCH.md), install the library, run the comparison again.
  Earlier leaders seed the new run, so the old best stays in the race and new
  members can join it.
- **New end date:** rerun after a few months; the leaders seed it and the
  ranking adapts to the new data.
- **Other systems:** any saved system with an aggregate indicator works (other
  universes, Top N or filters); its evaluation settings keep its runs apart.
- **Smaller sets:** `sizes: [2, ..., 12]` searches only small aggregates, which
  are easier to understand and often more robust.
- **Another seed** (`seed`) changes the balanced samples, a cheap way to look
  somewhere else.
- **A different reference** (`reference`) starts the neighbour stages from
  another aggregate.

## Limits and errors

- `budget must be a whole number from 0 to 5000000`: the per-run cap; chain
  follow-up runs instead. A refused request leaves a failed run ("Preparation
  failed") in the Research list.
- `Use an explicit historical date range ending before today`: the end date
  must be a closed day.
- Volume indicators are listed as not evaluable on coin ratios.
- A system whose market filter follows the candidate's own indicator cannot use
  the bulk evaluator.
