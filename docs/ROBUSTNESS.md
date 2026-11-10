# Robustness policy

Every optimization in QuantSuite has to fight overfitting. This applies to a single indicator, an aggregate of indicators, a rotation system such as LCES, and a single-asset strategy. A result counts only when small, realistic changes do not break it. A backtest multiple without a robustness verdict is not a result.

This policy is code, not advice. `sidecars/python/smithery/robust_policy.py` holds every group, gate and grade below, and the System Gauntlet (`smithery/system_gauntlet.py`), the Indicator Gauntlet (`smithery/robustness.py`) and the aggregate search (`smithery/lab_aggregates.py`) all read it. Change a threshold there, and in this file in the same commit.

## Why (2026-11-03)

LCES Ranks 1–10 came from a search that ranked about 6.8 million aggregates by one history's final capital, on Binance candles only. Not one of their members had passed the Indicator Gauntlet. Under test:

| Test | Result |
|---|---|
| Walk-forward (the same search shown only 2020–22) | Its winners made 2.1–6.0× in 2023–26, against 7.1× for random 27-indicator aggregates. |
| Other prices | Another exchange's candles, or ±0.03% noise, left 0.4–8.9% of the multiple. |
| One-day trade delay | Left 1.2% of the multiple (random aggregates kept 97%). |

The search had fitted the exact prices and the exact timing of a few big moves.

## The yardstick: CAGR kept

`retention = CAGR(variant) ÷ CAGR(base)`, the share of the yearly growth a change leaves. CAGR, not the final multiple: a ratio of multiples shrinks with every extra year of history, a ratio of CAGRs means the same over two years and over seven.

- A group's score is 0 at retention 0.4 and 100 at 0.9.
- A sign flip is a variant losing money while the base makes money.

| On 2026-11-03 | One-day delay | ±0.03% noise | Other exchange |
|---|---|---|---|
| Random 27-indicator aggregates | ≈1.0 | ≈1.0 | ≈1.0 |
| LCES (dc + dcl + dmi) | 0.76 | 0.93–1.0 | 1.2 |
| LCES Rank 1 | 0.39 | 0.46–0.57 | 0.52 |

## The axes

The user named the first five. The tests of 2026-11-03 added the execution axis, the member groups and the walk-forward of the search itself.

| Axis | Group | What changes | Fails when |
|---|---|---|---|
| **1 Parameters** | parameter_jitter | Every member's numeric parameters ×(1 ± 15%) at random, all at once. 16 runs (6 in fast mode). | The median keeps < 0.5, or any run keeps < 0.25, or more than 10% of runs lose money. |
| | member_drop | Each member of an aggregate left out once. | From 6 members: the median keeps < 0.5, or one drop keeps < 0.2, or any drop loses money. Below 6 members it is shown but not scored, because in a vote of three each voice is a third. |
| | member_add | One more indicator from the library added (12 runs, 6 in fast mode). | From 6 members: the median keeps < 0.5, or more than 10% lose money. |
| | system_settings | Top N ±1, market-filter lengths ×0.8 and ×1.2; a filter that is an indicator has its parameters ×0.8 and ×1.2 instead. | The median keeps < 0.5, or any run keeps < 0.25, or any run loses money. |
| **2 Time** | start_shift | The backtest starting 1–11 months later; every run keeps two or more years. | The median keeps < 0.5, or any run keeps < 0.3, or any run loses money. |
| | eras | Calendar years, rolling 1-year returns, the worst rolling 2-year CAGR. | A rolling 2-year window has a CAGR below −25%. Scored by the share of positive rolling years. |
| | walk-forward (search) | The search itself, shown only an earlier window; its picks run on the blind years after. | A pick does not beat random aggregates of the same size in the blind years (aggregate search, card 3). |
| | optimization windows (indicators) | Parameters re-optimized on 2020–22, 2021–24, 2023–25 and 2024–26. | Parameters or out-of-sample statistics move too much (Indicator Gauntlet, card 2). |
| **3 Exchange** | price_noise | Every open, high, low and close ×(1 + N(0, 0.03%)), three seeds. | The median keeps < 0.6, or any seed keeps < 0.5, or any seed loses money. |
| | venue | Another exchange's candle on every day it has one (OKX → Bybit → KuCoin → Coinbase); the other days stay as saved. Binance, Gate.io and MEXC are never the other venue. | It keeps < 0.5, or loses money. |
| **4 Assets** | universe | Top N −2 and +2, without the largest coin. A single-asset system uses other assets. | The median keeps < 0.4, or any run loses money. |
| | (indicators) | ETH, XRP, BNB and SOL besides BTC; optimization across the basket. | Indicator Gauntlet. |
| **5 Monte Carlo** | permutation | The window's calendar days shuffled for every coin at once (39 shuffles, 9 in fast mode). | p > 0.10 caps the grade at B. |
| | bootstrap | Stationary block bootstrap (20-day blocks) of the daily returns: CAGR and drawdown spread, chance of a 1- and 3-year loss. | The chance of a 3-year loss is > 25%. |
| **6 Execution** | delay | Every trade one day later (two days shown). | One day keeps < 0.5, or loses money. |
| | fees | 2× the fee (3× shown). | It keeps < 0.5, or loses money. |

