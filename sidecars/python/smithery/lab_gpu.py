"""Optional GPU acceleration of the bulk evaluator (smithery.lab_fast), bit-identical to it.

Used only when PyTorch with CUDA is importable; otherwise callers keep the CPU
FastSystem. Member vote sums, held verdicts, pair scores and the rotation pick run
on the GPU; the engine's equity product runs on the CPU as one sequential
multiply.accumulate over the same factors the bar loop multiplies, so curves equal
FastSystem.simulate bit for bit (tests compare them). Rotation systems with USD in
the universe only; anything else falls back to the CPU.
"""
from __future__ import annotations

from pathlib import Path

import numpy as np

from .lab_fast import NONE, FastSystem


def available() -> bool:
    try:
        import torch
        return bool(torch.cuda.is_available())
    except Exception:  # noqa: BLE001 — no torch, no CUDA, a broken driver: the CPU path
        return False


def supports(system: FastSystem) -> bool:
    return not system.single and bool(system.config.include_usd)


class GpuSystem:
    """A built FastSystem's cube on the GPU: ``simulate`` and ``evaluate`` like FastSystem."""

    def __init__(self, folder, budget_bytes: float = 6.0e9):
        import torch
        self.torch = torch
        self.dev = torch.device("cuda")
        self.cpu = FastSystem.load(folder)
        s = self.cpu
        if not supports(s):
            raise ValueError("GPU evaluation supports rotation systems with USD only")
        a = s.rotation
        self.T, self.P, self.N = len(s.dates), len(s.pairs), len(s.assets)
        self.cash = self.N - 1
        self.dates, self.failed = s.dates, s.failed
        self.budget_bytes = budget_bytes
        oa = torch.zeros(self.P, self.N, dtype=torch.float16)
        ob = torch.zeros(self.P, self.N, dtype=torch.float16)
        ends = torch.as_tensor(a["ends"])
        oa[torch.arange(self.P), ends[:, 0]] = 1
        ob[torch.arange(self.P), ends[:, 1]] = 1
        self.onehot_a, self.onehot_b = oa.to(self.dev), ob.to(self.dev)
        self.active = torch.as_tensor(a["active"]).to(self.dev)
        self.order = torch.as_tensor(a["order"].astype(np.int64)).to(self.dev)
        self.in_today = torch.as_tensor(a["in_today"]).to(self.dev)
        self.has_syms = torch.as_tensor(a["has_syms"]).to(self.dev)
        self.returns = torch.as_tensor(np.ascontiguousarray(a["returns"])).to(self.dev)
        self.groups = []
        hi = int(s.mpos.max()) + 1
        for group in s.groups:
            votes = np.load(Path(folder) / group.file, mmap_mode="r")
            first = int(group.first)
            cut = np.array(votes[:, :, first:hi])
            self.groups.append({
                "votes": torch.from_numpy(cut.reshape(cut.shape[0], -1)).to(self.dev).to(torch.float16),
                "tg": cut.shape[2], "mpos": torch.as_tensor((s.mpos - first).astype(np.int64)).to(self.dev),
                "gate": None if group.gate is None else torch.as_tensor(np.asarray(group.gate, dtype=np.int64)).to(self.dev),
                "index": {k: i for i, k in enumerate(group.keys)}, "start": group.start})
        self.by_start = {g["start"]: i for i, g in enumerate(self.groups)}

    def _group(self, members):
        return self.by_start[min(self.cpu.key_start[k] for k in members)]

    def _best(self, g, trials):
        torch, gr = self.torch, self.groups[g]
        K, tg, P, T = gr["votes"].shape[0], gr["tg"], self.P, self.T
        out = torch.empty(len(trials), T, dtype=torch.int64, device=self.dev)
        per = P * tg * 16 + P * T * 6 + T * self.N * 8
        step = max(16, min(1024, int(self.budget_bytes // per)))
        positions = torch.arange(tg, device=self.dev, dtype=torch.int16) * 4
        minus = torch.tensor(-1, dtype=torch.int16, device=self.dev)
        for lo in range(0, len(trials), step):
            part = trials[lo:lo + step]
            M = torch.zeros(len(part), K, dtype=torch.float16, device=self.dev)
            rows = [(i, gr["index"][k]) for i, t in enumerate(part) for k in t]
            M[torch.tensor([r[0] for r in rows], device=self.dev), torch.tensor([r[1] for r in rows], device=self.dev)] = 1
            total = (M @ gr["votes"]).view(len(part), P, tg)
            singles = [len(t) == 1 for t in part]
            if all(singles):
                bull_state = total[:, :, gr["mpos"]] > 0
            else:
                if any(singles):
                    raise ValueError("singles and aggregates in one batch")
                enc = torch.where(total != 0, positions + (torch.sign(total).to(torch.int16) + 1), minus)
                del total
                last = torch.cummax(enc, dim=2).values[:, :, gr["mpos"]]
                del enc
                bull_state = (last >= 0) & ((last & 3) == 2)
            bull = bull_state & self.active
            bear = (~bull_state) & self.active
            scores = torch.matmul(bull.transpose(1, 2).to(torch.float16), self.onehot_a) + \
                torch.matmul(bear.transpose(1, 2).to(torch.float16), self.onehot_b)
            idx = self.order.clamp(min=0).unsqueeze(0).expand(len(part), -1, -1)
            cand = torch.where(self.order.unsqueeze(0) >= 0, torch.gather(scores.float(), 2, idx),
                               torch.tensor(-1.0, device=self.dev))
            pick = cand.argmax(dim=2)
            out[lo:lo + len(part)] = torch.gather(self.order.unsqueeze(0).expand(len(part), -1, -1), 2, pick.unsqueeze(2)).squeeze(2)
        return out

    def _factors(self, best, gate, delay=0):
        torch = self.torch
        B, T = best.shape
        cash = self.cash
        cost = (1.0 - self.cpu.config.fee_rate) * (1.0 - self.cpu.config.slippage_rate)
        factor = torch.tensor(np.array([1.0, cost ** 1, cost ** 2]), device=self.dev)
        gate_off = torch.zeros(1, T, dtype=torch.bool, device=self.dev) if gate is None else (gate != 1).unsqueeze(0)
        cash_t = torch.tensor(cash, device=self.dev)
        pend = torch.where(gate_off, cash_t, best)
        pend = torch.where(self.has_syms.unsqueeze(0), pend, cash_t)
        shift = 1 + delay
        p = torch.full((B, T), NONE, dtype=torch.int64, device=self.dev)
        p[:, shift:] = pend[:, :T - shift]
        forced = (p >= 0) & (p != cash) & ~self.in_today.T.gather(0, p.clamp(min=0))
        p = torch.where(forced, cash_t, p)
        take = p != NONE
        prev = torch.cat([torch.full((B, 1), NONE, dtype=torch.int64, device=self.dev), p[:, :-1]], dim=1)
        prev_cash = (prev == NONE) | (prev == cash)
        new_cash = (p == NONE) | (p == cash)
        legs = torch.where(prev_cash & new_cash, 0, torch.where(prev_cash | new_cash, 1, torch.where(prev == p, 0, 2)))
        legs = torch.where(take, legs, 0)
        owned = (p >= 0) & (p != cash)
        r = self.returns[p.clamp(min=0), torch.arange(T, device=self.dev).expand(B, -1)]
        g = 1.0 + torch.where(owned, r, torch.zeros((), dtype=torch.float64, device=self.dev))
        return factor[legs], g

    def simulate(self, trials, delay=0):
        """Equity curves (trials × bars), like FastSystem.simulate (bit-identical at delay 0)."""
        trials = [tuple(t) for t in trials]
        by_group = {}
        for i, t in enumerate(trials):
            by_group.setdefault(self._group(t), []).append(i)
        curve = np.empty((len(trials), self.T))
        for g, ids in by_group.items():
            for lo in range(0, len(ids), 4096):
                part = ids[lo:lo + 4096]
                best = self._best(g, [trials[i] for i in part])
                f, gfac = self._factors(best, self.groups[g]["gate"], delay)
                x = self.torch.stack([f, gfac], dim=2).reshape(len(part), -1).cpu().numpy()
                curve[part] = np.multiply.accumulate(x, axis=1)[:, 1::2]
        return curve
