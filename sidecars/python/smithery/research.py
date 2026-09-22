"""A finite, reproducible comparison of frozen indicator candidates.

Selection uses only development eras. The winner is locked in a manifest
BEFORE the final chronological test is evaluated. Both failures and passes
are retained. Existing histories have already been used by earlier forge
rounds: this is a held-out comparison within this run, never a claim that
the historical data are globally unseen. No run changes trading defaults.
"""
from __future__ import annotations

import datetime as dt
from pathlib import Path

import numpy as np
import pandas as pd

from .contract import validate_causality, validate_scale_invariance
from .data import OUTPUT_DIR, available, load
from .evidence import EVALUATION_VERSION, artifact_id, code_fingerprint, frame_fingerprint, write_json
from .execution import evaluate
from .indicators import REGISTRY, WARMUP_BARS
from .robustness import ASSET_SYMBOLS, TIMEFRAMES, VENUE_PREF, find_primary


def _frames(timeframes: list[str]) -> dict[str, pd.DataFrame]:
    shelf = available()
    keys = []
    for tf in timeframes:
        keys.append(find_primary(tf))
        for symbol in ASSET_SYMBOLS:
            key = next((f"{venue}_{symbol}_{tf}" for venue in VENUE_PREF
                        if f"{venue}_{symbol}_{tf}" in shelf), None)
            if key is None:
                raise ValueError(f"Missing {symbol} {tf}: refresh the complete shelf before Comparison")
            keys.append(key)
    return {key: load(key) for key in keys}


def _selection_score(cells: list[dict]) -> float:
    """Reward the weak cells as well as the median; no 0-100 score inflation."""
    if not cells or any(c["bankrupt"] for c in cells):
        return -1e6
    sharpe = [c["sharpe"] for c in cells]
    return float(.5 * np.quantile(sharpe, .20) + .5 * np.median(sharpe))


def _cell(df, sig, start, stop, key, era, mode, cost):
    result = evaluate(df, sig, start=start, stop=stop, mode=mode, cost_bps=cost)
    return {"series": key, "era": era, **result}


def _profitable(cell: dict) -> bool:
    # A positive arithmetic Sharpe can coexist with a compounded loss.
    return not cell["bankrupt"] and cell["sharpe"] > 0 and cell["return"] > 0