Groups run against their own base. Member drops, additions and delays use the bulk evaluator (`smithery/lab_fast.py`, engine-identical). Everything else is a native engine run on the saved data, changed in one way.

## Indicator Gauntlet (single indicators)

`smithery/robustness.py` certifies one indicator per track (1d, 4h, 1h) on the Forge's price shelf. Since evaluation `smithery-2026-11-03.1` it has six equally weighted axes:

| Axis | What it measures | Hard gate |
|---|---|---|
| Asset | BTC, ETH, XRP, BNB, SOL, plus the ratio pairs ETH/BTC, BNB/BTC and SOL/ETH (the systems trade indicators on ratios). | Fewer than 60% of the basket profitable. |
| Exchange | BTC on every venue of the shelf: signal agreement and Sharpe spread. | Profitable on one venue, losing on another. |
| Parameter | Each parameter ×0.5 … ×1.5: plateau and signal stability. | A ±10–25% change turns a profitable edge into a losing one. |
| Temporal | Two-year eras and rolling one-year Sharpe with fixed parameters. | — |
| Optimization windows | Parameters re-optimized (plateau medoid, basket median) on rolling three-year windows: how far they move, and the basket Sharpe in the year after each window. | The year after loses money in more than half the windows. |
| Monte Carlo | Permutation (luck), stationary bootstrap, GARCH synthetic charts, noise injection. | Permutation p > 0.10 (as before). |

Certification needs a score of 70 or more, no failed gate, full Monte Carlo counts and complete coverage.

**The ratio pairs' candles (2026-11-10).** A pair's candle comes from `smithery/ratio.py`, the one function the engine, the bulk evaluators and today's live signal build it with: open and close are the ratios of the two coins' opens and closes, high and low the highest and lowest ratio along the bar's hourly closes. On the 1d track the hours come from the shelf's 1h series of both coins. The 4h and 1h tracks use the stand-in (high(A)/high(B) and low(A)/low(B), widened to hold open and close), as the engine does on intraday bars. Before, this test drew the pair candle as its body alone (no wicks, about half the real range) while the systems ran on a candle three times too wide.

The pair part is 20 of the asset axis' 100 points (3.3 of the total), in steps of one pair. An indicator that reads only the close scores exactly as before. `EVALUATION_VERSION` is therefore unchanged and the verdicts stored with the indicator versions keep counting; the change to the evaluator's files only makes earlier Gauntlet runs "not current". An indicator that reads high or low can score differently: run those again.

- **Universal optimization:** the forge's walk-forward chooses parameters on the basket's median (`smithery/universal.py`), not on BTC alone. An explicitly chosen single series stays possible for research.
- **Certification is a gate:** the forge writes an `optimized_<tf>` version only when it certifies. A failing result is recorded as evidence. An existing version stays loadable so saved systems keep working, but its evidence says it is not robust.
- **Strict lookup:** `registry.robust_on_track(key, tf)` counts only verdicts from the current evaluation. The aggregate search uses it for its pool.
- **What a verdict depends on:** the indicator's own script (with its `REQUIRES`) and the Gauntlet's code, meaning `robustness.py` and every smithery module it imports (`evidence.evaluator_files`). Editing one of those turns the affected verdicts into history and calls for a re-certification. Editing the lab, a report or the aggregate search does not.
- **Recorded pool:** a research run's frozen vault holds the price shelf only, so a robust search decides its certified pool when the run is created and records it in `run.json` (`pool_certified`).

