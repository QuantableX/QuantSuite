"""Fast evaluation of many single and aggregate trend signals in one saved system.

The unchanged BacktestEngine supplies every input: it runs up to its
simulation step once per warm-up group and hands over the universe
snapshots, the candles cut to that warm-up and the TOTAL gate. Each
candidate's +1/-1/0 vote on every pair is then computed once per group with
the engine's own signal code, and singles and aggregates are simulated in
bulk with the engine's bar loop in the engine's order of float operations,
so equity curves are bit-identical. ``verify`` re-runs candidates through
the native engine; callers report nothing that was not verified.

Singles follow ``pair_signal``: "A beats B" when the vote is +1, otherwise B.
Aggregates follow ``aggregate_signal``: the sign of the members' vote sum
with the previous verdict held at an even split and before any member has
committed (``hysteresis_flip`` at a band of 0).
"""
from __future__ import annotations

import json
import math
import multiprocessing as mp
import os
import pickle
import shutil
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import asdict, dataclass, replace
from pathlib import Path

import numpy as np
import pandas as pd

USD = "USD"
NONE = -1


class Captured(Exception):
    """Raised by the probe engine with the inputs of its simulation step."""


def _probe_engine(engine):
    from rotation_lab.backtest.engine import BacktestEngine

    class Probe(BacktestEngine):
        def _simulate(self, config, universe, frames, notes, gate=None, *, parallel_pairs=False):
            raise Captured(config, universe, frames, gate)

    return Probe(registry=engine.registry, ohlcv=engine.ohlcv)


def unsupported_reason(config):
    """Why a system cannot be evaluated in bulk (None when it can)."""
    from rotation_lab.config import IndicatorConfig, TotalBreakoutConfig
    if config.mode == "single_asset":
        return None
    if config.mode != "rotation":
        return f"System mode {config.mode} has no bulk evaluator"
    if config.market_filter and not isinstance(config.market_indicator, (IndicatorConfig, TotalBreakoutConfig)):
        # The gate would follow each candidate's own signal.
        return "The market filter uses the candidate's own indicator"
    return None


def indicator_for(config, members):
    """The engine's indicator for a single key or an aggregate of keys."""
    from rotation_lab.config import IndicatorConfig
    members = tuple(members)
    return IndicatorConfig(trend=members[0] if len(members) == 1 else "aggregate",
                           aggregate=members if len(members) > 1 else (), params=config.indicator.params)


def _votes_task(task):
    """One pair of one warm-up group: every eligible key's +1/-1/0 vote."""
    from rotation_lab.backtest.signals import IndicatorDataUnavailable, _smithery, ratio_frame
    from rotation_lab.config import IndicatorConfig
    group, pair, keys, params, a, b, single = task
    frame = a if single else ratio_frame(a, b)
    values = np.zeros((len(keys), len(frame)), dtype=np.int8)
    errors = {}
    if len(frame):
        for i, key in enumerate(keys):
            try:
                signal = np.asarray(_smithery(frame, key, IndicatorConfig(trend=key, params=params)), dtype=float)
            except IndicatorDataUnavailable as exc:
                errors[key] = "unsupported: " + str(exc) + ("" if single or b is None else "; A/B price ratios have no traded volume")
                continue
            except Exception as exc:  # noqa: BLE001 — the native run would fail the same way
                errors[key] = f"error: {type(exc).__name__}: {exc}"
                continue
            if signal.shape != (len(frame),) or not np.isin(signal, (-1.0, 0.0, 1.0)).all():
                errors[key] = "unsupported: the signal is not +1/-1/0 on every bar"
                continue
            values[i] = signal.astype(np.int8)
    return group, pair, frame.index, values, errors


def _worker_init():
    # stdout may be the forge's JSON-lines pipe; indicator prints go to stderr.
    sys.stdout = sys.stderr
    for name in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS"):
        os.environ.setdefault(name, "1")


_EVALUATOR = None


def _eval_init(folder):
    global _EVALUATOR
    _worker_init()
    _EVALUATOR = FastSystem.load(folder)


def _eval_task(trials):
    curve, _ = _EVALUATOR.simulate(trials)
    metrics = FastSystem.metrics(curve)
    return [(float(m), float(d)) for m, d in zip(metrics["net_return_multiplier"], metrics["max_drawdown_pct"])]


def default_workers():
    return max(1, min(24, int(os.environ.get("SMITHERY_WORKERS", (os.cpu_count() or 4) - 2))))