def compare(keys: list[str], *, timeframes: list[str] | None = None,
            n_folds: int = 3, cost_bps: float = 20.0, progress=None) -> dict:
    keys = list(dict.fromkeys(keys))
    if len(keys) < 2 or any(k not in REGISTRY for k in keys):
        raise ValueError("Comparison needs at least two known indicators, including a baseline")
    if not 2 <= n_folds <= 12:
        raise ValueError("Comparison needs 2 to 12 development eras")
    if not np.isfinite(cost_bps) or cost_bps <= 0 or cost_bps >= 5000:
        raise ValueError("Comparison requires positive finite round-trip costs below 5000 bps")
    timeframes = list(TIMEFRAMES) if timeframes is None else list(dict.fromkeys(timeframes))
    if not timeframes or any(tf not in TIMEFRAMES for tf in timeframes):
        raise ValueError("Use the 1d, 4h, 1h and/or 1m tracks")
    frames = _frames(timeframes)
    common_start = max(df.index[0] for df in frames.values())
    end = min(df.index[-1] for df in frames.values())
    if end - common_start < pd.Timedelta(days=365 * 3):
        raise ValueError("Comparison requires at least three calendar years shared by every asset/track")
    cutoff = common_start + (end - common_start) * .75
    # Every candidate gets its declared warm-up on every asset. A fixed
    # percentage alone was too short for ScaleConsensus on newer assets.
    warmup = max(WARMUP_BARS.get(key, 400) for key in keys)
    if any(len(df) <= warmup for df in frames.values()):
        raise ValueError(f"Every series needs more than {warmup} warm-up bars")
    validation_start = max(common_start + (cutoff - common_start) * .40,
                           max(df.index[warmup] for df in frames.values()))
    if cutoff - validation_start < pd.Timedelta(days=30 * n_folds):
        raise ValueError("Not enough development history after every candidate's warm-up")
    bounds = pd.date_range(validation_start, cutoff, periods=n_folds + 1)
    run_id = artifact_id()
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    stem = f"{dt.date.today().isoformat()} Smithery Comparison ({run_id})"
    path = OUTPUT_DIR / f"{stem}.md"
    manifest = {
        "kind": "comparison", "version": EVALUATION_VERSION, "run_id": run_id,
        "code_sha256": code_fingerprint(), "candidates": {key: REGISTRY[key]().params for key in keys},
        "candidate_count": len(keys), "timeframes": timeframes, "folds": n_folds,
        "cost_bps": cost_bps, "stress_cost_bps": 2 * cost_bps,
        "validation_start": str(validation_start), "warmup_bars": warmup,
        "development_end": str(cutoff), "holdout_end": str(end),
        "data": {key: {"sha256": frame_fingerprint(df), "bars": len(df),
                       "start": str(df.index[0]), "end": str(df.index[-1])} for key, df in frames.items()},
        "protocol": "Fixed candidates; rank on development eras at base and doubled costs; test only the locked winner.",
        "limitations": [
            "Historical data were already used by earlier forge rounds; this is not virgin out-of-sample evidence.",
            "Five present-day assets are a transfer screen, not a survivorship-free LCES portfolio backtest.",
            "Short-side diagnostics exclude funding and borrow costs; execution is a simplified unit-exposure model.",
            "Selection considers multiple candidates; no unadjusted discovery p-value or live certification is claimed.",
        ],
    }
    # Also preserves the intended experiment when it is cancelled or fails.
    write_json(OUTPUT_DIR / f"{stem}.manifest.json", manifest)
    rows = []
    for key in keys:
        ind = REGISTRY[key]()
        cells, errors = [], []
        for series, frame in frames.items():
            try:
                dev = frame.loc[frame.index < cutoff]
                sample = dev.iloc[-min(1500, len(dev)):]
                for check in (validate_causality, validate_scale_invariance):
                    ok, message = check(ind, sample)
                    if not ok:
                        raise ValueError(message)
                sig = ind.signal(dev)
                for era in range(n_folds):
                    start = max(1, int(dev.index.searchsorted(bounds[era])))
                    stop = int(dev.index.searchsorted(bounds[era + 1]))
                    for cost in (cost_bps, 2 * cost_bps):
                        cells.append(_cell(dev, sig, start, stop, series, era + 1, "long_only", cost))
            except Exception as exc:
                errors.append(f"{series}: {type(exc).__name__}: {exc}")
        result = {"indicator": key, "name": ind.name, "params": ind.params,
                  "selection_score": _selection_score(cells) if not errors else None,
                  "development": cells, "errors": errors}
        rows.append(result)
        if progress:
            progress({"indicator": key, "name": ind.name, "selection_score": result["selection_score"],
                      "cells": len(cells), "errors": errors})
    eligible = [row for row in rows if row["selection_score"] is not None]
    if not eligible:
        write_json(path.with_suffix(".json"), {**manifest, "rows": rows, "winner": None})
        raise ValueError(f"No candidate completed the comparison; evidence: {path.with_suffix('.json')}")
    eligible.sort(key=lambda row: (-row["selection_score"], row["indicator"]))
    winner = eligible[0]["indicator"]
    write_json(OUTPUT_DIR / f"{stem}.selection.json", {
        "winner": winner, "params": REGISTRY[winner]().params,
        "selection_score": eligible[0]["selection_score"], "holdout_not_evaluated": True,
    })
    holdout, diagnostics, errors = [], [], []
    for series, frame in frames.items():
        try:
            df = frame.loc[frame.index <= end]
            sig = REGISTRY[winner]().signal(df)
            start = max(1, int(df.index.searchsorted(cutoff)))
            for mode in ("long_only", "long_short"):
                for cost in (cost_bps, 2 * cost_bps):
                    cell = _cell(df, sig, start, len(df), series, "holdout", mode, cost)
                    (holdout if mode == "long_only" else diagnostics).append(cell)
            # The buy-and-hold reference pays the same entry/exit costs.
            for cell in holdout:
                if cell["series"] == series:
                    bh = evaluate(df, pd.Series(1, index=df.index), start=start, cost_bps=cell["cost_bps"])
                    cell["buy_hold_return"] = bh["return"]
                    cell["buy_hold_max_dd"] = bh["max_dd"]
        except Exception as exc:
            errors.append(f"{series}: {type(exc).__name__}: {exc}")
    # Pairwise ratio transfer is a separate diagnostic; USD evidence cannot
    # certify how a detector ranks A against B inside LCES.
    ratios = []
    from rotation_lab.backtest.signals import ratio_frame
    for tf in timeframes:
        btc = frames[find_primary(tf)]
        for series, asset in frames.items():
            if not series.endswith(f"_{tf}") or "_BTC" in series:
                continue
            try:
                ratio = ratio_frame(asset, btc).loc[lambda f: f.index <= end]
                sig = REGISTRY[winner]().signal(ratio)
                start = max(1, int(ratio.index.searchsorted(cutoff)))
                cell = evaluate(ratio, sig, start=start, cost_bps=2 * cost_bps)
                ratios.append({"series": f"{series} / BTC", **cell})
            except Exception as exc:
                errors.append(f"ratio {series}: {type(exc).__name__}: {exc}")
    reasons = list(errors)
    if not holdout or not all(_profitable(c) for c in holdout):
        reasons.append("The winner did not remain profitable on every held-out asset/track/cost scenario")
    if any(c["max_dd"] < c["buy_hold_max_dd"] for c in holdout):
        reasons.append("At least one held-out drawdown was worse than buy-and-hold")
    if not ratios or not all(_profitable(c) for c in ratios):
        reasons.append("LCES ratio transfer did not pass on every tested pair/track")
    result = {**manifest, "rows": rows, "winner": winner, "holdout": holdout,
              "short_diagnostics": diagnostics, "ratio_diagnostics": ratios,
              "passed": not reasons, "reasons": reasons, "certified": False,
              "report": str(path), "report_name": path.stem}
    write_json(path.with_suffix(".json"), result)
    lines = ["# Smithery — frozen candidate comparison", "",
             f"Selected on development data: **{winner}**. Final test starts **{cutoff.date()}**.",
             f"{len(keys)} candidates · tracks {', '.join(timeframes)} · {n_folds} development eras · costs {cost_bps:g}/{2*cost_bps:g} bps round trip.",
             "", "## Development ranking", "", "| Indicator | Robust selection objective | Status |", "|---|---:|---|"]
    for row in sorted(rows, key=lambda r: -(r["selection_score"] if r["selection_score"] is not None else -1e9)):
        score = "—" if row["selection_score"] is None else f"{row['selection_score']:.3f}"
        lines.append(f"| {row['indicator']} | {score} | {'failed checks' if row['errors'] else 'evaluated'} |")
    lines.extend(["", "## Final chronological test — long-only", "",
                  "| Series | Cost bps | Sharpe | Return | Max DD | Buy & hold return |", "|---|---:|---:|---:|---:|---:|"])
    for cell in holdout:
        lines.append(f"| {cell['series']} | {cell['cost_bps']:.0f} | {cell['sharpe']:.2f} | {cell['return']:.1%} | {cell['max_dd']:.1%} | {cell['buy_hold_return']:.1%} |")
    lines.extend(["", "## LCES ratio transfer screen", "",
                  "Synthetic OHLC ratios are bounds, not tradable prices. These results assess transfer only; run the actual historical-universe portfolio backtest before deployment.",
                  "", "| Ratio | Sharpe | Return | Max DD |", "|---|---:|---:|---:|"])
    for cell in ratios:
        lines.append(f"| {cell['series']} | {cell['sharpe']:.2f} | {cell['return']:.1%} | {cell['max_dd']:.1%} |")
    lines.extend(["", "## Decision", "",
                  "Passed comparison gates; full gauntlet and paper validation still required." if not reasons
                  else "No promotion. The selected candidate failed the final checks."])
    lines.extend(f"- {reason}" for reason in reasons)
    lines.extend(["", "## Limits and reproducibility", ""])
    lines.extend(f"- {reason}" for reason in manifest["limitations"])
    lines.extend(["", f"Evaluation version: `{EVALUATION_VERSION}`. Run `{run_id}`.",
                  "The adjacent JSON, manifest and selection files record every candidate, parameter, date boundary, failure and data/code hash."])
    with path.open("x", encoding="utf-8") as handle:
        handle.write("\n".join(lines) + "\n")
    return result
