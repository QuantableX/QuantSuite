"""The Robustness Gauntlet — five axes, one score. See Docs/01.

Axes: asset · exchange · parameter · temporal · Monte Carlo.
Each axis returns (sub_score 0-100, markdown table rows). Aggregate is the
doc-specified weighted mean; permutation p > 0.10 hard-caps the grade at B.

Certification tracks: the gauntlet runs per TIMEFRAME — `1d`
is the reference track, `4h`, `1h` and `1m` are the intraday tracks — on that
timeframe's shelf series (`<EXCHANGE>_<SYMBOL>_<tf>`). A verdict belongs to
its track; an indicator certified on daily bars says nothing about 1m until
the 1m gauntlet has said it.
"""
from __future__ import annotations

import datetime as dt
import time

import numpy as np
import pandas as pd

from . import backtest, monte_carlo as mc
from . import parallel as par
from .contract import TrendIndicator, validate_causality, validate_scale_invariance
from .data import DOCS_DIR, OUTPUT_DIR, asset_class, available, load
from .evidence import (EVALUATION_VERSION, artifact_id, certification_gates,
                       code_fingerprint, frame_fingerprint, write_json)

WEIGHTS = {"asset": 0.20, "exchange": 0.20,
           "parameter": 0.20, "temporal": 0.20, "monte_carlo": 0.20}
VENUE_PREF = ["BINANCE", "OKX", "BYBIT", "KUCOIN", "BITSTAMP", "COINBASE", "KRAKEN"]
# Asset axis shelf: top-5 crypto by market cap, stablecoins & staked versions
# excluded — BTC (primary) + these four. Nothing else is ever tested.
ASSET_SYMBOLS = ["ETHUSDT", "XRPUSDT", "BNBUSDT", "SOLUSDT"]
# The certification tracks. Each Monte Carlo ordeal re-runs the indicator
# hundreds of times, so the primary series is capped — in CALENDAR terms,
# the same ~7 years on every track: a trend indicator's null is about regime
# episodes, which last months whatever the bar size, and a window with too
# few of them cannot tell timing skill from drift (the first 4h pass with a
# 4500-bar window failed every luck gate for exactly that reason).
TIMEFRAMES = ("1d", "4h", "1h", "1m")
MC_MAX_BARS = {"1d": 2500, "4h": 15_000, "1h": 60_000, "1m": 3_600_000}
# The GARCH charts are synthesised bar by bar; ~5.5 years on every track.
GARCH_BARS = {"1d": 2000, "4h": 12_000, "1h": 48_000, "1m": 2_880_000}


def s_map(sharpe: float, lo: float = -0.5, hi: float = 1.5) -> float:
    """Map a cost-adjusted Sharpe onto [0, 100]."""
    return float(np.clip((sharpe - lo) / (hi - lo), 0, 1) * 100)


def agreement(a: pd.Series, b: pd.Series) -> float:
    """Flip agreement on bars where both streams have committed (nonzero)."""
    j = a.index.intersection(b.index)
    x, y = a.reindex(j), b.reindex(j)
    m = (x != 0) & (y != 0)
    return float((x[m] == y[m]).mean()) if m.any() else 0.0


def cost_for(key: str) -> float:
    return backtest.DEFAULT_COST_BPS[asset_class(key)]


def find_primary(timeframe: str = "1d") -> str:
    keys = available()
    for ex in VENUE_PREF:
        for pat in (f"{ex}_BTCUSDT_{timeframe}", f"{ex}_BTCUSD_{timeframe}"):
            if pat in keys:
                return pat
    raise RuntimeError(f"no BTC {timeframe} series on the shelf — refresh the shelf first")


def available_timeframes() -> list[str]:
    """The tracks the shelf can run: those with a BTC primary series."""
    out = []
    for tf in TIMEFRAMES:
        try:
            find_primary(tf)
            out.append(tf)
        except RuntimeError:
            continue
    return out


def grade(score: float, perm_p: float | None) -> str:
    g = "S" if score >= 85 else "A" if score >= 70 else "B" if score >= 55 else \
        "C" if score >= 40 else "F"
    if perm_p is not None and perm_p > 0.10 and g in ("S", "A"):
        g = "B (hard-capped: permutation p > 0.10)"
    return g