## Grade and verdict

- **Axis score:** the mean of its groups' scores.
- **Overall score:** the mean of the six axis scores.
- **Grade:** S ≥ 85, A ≥ 70, B ≥ 55, C ≥ 40, otherwise F. These are the Indicator Gauntlet's bands, and a permutation p > 0.10 caps S and A at B.

| Verdict | When |
|---|---|
| **robust** | No gate failed, grade B or better, and every axis was measured. |
| **not robust** | A gate failed, or the grade is C or F. The report names each failed gate. |
| **incomplete** | An axis could not be measured, for example no candles from another exchange. It never counts as robust. |

A system that makes no money on its own history has nothing to protect and is not robust.

## Where it applies

| Where | How |
|---|---|
| Indicator Gauntlet | Certifies single indicators. Optimized versions are written only when they pass (card 2). |
| Aggregate search | Takes only indicators that passed, scores each candidate in several price worlds, confirms picks walk-forward, and runs the System Gauntlet on the finalists (card 3). |
| Research kind `robustness` | The System Gauntlet for any saved system: MCP tool `quantsuite.algo.research_robustness`, QuantScript → Forge → Research ("Robustness check"), and the Robustness panel in QuantSystems' right sidebar (verdict, failed gates, quick or full check, report). |
| MCP research tools | `research_aggregate_comparison` is robust by default (`pool`, `blind_days`, `gauntlet_top`, `venues`). The profit overview tells agents to check any kept system with `research_robustness`. `research_list` carries each run's verdict. |
| Installing a research indicator | `research_install` installs only an indicator certified on the target track. |
| A fund (Manual) | A dynamic split method is itself an optimization. The result shows each member's verdict, the method against Equal at every lookback and in the rule's four windows, and a Monte Carlo of the fund's returns. A fund plans and evaluates; it places no orders. |
| A saved system | QuantSystems shows its latest verdict: robust, not robust (with the failed gates) or not checked. A system that was never checked or failed stays visibly marked. |

## Aggregate search (combinations)

`smithery/lab_aggregates.py` with `smithery/robust_search.py` is robust by default. `pool: "all"` stays possible for research, but no result of an ungated pool counts as robust.

1. **Pool gate:** only indicators that pass the Indicator Gauntlet on the system's track under the current evaluation (`registry.robust_on_track`). The system's current aggregate is evaluated as a control, but its members only join the pool when they pass.
2. **Price worlds:** every candidate is scored in each world, and the search ranks it by its worst world's final capital on the search window. The worlds:
   - the saved candles;
   - ±0.03% noise, two seeds;
   - another exchange's candles, when they were loaded;
   - every trade one day later.

   A fit to one exact history no longer wins.
3. **Walk-forward:** the search sees only the bars before the cut, which by default is two years before the end (a third of a shorter history). The 50 best distinct leaders then run on the blind bars after it, against 300 random aggregates of the same size from the same pool. A leader passes when its worst-world CAGR in the blind window is positive and beats the random median.
4. **System Gauntlet:** the best leaders that passed, three by default, go through the System Gauntlet in fast mode on the whole history. Only robust finalists are candidates for saved systems.
5. **No seeding from the whole history:** a robust search never starts from the saved aggregates or earlier leaders, because they were found on the whole history.

**Objective:** `objective: "capital"` (default) ranks by the worst world's final capital; `"calmar"` ranks by the worst world's CAGR per unit of max drawdown, for lower-drawdown systems. The walk-forward and the System Gauntlet judge both the same way.

**Speed:** every world is an engine-identical bulk evaluator. When PyTorch with CUDA is importable (`smithery/lab_gpu.py`, checked bit-identical against the CPU evaluator), the GPU scores the worlds. `SMITHERY_GPU=0` turns it off.

## Running it

- From the app or MCP: research kind `robustness` with a saved system's name. The run freezes the engine, the library and the saved prices like every research run. Before freezing it loads candles from the other venue chain into its own cache (`~/.quantsuite/modules/systems/cache/venues/`); the saved systems cache is never written.
- Full mode takes about 15–30 minutes for a 27-member system with 6 workers. Fast mode takes about 6–10 minutes.
- `SMITHERY_ROBUST_WORKERS` sets the parallel engine runs. Each needs 2–3 GB of memory.
