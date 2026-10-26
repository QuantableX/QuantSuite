"""Seeded, finite searches over single indicators and aggregates of distinct bases.

Every stage is fixed before its results are seen: exhaustive where the space
fits the budget, otherwise a seeded sample in a fixed hash order. Adaptive
rounds start from the leaders of the stages before them, so a run with the same
inputs and seed is reproduced exactly. Each base casts at most one vote.
"""
from __future__ import annotations

import hashlib
import itertools
import math
import random


def ordered(trials, salt):
    """A fixed pseudo-random order that does not depend on evaluation results."""
    return sorted(trials, key=lambda t: hashlib.sha256((salt + "|" + "+".join(t)).encode()).digest())


class Search:
    def __init__(self, evaluate, keys, bases, *, seed, budget, progress=None):
        self.evaluate = evaluate          # list of trials -> list of (capital multiple, max drawdown %) or None
        self.keys = sorted(keys)
        self.bases = {k: bases[k] for k in self.keys}
        self.by_base = {}
        for key in self.keys:
            self.by_base.setdefault(self.bases[key], []).append(key)
        self.base_names = sorted(self.by_base)
        self.seed, self.budget = seed, budget
        self.progress = progress or (lambda message, fraction=None: None)
        self.seen = {}
        self.stages = []
        self.aggregates = 0

    # ---------------------------------------------------------------- core

    def valid(self, trial):
        return len({self.bases[k] for k in trial}) == len(trial)

    def remaining(self):
        return max(0, self.budget - self.aggregates)

    def run(self, name, trials, cap=None, singles=False):
        """Evaluate the stage's not yet seen, valid trials (at most ``cap`` and the budget)."""
        fresh, proposed = [], 0
        for trial in trials:
            trial = tuple(sorted(trial))
            proposed += 1
            if trial in self.seen or not self.valid(trial):
                continue
            self.seen[trial] = None
            fresh.append(trial)
        limit = len(fresh) if singles else min(len(fresh), self.remaining() if cap is None else min(cap, self.remaining()))
        for trial in fresh[limit:]:
            del self.seen[trial]
        fresh = fresh[:limit]
        self.progress(f"{name}: {len(fresh):,} candidates", None)
        for start in range(0, len(fresh), 4096):
            part = fresh[start:start + 4096]
            for trial, value in zip(part, self.evaluate(part)):
                self.seen[trial] = value
            self.progress(f"{name}: {min(start + 4096, len(fresh)):,} of {len(fresh):,} evaluated", None)
        if not singles:
            self.aggregates += len(fresh)
        self.stages.append({"stage": name, "proposed": proposed, "tested": len(fresh)})
        return fresh

    # ------------------------------------------------------------- results

    def results(self, size=None, min_size=1):
        return [(t, v) for t, v in self.seen.items() if v is not None and len(t) >= min_size and (size is None or len(t) == size)]

    def leaders(self, n, min_size=2, size=None):
        rows = self.results(size, min_size)
        rows.sort(key=lambda r: (-r[1][0], len(r[0]), r[0]))
        return rows[:n]

    def best_by_size(self):
        best = {}
        for trial, value in self.results(min_size=2):
            size = len(trial)
            if size not in best or (-value[0], trial) < (-best[size][1][0], best[size][0]):
                best[size] = (trial, value)
        return dict(sorted(best.items()))

    def counts(self):
        out = {}
        for trial, value in self.seen.items():
            out[len(trial)] = out.get(len(trial), 0) + 1
        return dict(sorted(out.items()))

    # -------------------------------------------------------- candidate sets

    def all_of_size(self, size):
        """Every valid trial of ``size`` (one version per base), lazily."""
        for bases in itertools.combinations(self.base_names, size):
            yield from itertools.product(*(self.by_base[b] for b in bases))

    def count_of_size(self, size):
        total = 0
        for bases in itertools.combinations(self.base_names, size):
            total += math.prod(len(self.by_base[b]) for b in bases)
            if total > 50_000_000:
                return total
        return total

    def sample(self, size, count, salt):
        rng = random.Random(f"{self.seed}|{salt}|{size}")
        draws, tries = set(), 0
        while len(draws) < count and tries < count * 40:
            tries += 1
            bases = rng.sample(self.base_names, size)
            draws.add(tuple(sorted(rng.choice(self.by_base[b]) for b in bases)))
        return sorted(draws)

    def neighbors(self, trial):
        """Add one base, remove one member (keeping 2+), or swap one member for another base."""
        trial = tuple(sorted(trial))
        used = {self.bases[k] for k in trial}
        if len(trial) > 2:
            for k in trial:
                yield tuple(x for x in trial if x != k)
        for key in self.keys:
            if self.bases[key] not in used:
                yield trial + (key,)
        for k in trial:
            rest = tuple(x for x in trial if x != k)
            rest_bases = used - {self.bases[k]}
            for key in self.keys:
                if key != k and self.bases[key] not in rest_bases:
                    yield rest + (key,)

    def stage_exhaustive_or_sample(self, name, size, share):
        """All trials of ``size`` when they fit ``share`` of the budget left, else a seeded sample."""
        cap = max(1, int(self.remaining() * share))
        if self.count_of_size(size) <= cap:
            return self.run(name, self.all_of_size(size), cap=cap)
        return self.run(name, self.sample(size, cap, name), cap=cap)

    def stage_balanced(self, name, sizes, share):
        """Seeded samples of every size, more for small sizes (where most of the mass is)."""
        sizes = [s for s in sizes if s >= 2]
        if not sizes:
            return []
        weights = {s: 1.0 if s <= 8 else 0.5 if s <= 16 else 0.125 if s <= 64 else 0.02 for s in sizes}
        total = sum(weights.values())
        cap = int(self.remaining() * share)
        trials = []
        for s in sizes:
            trials += self.sample(s, max(1, int(cap * weights[s] / total)), name)
        return self.run(name, trials, cap=cap)

    def stage_adaptive(self, name, rounds, share, per_size=None, extra_seeds=()):
        """Local search: neighbours of the leaders and of the best of each size."""
        tested = []
        cap_total = int(self.remaining() * share)
        for r in range(1, rounds + 1):
            cap = max(1, cap_total // rounds)
            sizes = self.best_by_size()
            marks = [s for s in sizes if s <= 16 or s in (24, 32, 48, 64, 96, 128, 256) or s == max(sizes)]
            seeds = {t for t, _ in self.leaders(12)} | {sizes[s][0] for s in marks} | {tuple(sorted(t)) for t in extra_seeds}
            proposals = {}
            for seed in sorted(seeds):
                for trial in self.neighbors(seed):
                    trial = tuple(sorted(trial))
                    if trial not in self.seen:
                        proposals.setdefault(len(trial), set()).add(trial)
            per = per_size or max(1, cap // max(1, len(proposals)))
            chosen = []
            for size in sorted(proposals):
                chosen += ordered(proposals[size], f"{self.seed}|{name}|{r}")[:per]
            tested += self.run(f"{name} {r}", chosen, cap=cap)
            if not self.remaining():
                break
        return tested