class Gauntlet:
    def __init__(self, ind: TrendIndicator, seed: int = 42,
                 perm_n: int = 120, boot_n: int = 200, garch_n: int = 100,
                 fast: bool = False, progress=None, timeframe: str = "1d"):
        self.ind = ind
        self.seed = seed
        self.fast = bool(fast)
        if timeframe not in TIMEFRAMES:
            raise ValueError(f"unknown certification track '{timeframe}' — one of {TIMEFRAMES}")
        self.timeframe = timeframe
        if fast:
            perm_n, boot_n, garch_n = 25, 40, 20
        self.perm_n, self.boot_n, self.garch_n = perm_n, boot_n, garch_n
        if any(not isinstance(n, int) or n < 1 or n > 5000 for n in (perm_n, boot_n, garch_n)):
            raise ValueError("Monte Carlo counts must be integers in [1, 5000]")
        self.rng = np.random.default_rng(seed)
        self.sections: list[str] = []
        self.scores: dict[str, float] = {}
        self.perm_p: float | None = None
        # Optional callback for the suite's Smithery page: called with a dict
        # carrying `stage` ("contract" | "axis") as each step finishes. The
        # printed lines stay — the vault scripts read them.
        self.progress = progress
        self.coverage_complete = True

    def _report(self, **payload) -> None:
        if self.progress is not None:
            try:
                self.progress(payload)
            except Exception:  # noqa: BLE001 — a broken listener must not fail a certification
                pass

    # ------------------------------------------------------------ helpers
    def _bt(self, df: pd.DataFrame, cost: float) -> dict:
        return backtest.run(df, self.ind.signal(df), cost_bps=cost)

    def _md_table(self, header: list[str], rows: list[list]) -> str:
        out = ["| " + " | ".join(header) + " |",
               "|" + "---|" * len(header)]
        for r in rows:
            out.append("| " + " | ".join(
                f"{v:.2f}" if isinstance(v, float) else str(v) for v in r) + " |")
        return "\n".join(out)

    # ------------------------------------------------------------ axes
    def axis_validators(self, df: pd.DataFrame) -> str:
        sub = df.iloc[-1500:] if len(df) > 1500 else df
        ok_c, msg_c = validate_causality(self.ind, sub)
        ok_s, msg_s = validate_scale_invariance(self.ind, sub)
        if not ok_c:
            raise RuntimeError(f"CONTRACT VIOLATION (Law 1): {msg_c}")
        if not ok_s:
            raise RuntimeError(f"CONTRACT VIOLATION (Law 2): {msg_s}")
        return f"- Law 1 (causality): ✅ {msg_c}\n- Law 2 (scale): ✅ {msg_s}"

    def axis_asset(self, primary: str) -> float:
        avail = available()
        keys, missing = [primary], []
        for sym in ASSET_SYMBOLS:
            for ex in VENUE_PREF:
                k = f"{ex}_{sym}_{self.timeframe}"
                if k in avail:
                    keys.append(k)
                    break
            else:
                missing.append(sym)
        if missing:
            self.coverage_complete = False
        rows, sharpes, cat_pen = [], [], 0
        bts = par.pmap(par.eval_backtest,
                       [(self.ind, load(k), cost_for(k)) for k in keys],
                       min_parallel=2)
        for k, b in zip(keys, bts):
            sharpes.append(b["sharpe"])
            catastrophic = b["max_dd"] < 1.3 * b["bh_max_dd"]   # dd is negative
            if catastrophic:
                cat_pen += 15
            rows.append([k, b["n_bars"], b["sharpe"], b["bh_sharpe"], b["max_dd"],
                         b["bh_max_dd"], b["flips_per_year"],
                         "⚠️ catastrophic-dd" if catastrophic else "ok"])
        breadth = float(np.mean([s > 0 for s in sharpes]))
        med = float(np.median(sharpes))
        score = float(np.clip(50 * breadth + 0.5 * s_map(med) - cat_pen, 0, 100))
        miss_note = (f"\n\n⚠️ missing from shelf (run `fetch_data.py`): "
                     + ", ".join(missing)) if missing else ""
        self.sections.append(
            "## Axis 1 — Asset robustness\n\n"
            + self._md_table(["series", "bars", "sharpe", "b&h sharpe", "maxDD",
                              "b&h maxDD", "flips/yr", "flag"], rows)
            + miss_note
            + f"\n\nbreadth (Sharpe>0): **{breadth:.0%}** · median Sharpe: **{med:.2f}**"
              f" · catastrophe penalty: −{cat_pen} → **{score:.0f}/100**")
        return score

    def axis_exchange(self) -> float:
        keys = [k for k in available()
                if k.endswith(f"_{self.timeframe}") and k.split("_")[1] in ("BTCUSDT", "BTCUSD")]
        if len(keys) < 2:
            self.coverage_complete = False
            self.sections.append("## Axis 2 — Exchange robustness\n\n_skipped: <2 venues on shelf_")
            return np.nan
        sigs, rows, sharpes = {}, [], {}
        frames = {k: load(k) for k in keys}
        start = max(f.index[0] for f in frames.values())
        raw = par.pmap(par.eval_signal,
                       [(self.ind, frames[k]) for k in keys], min_parallel=2)
        for k, arr in zip(keys, raw):
            df = frames[k]
            sigs[k] = par.rehydrate(arr, df.index)
            dfc = df[df.index >= start]
            sharpes[k] = backtest.sharpe_only(dfc, sigs[k].reindex(dfc.index), cost_for(k))
        agr = []
        for i, a in enumerate(keys):
            for b in keys[i + 1:]:
                pa = agreement(sigs[a][sigs[a].index >= start], sigs[b][sigs[b].index >= start])
                agr.append(pa)
                rows.append([f"{a} vs {b}", pa, abs(sharpes[a] - sharpes[b])])
        mean_agr = float(np.mean(agr))
        spread = float(max(sharpes.values()) - min(sharpes.values()))
        score = float(75 * np.clip((mean_agr - 0.70) / 0.25, 0, 1)
                      + 25 * (1 - np.clip(spread / 1.5, 0, 1)))
        self.sections.append(
            "## Axis 2 — Exchange robustness\n\n"
            + self._md_table(["venue pair", "signal agreement", "sharpe |Δ|"], rows)
            + f"\n\nmean agreement: **{mean_agr:.1%}** (target ≥90%) · Sharpe spread: "
              f"**{spread:.2f}** → **{score:.0f}/100**")
        return score

    def axis_parameter(self, primary: str) -> float:
        df = load(primary)
        cost = cost_for(primary)
        base_sig = self.ind.signal(df)
        base_sharpe = backtest.sharpe_only(df, base_sig, cost)
        rows, plateaus, stabs = [], [], []
        perts = []
        for pname, (lo, hi) in self.ind.param_space.items():
            base_v = self.ind.params[pname]
            for mult in (0.5, 0.75, 0.9, 1.1, 1.25, 1.5):
                v = float(np.clip(base_v * mult, lo, hi))
                if isinstance(base_v, int):
                    v = int(round(v))
                perts.append((pname, base_v, v))
        results = par.pmap(
            par.eval_sharpe_signal,
            [(self.ind.with_params(**{pn: v}), df, cost) for pn, _, v in perts])
        by_param: dict[str, tuple[list, list]] = {}
        for (pname, base_v, v), (sh, arr, idx) in zip(perts, results):
            ag = agreement(base_sig, par.rehydrate(arr, idx))
            by_param.setdefault(pname, ([base_sharpe], []))
            by_param[pname][0].append(sh)
            by_param[pname][1].append(ag)
            rows.append([pname, f"{base_v} → {v}", sh, ag])
        for pname, (neigh, agrs) in by_param.items():
            med = np.median(neigh)
            plateau = float(np.clip(min(neigh) / med, 0, 1)) if med > 0 else 0.0
            plateaus.append(plateau)
            stabs.append(float(np.mean(agrs)))
        plateau = float(np.mean(plateaus)) if plateaus else 1.0
        stab = float(np.mean(stabs)) if stabs else 1.0
        score = float(np.clip(50 * plateau + 50 * np.clip((stab - 0.6) / 0.4, 0, 1), 0, 100))
        self.sections.append(
            "## Axis 3 — Parameter robustness\n\n"
            f"base parameterization: `{self.ind.params}` · base Sharpe **{base_sharpe:.2f}**\n\n"
            + self._md_table(["param", "perturbation", "sharpe", "signal agreement"], rows)
            + f"\n\nplateau score (min/median): **{plateau:.2f}** · mean signal stability: "
              f"**{stab:.1%}** → **{score:.0f}/100**")
        return score

    def axis_temporal(self, primary: str) -> float:
        df = load(primary)
        b = self._bt(df, cost_for(primary))
        pnl = b["pnl"]
        ppy = backtest.periods_per_year(df.index)
        if len(pnl) < int(ppy):
            self.sections.append("## Axis 4 — Temporal robustness\n\n"
                                 "Incomplete: at least one calendar year of candles is required for rolling-year checks.")
            return np.nan
        block = int(2 * ppy)
        rows, era_pos, era_sh = [], [], []
        for s in range(0, len(pnl) - block // 2, block):
            seg = pnl.iloc[s:s + block]
            sh = float(seg.mean() / seg.std() * np.sqrt(ppy)) if seg.std() > 0 else 0.0
            era_pos.append(seg.sum() >= 0); era_sh.append(sh)
            rows.append([f"{pnl.index[s].date()} → {pnl.index[min(s + block, len(pnl)) - 1].date()}",
                         float(np.expm1(seg.sum())), sh])
        roll = pnl.rolling(int(ppy)).mean() / pnl.rolling(int(ppy)).std()
        frac_roll = float((roll.dropna() > 0).mean())
        score = float(np.clip(50 * np.mean(era_pos) + 0.30 * s_map(min(era_sh))
                              + 20 * frac_roll, 0, 100)) if era_sh else 0.0
        self.sections.append(
            "## Axis 4 — Temporal robustness (eras)\n\n"
            + self._md_table(["era", "return", "sharpe"], rows)
            + f"\n\neras non-negative: **{np.mean(era_pos):.0%}** · worst era Sharpe: "
              f"**{min(era_sh):.2f}** · rolling-1y Sharpe > 0: **{frac_roll:.0%}**"
              f" → **{score:.0f}/100**")
        return score

    def axis_monte_carlo(self, primary: str) -> float:
        df = load(primary)
        cap = MC_MAX_BARS.get(self.timeframe, 2500)
        if self.fast and self.timeframe == "1m":
            cap = min(cap, 60_000)  # bounded smoke test, never certification
        if len(df) > cap:
            df = df.iloc[-cap:]
        cost = cost_for(primary)
        n = len(df)
        real = self._bt(df, cost)
        real_total = real["total_log_ret"]
        lowpower = (" ⚠️ LOW-POWER (<500 bars)" if n < 500 else "") + \
            f" — window {n} bars ({df.index[0].date()} → {df.index[-1].date()})"

        # Simulated charts are generated serially in the parent (same RNG
        # draw order as the original serial code -> bit-identical results);
        # only the expensive signal+backtest evaluations fan out.

        # 1 — permutation (the luck detector)
        sims = (mc.rebuild_ohlc(df, mc.permutation_order(n, self.rng))
                for _ in range(self.perm_n))
        perm_res = par.simulated_results(self.ind, sims, cost, n)
        beats = sum(1 for total, _ in perm_res if total >= real_total)
        p = (1 + beats) / (1 + self.perm_n)
        self.perm_p = p
        sc_perm = float(np.clip(100 * (1 - (p - 0.01) / 0.19), 0, 100))

        # 2 — stationary bootstrap (unlucky-history scenario)
        sims = (mc.rebuild_ohlc(df, mc.stationary_bootstrap_order(n, self.rng))
                for _ in range(self.boot_n))
        boots = [sh for _, sh in
                 par.simulated_results(self.ind, sims, cost, n)]
        p5 = float(np.percentile(boots, 5))
        sc_boot = s_map(p5, lo=-1.0, hi=1.0)

        # 3 — GARCH synthetic histories (charts that could have happened)
        r = np.diff(np.log(df["close"].to_numpy(dtype=float)))
        g = mc.fit_garch_t(r)
        reg = mc.calibrate_drift_regimes(r)
        sims = (mc.garch_synthetic(df, min(n, GARCH_BARS.get(self.timeframe, 2000)), self.rng, garch=g, regimes=reg)
                for _ in range(self.garch_n))
        gs = [sh for _, sh in
              par.simulated_results(self.ind, sims, cost, n)]
        g_med, g_frac = float(np.median(gs)), float(np.mean([s > 0 for s in gs]))
        sc_garch = float(0.5 * s_map(g_med) + 50 * g_frac)

        # 4 — microstructure noise injection (degradation curve)
        base_sig = self.ind.signal(df)
        amps = (0.05, 0.10, 0.20, 0.35, 0.50)
        noisy = [mc.noise_inject(df, amp, np.random.default_rng(self.seed + int(amp * 100)))
                 for amp in amps]
        noise_sigs = par.pmap(par.eval_signal, [(self.ind, nd) for nd in noisy])
        curve = [(amp, agreement(base_sig, par.rehydrate(arr, df.index)))
                 for amp, arr in zip(amps, noise_sigs)]
        agr20 = dict((round(a, 2), v) for a, v in curve)[0.20]
        sc_noise = float(100 * np.clip((agr20 - 0.60) / 0.35, 0, 1))

        score = float(0.35 * sc_perm + 0.20 * sc_boot + 0.25 * sc_garch + 0.20 * sc_noise)
        curve_str = " · ".join(f"{int(a*100)}%→{v:.0%}" for a, v in curve)
        self.sections.append(
            f"## Axis 5 — Monte Carlo robustness{lowpower}\n\n"
            + self._md_table(
                ["ordeal", "n", "result", "sub-score"],
                [["permutation test", self.perm_n, f"p = {p:.3f}", sc_perm],
                 ["stationary bootstrap", self.boot_n,
                  f"5th-pct Sharpe = {p5:.2f} (median {np.median(boots):.2f})", sc_boot],
                 ["GARCH-t synthetic charts", self.garch_n,
                  f"median Sharpe = {g_med:.2f}, {g_frac:.0%} profitable", sc_garch],
                 ["noise injection", 5, f"agreement curve: {curve_str}", sc_noise]])
            + f"\n\nGARCH fit: α={g['alpha']:.3f} β={g['beta']:.3f} ν={g['nu']:.1f} · "
              f"drift regimes μ=({reg['mu'][0]:.5f}, {reg['mu'][1]:.5f}) p_stay={reg['p_stay']:.3f}"
              f"\n\n→ **{score:.0f}/100**")
        return score

    # ------------------------------------------------------------ run
    def run(self) -> dict:
        t0 = time.time()
        self.sections, self.scores, self.perm_p = [], {}, None
        self.coverage_complete = True
        self.rng = np.random.default_rng(self.seed)
        primary = find_primary(self.timeframe)
        df = load(primary)
        self._report(stage="contract", status="running", primary=primary, bars=len(df),
                     timeframe=self.timeframe)
        try:
            contract_md = self.axis_validators(df)
        except RuntimeError as e:
            self._report(stage="contract", status="failed", primary=primary, bars=len(df), message=str(e))
            raise
        self._report(stage="contract", status="passed", primary=primary, bars=len(df))

        axes = [("asset", lambda: self.axis_asset(primary)),
                ("exchange", self.axis_exchange),
                ("parameter", lambda: self.axis_parameter(primary)),
                ("temporal", lambda: self.axis_temporal(primary)),
                ("monte_carlo", lambda: self.axis_monte_carlo(primary))]
        for name, fn in axes:
            self._report(stage="axis", axis=name, status="running", weight=WEIGHTS[name])
            t_axis = time.time()
            try:
                self.scores[name] = fn()
            except Exception as e:  # noqa: BLE001 — one axis failing must not kill the run
                self.scores[name] = np.nan
                self.sections.append(f"## Axis — {name}\n\n_failed: {type(e).__name__}: {e}_")
            finite = bool(np.isfinite(self.scores[name]))
            print(f"  axis {name}: {self.scores[name]:.0f}" if finite
                  else f"  axis {name}: skipped", flush=True)
            self._report(stage="axis", axis=name, status="done" if finite else "skipped",
                         score=(round(float(self.scores[name]), 1) if finite else None),
                         weight=WEIGHTS[name], perm_p=self.perm_p if name == "monte_carlo" else None,
                         elapsed_s=round(time.time() - t_axis, 1))

        # Missing axes cannot increase the remaining axes' weights.
        valid = {k: v for k, v in self.scores.items() if np.isfinite(v)}
        total = sum(WEIGHTS[k] * v for k, v in valid.items())
        counts = {"perm": self.perm_n, "boot": self.boot_n, "garch": self.garch_n}
        reasons = certification_gates(self.scores, self.perm_p, counts, fast=self.fast,
                                      coverage=self.coverage_complete, required_axes=WEIGHTS)
        if total < 70:
            reasons.append("Robustness score is below 70/100")
        certified = not reasons
        g = grade(total, self.perm_p)

        today = dt.date.today().isoformat()
        track = "" if self.timeframe == "1d" else f" — {self.timeframe} track"
        head = (f"# {self.ind.name} — Gauntlet Certification{track}\n\n"
                f"**date:** {today} · **timeframe:** {self.timeframe} · **primary series:** `{primary}` "
                f"({len(df)} bars) · **seed:** {self.seed} · "
                f"**MC counts:** perm={self.perm_n} boot={self.boot_n} garch={self.garch_n} · "
                f"**runtime:** {time.time() - t0:.0f}s\n\n"
                f"**params:** `{self.ind.params}`\n\n"
                f"## Contract validators\n\n{contract_md}\n")
        table = self._md_table(
            ["axis", "weight", "sub-score"],
            [[k, WEIGHTS[k], (f"{v:.0f}" if np.isfinite(v) else "skipped")]
             for k, v in self.scores.items()])
        # Reduced MC counts are a smoke test: the report and the ledger say so,
        # and the verdict line never reads as a certification.
        smoke = ("\n\n⚠️ **Fast run** (reduced Monte Carlo counts) — a smoke test, not a certification."
                 if self.fast else "")
        verdict = (f"\n## Verdict\n\n{table}\n\n"
                   f"# Robustness Score: **{total:.0f}/100** — Grade **{g}**\n\n"
                   + ("⚔️ **Battle-Ready**" if certified
                      else "🔨 **Back to the forge** — see failing axes above" if not self.fast
                      else "🧪 **Smoke test** — run the full gauntlet for a verdict")
                   + smoke
                   + ("\n\nCertification gates:\n" + "\n".join(f"- {r}" for r in reasons) if reasons else ""))
        report = head + "\n" + "\n\n".join(self.sections) + verdict + "\n"

        OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
        # The report name carries the track and the smoke flag in one
        # parenthesis — never square brackets, which would break the ledger's
        # wikilinks: `… Gauntlet (4h).md`, `… Gauntlet (1h, fast).md`.
        run_id = artifact_id()
        tags = ([] if self.timeframe == "1d" else [self.timeframe]) + (["fast"] if self.fast else []) + [run_id]
        suffix = f" ({', '.join(tags)})" if tags else ""
        path = OUTPUT_DIR / f"{today} {self.ind.name} Gauntlet{suffix}.md"
        with path.open("x", encoding="utf-8") as handle:
            handle.write(report)
        write_json(path.with_suffix(".json"), {
            "version": EVALUATION_VERSION, "run_id": run_id, "kind": "gauntlet",
            "name": self.ind.name, "params": self.ind.params, "timeframe": self.timeframe,
            "code_sha256": code_fingerprint(), "primary": primary,
            "primary_sha256": frame_fingerprint(df), "seed": self.seed, "counts": counts,
            "score": total, "grade": g, "scores": self.scores, "perm_p": self.perm_p,
            "certified": certified, "reasons": reasons, "fast": self.fast,
            "note": "Legacy five-axis signal score; run Comparison for next-open execution and held-out checks.",
        })
        label = self.ind.name + ("" if self.timeframe == "1d" else f" ({self.timeframe})")
        self._append_ledger(today, total, g + (" (fast — not a certification)" if self.fast else ""),
                            path.name, label)
        return {"score": total, "grade": g, "perm_p": self.perm_p, "timeframe": self.timeframe,
                "scores": self.scores, "report": str(path), "fast": self.fast,
                "certified": certified, "reasons": reasons, "version": EVALUATION_VERSION}

    def _append_ledger(self, today: str, total: float, g: str, fname: str, label: str | None = None) -> None:
        reg = DOCS_DIR / "04 Indicator Registry.md"
        try:
            txt = reg.read_text(encoding="utf-8")
            anchor = "| Date | Indicator | Score | Grade | Report |"
            i = txt.index(anchor)
            j = txt.index("\n", txt.index("\n", i) + 1)   # end of separator line
            row = f"\n| {today} | {label or self.ind.name} | {total:.0f} | {g} | [[{fname[:-3]}]] |"
            reg.write_text(txt[:j] + row + txt[j:], encoding="utf-8")
        except Exception:  # noqa: BLE001 — ledger is cosmetic; never fail the run
            pass