@dataclass
class Group:
    start: str            # the warm-up's fetch start (ISO date)
    keys: list            # candidate keys whose own warm-up fits in this group
    file: str             # votes: (len(keys), pairs, full bars) int8, .npy
    first: int            # position of the group's first bar in the full bars
    gate: list | None     # TOTAL gate on the master bars (rotation with a market filter)


# What a worker process needs to simulate from the stored votes.
_STATE = ("config", "keys", "failed", "key_start", "groups", "pairs", "full", "dates", "mpos", "single",
          "cadence", "assets", "rotation", "returns")


class FastSystem:
    """Bulk, engine-identical simulation of candidates in one saved system.

    ``engine`` is a BacktestEngine whose ranking and candle services answer
    every request identically (lab_compare.FrozenService), so captured inputs
    and native verification runs see the same data.
    """

    def __init__(self, config, keys, folder, engine, *, workers=None, progress=None):
        self.config = replace(config, compare_trends=())
        self.engine = engine
        self.keys = list(dict.fromkeys(keys))
        self.folder = Path(folder)
        self.workers = workers or default_workers()
        self.progress = progress or (lambda message, fraction=None: None)
        self.single = self.config.mode == "single_asset"
        self.cadence = self.config.bar_cadence if self.single else self.config.cadence
        self.failed = {}
        self.groups = []
        self._votes = {}
        self._pool = None
        self.rotation = self.returns = None

    # ----------------------------------------------------------------- inputs

    def config_for(self, members):
        return replace(self.config, indicator=indicator_for(self.config, members))

    def _start(self, members):
        from rotation_lab.backtest.engine import _fetch_start
        config = self.config_for(members)
        return _fetch_start(replace(config, cadence=self.cadence))

    def build(self):
        reason = unsupported_reason(self.config)
        if reason:
            raise ValueError(reason)
        from smithery.indicators import REGISTRY
        missing = [k for k in self.keys if k not in REGISTRY]
        if missing:
            raise ValueError(f"Unknown indicators: {missing}")
        if not self.keys:
            raise ValueError("No candidate indicators")
        self.folder.mkdir(parents=True, exist_ok=True)
        starts = {k: self._start((k,)) for k in self.keys}
        self.key_start = {k: s.isoformat() for k, s in starts.items()}
        group_starts = sorted(set(starts.values()))
        self.progress(f"Capturing the engine's inputs for {len(group_starts)} warm-up groups", 0.01)
        captured = {s: self._capture(next(k for k in self.keys if starts[k] == s)) for s in group_starts}
        if self.single:
            self._prepare_single(captured[group_starts[0]])
        else:
            self._prepare_rotation(captured, group_starts)
        for start in group_starts:
            first = int(self.full.searchsorted(self._stamp(start)))
            gate = None
            if not self.single and captured[start]["gate"] is not None:
                gate = captured[start]["gate"].reindex(self.dates, fill_value=0).to_numpy().astype(int).tolist()
            self.groups.append(Group(start.isoformat(), [k for k in self.keys if starts[k] >= start],
                                     f"votes-{start.isoformat()}.npy", first, gate))
        self._compute_votes(captured, group_starts)
        self._save_meta()
        return self

    def _capture(self, key):
        """The engine's own inputs for ``key``'s warm-up, without simulating."""
        from rotation_lab.backtest.engine import _bar_returns, _fetch_start
        from rotation_lab.data.ohlcv import confirmed_frame, utc_now
        config = self.config_for((key,))
        if self.single:
            # The single-asset path has no simulation hook: this mirrors its prefix.
            config = replace(config, cadence=self.cadence)
            market = config.single_asset
            data = self.engine.ohlcv.get_pair_series(market.exchange, market.pair, _fetch_start(config),
                                                     config.end_date, market.timeframe)
            frame = confirmed_frame(data.frame, market.timeframe, utc_now())
            if frame.empty:
                raise ValueError(f"No confirmed {market.timeframe} candles for {market.pair} on {market.exchange}")
            if ((frame[["open", "close"]] <= 0) | ~np.isfinite(frame[["open", "close"]])).any().any():
                raise ValueError("Single Asset candles contain invalid open/close prices")
            index = frame.index[(frame.index >= pd.Timestamp(config.start_date)) &
                                (frame.index < pd.Timestamp(config.end_date + pd.Timedelta(days=1)))]
            if index.empty:
                raise ValueError("No confirmed candles in the selected backtest window")
            return {"frame": frame, "index": index, "returns": _bar_returns(frame, index)}
        try:
            result = _probe_engine(self.engine).run(config)
        except Captured as exc:
            _, universe, frames, gate = exc.args
            return {"universe": universe, "frames": frames, "gate": gate}
        raise ValueError("; ".join(result.notes) or "The engine did not reach its simulation step")

    def _stamp(self, day):
        stamp = pd.Timestamp(day)
        return stamp.tz_localize(self.full.tz) if self.full.tz is not None and stamp.tz is None else stamp

    def _prepare_single(self, first):
        self.frame = first["frame"]
        self.full = self.frame.index
        self.dates = first["index"]
        self.mpos = self.full.get_indexer(self.dates)
        self.returns = first["returns"]
        self.pairs = [(self.config.single_asset.pair, None)]
        self.assets = None

    def _prepare_rotation(self, captured, group_starts):
        from rotation_lab.backtest.engine import _bar_returns
        first = captured[group_starts[0]]
        universe, frames = first["universe"], first["frames"]
        shape = [(s.on_date, [c.symbol for c in s.coins]) for s in universe.snapshots]
        for start in group_starts[1:]:
            if [(s.on_date, [c.symbol for c in s.coins]) for s in captured[start]["universe"].snapshots] != shape:
                raise ValueError("The ranking answers differ between warm-up groups")
        # The engine's master bars: the union of the coins' bars inside the window.
        indexes = iter(f.index for f in frames.values())
        master = next(indexes)
        for index in indexes:
            master = master.union(index)
        lo = pd.Timestamp(self.config.start_date, tz=master.tz)
        hi = pd.Timestamp(self.config.end_date + pd.Timedelta(days=1), tz=master.tz)
        self.dates = pd.DatetimeIndex(master[(master >= lo) & (master < hi)], freq=None)
        if self.dates.empty:
            raise ValueError("No bars in the selected window")
        self.full = master  # the earliest group's candles hold every later group's bars
        self.mpos = self.full.get_indexer(self.dates)
        self.assets = sorted({c.symbol for s in universe.snapshots for c in s.coins}) + [USD]
        cash = len(self.assets) - 1
        ids = {s: i for i, s in enumerate(self.assets)}
        include_usd = bool(self.config.include_usd)
        pairs = {}
        for snapshot in universe.snapshots:
            syms = [c.symbol for c in snapshot.coins]
            for i, a in enumerate(syms):
                if include_usd:
                    pairs.setdefault((a, None), len(pairs))
                for b in syms[i + 1:]:  # orientation follows the rank order, as in the engine
                    pairs.setdefault((a, b), len(pairs))
        self.pairs = list(pairs)
        T, P, N = len(self.dates), len(self.pairs), len(self.assets)
        active = np.zeros((P, T), dtype=bool)
        width = max((len(s.coins) for s in universe.snapshots), default=0) + int(include_usd)
        order = np.full((T, max(width, 1)), NONE, dtype=np.int16)
        in_today = np.zeros((T, N), dtype=bool)
        first_sym = np.full(T, NONE, dtype=np.int16)
        snaps = iter(universe.snapshots)
        upcoming, current = next(snaps, None), None
        for t, ts in enumerate(self.dates):
            # The engine's walk: the latest snapshot dated on or before the bar.
            while upcoming and pd.Timestamp(upcoming.on_date) <= ts:
                current, upcoming = upcoming, next(snaps, None)
            syms = [c.symbol for c in current.coins] if current else []
            if not syms:
                continue
            row = [ids[s] for s in syms] + ([cash] if include_usd else [])
            order[t, :len(row)] = row
            in_today[t, [ids[s] for s in syms]] = True
            first_sym[t] = ids[syms[0]]
            for i, a in enumerate(syms):
                if include_usd:
                    active[pairs[(a, None)], t] = True
                for b in syms[i + 1:]:
                    active[pairs[(a, b)], t] = True
        returns = np.zeros((N, T), dtype=float)
        for sym, frame in frames.items():
            if sym in ids:
                returns[ids[sym]] = _bar_returns(frame, self.dates)
        ends = np.array([[ids[a], ids[b] if b else cash] for a, b in self.pairs], dtype=np.int64).reshape(-1, 2)
        self.frames = {start: captured[start]["frames"] for start in group_starts}
        self.rotation = dict(active=active, order=order, in_today=in_today, first_sym=first_sym,
                             has_syms=first_sym >= 0, returns=returns, ends=ends)

    def _compute_votes(self, captured, group_starts):
        params = self.config.indicator.params
        tasks, arrays = [], {}
        for g, start in enumerate(group_starts):
            group = self.groups[g]
            arrays[g] = np.lib.format.open_memmap(self.folder / group.file, mode="w+", dtype=np.int8,
                                                  shape=(len(group.keys), len(self.pairs), len(self.full)))
            if self.single:
                history = self.frame.loc[self.frame.index >= self._stamp(start)]
                tasks.append((g, 0, group.keys, params, history, None, True))
                continue
            frames = self.frames[start]
            for p, (a, b) in enumerate(self.pairs):
                if a in frames and (b is None or b in frames):  # otherwise the engine's zeros: B wins
                    tasks.append((g, p, group.keys, params, frames[a], frames[b] if b else None, False))
        done, total, started = 0, len(tasks), time.monotonic()

        def store(result):
            g, p, bars, values, errors = result
            # Timestamps, not integers: pandas may hold bars at any resolution.
            positions = self.full.get_indexer(bars)
            if (positions < 0).any():
                raise ValueError("A pair's bars are outside the engine's candles")
            arrays[g][:, p, positions] = values
            for key, error in errors.items():
                self.failed.setdefault(key, error)

        def tick():
            if done % 25 == 0 or done == total:
                self.progress(f"Signals: {done:,} of {total:,} pair computations · {time.monotonic() - started:,.0f} s",
                              0.05 + 0.85 * done / max(total, 1))

        if self.workers > 1 and total > 4:
            with ProcessPoolExecutor(max_workers=min(self.workers, total), mp_context=mp.get_context("spawn"),
                                     initializer=_worker_init) as pool:
                for future in as_completed([pool.submit(_votes_task, task) for task in tasks]):
                    store(future.result())
                    done += 1
                    tick()
        else:
            for task in tasks:
                store(_votes_task(task))
                done += 1
                tick()
        for array in arrays.values():
            array.flush()

    def _save_meta(self):
        meta = {"mode": self.config.mode, "keys": self.keys, "failed": self.failed, "key_start": self.key_start,
                "groups": [{**asdict(g), "gate": None if g.gate is None else len(g.gate)} for g in self.groups],
                "pairs": [list(p) for p in self.pairs], "bars": len(self.dates), "full_bars": len(self.full),
                "first_bar": str(self.dates[0]), "last_bar": str(self.dates[-1]), "assets": self.assets}
        (self.folder / "cube.json").write_text(json.dumps(meta, default=str), encoding="utf-8")
        with (self.folder / "state.pkl").open("wb") as stream:
            pickle.dump({name: getattr(self, name) for name in _STATE}, stream)

    @classmethod
    def load(cls, folder):
        """A built evaluator from its folder (for worker processes; no engine)."""
        self = cls.__new__(cls)
        with (Path(folder) / "state.pkl").open("rb") as stream:
            self.__dict__.update(pickle.load(stream))
        self.folder, self.engine, self.workers, self._votes, self._pool = Path(folder), None, 1, {}, None
        self.progress = lambda message, fraction=None: None
        return self

    def evaluate(self, trials, chunk=96):
        """Final capital and maximum drawdown per trial (None when a member is unusable), in parallel."""
        trials = [tuple(t) for t in trials]
        out = [None] * len(trials)
        todo = [i for i, t in enumerate(trials) if not any(k in self.failed for k in t)]
        chunks = [todo[i:i + chunk] for i in range(0, len(todo), chunk)]
        if self.workers <= 1 or len(chunks) < 2:
            for part in chunks:
                for i, value in zip(part, _eval_local(self, [trials[i] for i in part])):
                    out[i] = value
            return out
        if self._pool is None:
            self._pool = ProcessPoolExecutor(max_workers=self.workers, mp_context=mp.get_context("spawn"),
                                             initializer=_eval_init, initargs=(str(self.folder),))
        futures = {self._pool.submit(_eval_task, [trials[i] for i in part]): part for part in chunks}
        for future in as_completed(futures):
            for i, value in zip(futures[future], future.result()):
                out[i] = value
        return out

    # ---------------------------------------------------------- evaluation

    @property
    def usable(self):
        return [k for k in self.keys if k not in self.failed]

    def group_of(self, members):
        start = min(self.key_start[k] for k in members)
        return next(g for g, group in enumerate(self.groups) if group.start == start)

    def _group_votes(self, g):
        if g not in self._votes:
            group = self.groups[g]
            self._votes[g] = (np.load(self.folder / group.file, mmap_mode="r"), {k: i for i, k in enumerate(group.keys)})
        return self._votes[g]

    def state(self, members):
        """Per pair and full bar: the single's raw vote, or the aggregate's held verdict."""
        g = self.group_of(members)
        votes, index = self._group_votes(g)
        rows = [index[k] for k in members]
        if len(rows) == 1:
            return votes[rows[0]], g
        total = votes[rows].sum(axis=0, dtype=np.int16)
        last = np.where(total != 0, np.arange(total.shape[1]), -1)
        np.maximum.accumulate(last, axis=1, out=last)
        state = np.sign(np.take_along_axis(total, np.maximum(last, 0), axis=1)).astype(np.int8)
        state[last < 0] = 0
        return state, g

    def simulate(self, trials):
        """Equity curves (trials × bars) and holdings of several trials (tuples of keys)."""
        trials = [tuple(t) for t in trials]
        bad = sorted({k for t in trials for k in t if k in self.failed})
        if bad:
            raise ValueError(f"Unusable candidates: {bad}")
        return self._simulate_single(trials) if self.single else self._simulate_rotation(trials)

    def _simulate_rotation(self, trials):
        a = self.rotation
        T, P, N = len(self.dates), len(self.pairs), len(self.assets)
        onehot_a = np.zeros((P, N), dtype=np.float32)
        onehot_b = np.zeros((P, N), dtype=np.float32)
        onehot_a[np.arange(P), a["ends"][:, 0]] = 1
        onehot_b[np.arange(P), a["ends"][:, 1]] = 1
        order, active = a["order"], a["active"]
        rows = np.arange(T)[:, None]
        best = np.empty((len(trials), T), dtype=np.int64)
        gates = []
        for i, members in enumerate(trials):
            state, g = self.state(members)
            bull = state[:, self.mpos] > 0
            # Pine's scoring: the winner of each active pair scores one point.
            scores = (bull & active).T.astype(np.float32) @ onehot_a + ((~bull) & active).T.astype(np.float32) @ onehot_b
            candidates = np.where(order >= 0, scores[rows, np.maximum(order, 0)], -1.0)
            best[i] = order[np.arange(T), candidates.argmax(axis=1)]  # first maximum = rank tie-break
            gates.append(self.groups[g].gate)
        return self._walk(best, gates)

    def _walk(self, best, gates):
        """The engine's bar loop, vectorized over trials, same float operations."""
        a = self.rotation
        B, T = best.shape
        cash = len(self.assets) - 1
        cost = (1.0 - self.config.fee_rate) * (1.0 - self.config.slippage_rate)
        factor = np.array([1.0, cost ** 1, cost ** 2])
        usd_in = bool(self.config.include_usd)
        gated = np.array([g is not None for g in gates])
        gate = np.array([g if g is not None else [1] * T for g in gates], dtype=np.int64).reshape(B, T)
        pending = np.full(B, NONE, dtype=np.int64)
        current = np.full(B, NONE, dtype=np.int64)
        equity = np.ones(B)
        curve = np.empty((B, T))
        held = np.empty((B, T), dtype=np.int64)
        returns, in_today, first_sym, has_syms = a["returns"], a["in_today"], a["first_sym"], a["has_syms"]
        for t in range(T):
            # A pending coin that left the universe is exited.
            forced = (pending >= 0) & (pending != cash) & ~in_today[t, np.maximum(pending, 0)]
            if forced.any():
                pending[forced] = cash if usd_in else (first_sym[t] if has_syms[t] else NONE)
            take = pending != NONE
            prev_cash = (current == NONE) | (current == cash)
            new_cash = (pending == NONE) | (pending == cash)
            legs = np.where(prev_cash & new_cash, 0, np.where(prev_cash | new_cash, 1, np.where(current == pending, 0, 2)))
            legs[~take] = 0
            equity = equity * factor[legs]
            current = np.where(take, pending, current)
            owned = (current >= 0) & (current != cash)
            equity = equity * (1.0 + np.where(owned, returns[np.maximum(current, 0), t], 0.0))
            curve[:, t] = equity
            held[:, t] = current
            if not has_syms[t]:
                pending = np.full(B, cash, dtype=np.int64) if usd_in else current.copy()
                continue
            pending = np.where(gated & (gate[:, t] != 1), cash, best[:, t])
        return curve, held

    def _simulate_single(self, trials):
        cost = (1.0 - self.config.fee_rate) * (1.0 - self.config.slippage_rate)
        factor = np.array([1.0, cost ** 1, cost ** 2])
        prior = self.mpos - 1
        positions = np.zeros((len(trials), len(self.dates)), dtype=np.int64)
        for i, members in enumerate(trials):
            state, g = self.state(members)
            # signal.shift(1) on the warm-up's own history, then the window.
            ok = prior >= self.groups[g].first
            positions[i, ok] = state[0, prior[ok]]
        positions = np.clip(positions, -1, 1)
        if self.config.single_asset.direction == "long_cash":
            positions = np.clip(positions, 0, None)
        B, T = positions.shape
        equity, previous = np.ones(B), np.zeros(B, dtype=np.int64)
        curve = np.empty((B, T))
        for t in range(T):
            position = np.where(equity <= 0, 0, positions[:, t])
            positions[:, t] = position
            equity = equity * factor[np.abs(position - previous)]
            equity = equity * np.maximum(0.0, 1 + position * self.returns[t])
            curve[:, t] = equity
            previous = position
        return curve, positions

    def labels(self, held):
        """Held asset names per bar, as the engine's ``held_asset`` shows them."""
        if self.single:
            base, quote = self.config.single_asset.pair.split("/")
            names = {1: base, -1: f"Short {base}", 0: quote}
            return [names[int(v)] for v in held]
        return [None if v == NONE else self.assets[v] for v in held]

    @staticmethod
    def metrics(curve):
        """Final capital and maximum drawdown for many curves (compute_metrics' definitions)."""
        peak = np.maximum.accumulate(curve, axis=1)
        drawdown = ((peak - curve) / peak).max(axis=1)
        return {"net_return_multiplier": curve[:, -1], "max_drawdown_pct": drawdown * 100}

    def series(self, members):
        """One trial as the engine reports it: equity and held Series, and its metrics."""
        from rotation_lab.backtest.metrics import compute_metrics
        curve, held = self.simulate([tuple(members)])
        equity = pd.Series(curve[0], index=self.dates, name="equity")
        holdings = pd.Series(self.labels(held[0]), index=self.dates, name="held").astype(object)
        return equity, holdings, compute_metrics(equity, self.cadence.bars_per_year)

    # ---------------------------------------------------------- verification

    def native(self, members):
        return self.engine.run(self.config_for(tuple(members)))

    def verify(self, members):
        """Re-run ``members`` natively; held assets must match on every bar."""
        equity, holdings, metrics = self.series(members)
        result = self.native(members)
        def names(values):  # None and NaN both mean "nothing held yet"
            return [None if v is None or (isinstance(v, float) and math.isnan(v)) else str(v) for v in values]
        native_held, fast_held = names(result.held_asset), names(holdings)
        same_bars = result.equity_strategy.index.equals(equity.index)
        mismatches = sum(x != y for x, y in zip(native_held, fast_held)) + abs(len(native_held) - len(fast_held))
        final = float(result.equity_strategy.iloc[-1]) if len(result.equity_strategy) else float("nan")
        error = abs(float(equity.iloc[-1]) / final - 1) if final and math.isfinite(final) else float("inf")
        return {"members": list(members), "same_bars": bool(same_bars), "holding_mismatches": int(mismatches),
                "capital_relative_error": error,
                "bit_identical": bool(same_bars and np.array_equal(result.equity_strategy.to_numpy(), equity.to_numpy())),
                "passed": bool(same_bars and mismatches == 0 and error < 1e-10),
                "metrics": asdict(result.metrics_strategy), "fast_metrics": asdict(metrics)}

    def close(self, remove=True):
        if self._pool is not None:
            self._pool.shutdown(wait=True, cancel_futures=True)
            self._pool = None
        self._votes.clear()
        if remove and self.folder.exists():
            shutil.rmtree(self.folder, ignore_errors=True)


def _eval_local(system, trials):
    curve, _ = system.simulate(trials)
    metrics = FastSystem.metrics(curve)
    return [(float(m), float(d)) for m, d in zip(metrics["net_return_multiplier"], metrics["max_drawdown_pct"])]
