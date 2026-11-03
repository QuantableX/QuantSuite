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
| | system_settings | Top N ±1, market-filter lengths ×0.8 and ×1.2. | The median keeps < 0.5, or any run keeps < 0.25, or any run loses money. |
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
| Research kind `robustness` | The System Gauntlet for any saved system (`quantsuite.algo.research_*`, QuantScript's research tab). |
| Saving or importing a result as a system or strategy | Needs a robust verdict, or the user's explicit "not robust" mark in the app (card 4). |

## Running it

- From the app or MCP: research kind `robustness` with a saved system's name. The run freezes the engine, the library and the saved prices like every research run. Before freezing it loads candles from the other venue chain into its own cache (`~/.quantsuite/modules/systems/cache/venues/`); the saved systems cache is never written.
- Full mode takes about 15–30 minutes for a 27-member system with 6 workers. Fast mode takes about 6–10 minutes.
- `SMITHERY_ROBUST_WORKERS` sets the parallel engine runs. Each needs 2–3 GB of memory.
