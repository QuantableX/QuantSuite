"""Batched indicators: S series on one time grid, computed in one call, on the GPU when CUDA is there.

An indicator written for the batch implements ``_compute_batch(self, b)`` against a ``Batch``:
``b.open``, ``b.high``, ``b.low``, ``b.close`` and ``b.volume`` are ``Col`` objects, each a
``[series x bars]`` array, with the pandas operations indicators use (``rolling``, ``ewm``,
``shift``, ``diff``, ``where``, ``clip``, ``ffill``, ``cumsum`` ...) and numpy's functions
(``np.log(col)``, ``np.where(cond, a, b)``). ``b.k`` has what pandas spells differently on a
frame (``max_of`` for ``concat(...).max(axis=1)``, ``hysteresis``, ``warmup``, ``windows``).

One code path for both devices:

- CPU (numpy): rolling and ewm run through pandas itself, column by column, so a batch
  gives bit for bit what the script's pandas code gave on one series.
- GPU (torch on CUDA): the same operations as GPU kernels, in float64. Sums and
  transcendental functions can differ from pandas in the last bits, so a vote can differ
  where a score sits exactly on its threshold; the parity check reports every such bar.

``SMITHERY_GPU=0`` keeps everything on the CPU. Every caller records ``device_note()``.
"""
from __future__ import annotations

import math
import os

import numpy as np
import pandas as pd

OHLCV = ("open", "high", "low", "close", "volume")

# --------------------------------------------------------------------------- device

_TORCH: dict = {}


def _torch():
    """(torch, device) when the GPU path may run, else (None, reason)."""
    if "state" not in _TORCH:
        if os.environ.get("SMITHERY_GPU", "1").strip() == "0":
            _TORCH["state"] = (None, "CPU: SMITHERY_GPU=0")
        else:
            try:
                import torch
            except Exception as exc:  # noqa: BLE001 — torch is optional
                _TORCH["state"] = (None, f"CPU: PyTorch not installed ({type(exc).__name__})")
            else:
                if not torch.cuda.is_available():
                    _TORCH["state"] = (None, "CPU: no CUDA device")
                else:
                    _TORCH["state"] = (torch, torch.device("cuda"))
    return _TORCH["state"]


def gpu_available() -> bool:
    return _torch()[0] is not None


def device_note() -> str:
    """What batches run on, for progress lines and result files: ``GPU (<name>)`` or ``CPU: <reason>``."""
    torch, device = _torch()
    if torch is None:
        return device
    return f"GPU ({torch.cuda.get_device_name(device)})"


def device_for(indicator, device: str = "auto") -> tuple[str, str]:
    """Where ``signals`` computes this indicator: ``("gpu", "")`` or ``("cpu", reason)``."""
    if not type(indicator).has_batch():
        return "cpu", "no batch form (one series at a time)"
    if device == "cpu":
        return "cpu", "CPU requested"
    if device == "auto" and getattr(indicator, "gpu", True) is False:
        return "cpu", "compiled per-row kernels (gpu = False)"
    torch, reason = _torch()
    if torch is None:
        return "cpu", reason.removeprefix("CPU: ")
    return "gpu", ""


def device_label(indicator, device: str = "auto") -> str:
    """``GPU (<name>)`` or ``CPU: <reason>`` for one indicator, for a job's progress line."""
    where, reason = device_for(indicator, device)
    return device_note() if where == "gpu" else f"CPU: {reason}"


# Series computed in this process, for the jobs' progress lines: on the GPU, and on the CPU by reason.
_TALLY: dict = {"gpu": 0, "cpu": {}}


def _count(where: str, n: int, reason: str = "") -> None:
    if n <= 0:
        return
    if where == "gpu":
        _TALLY["gpu"] += n
    else:
        _TALLY["cpu"][reason] = _TALLY["cpu"].get(reason, 0) + n


def tally() -> dict:
    """A copy of this process's counts (``signal()`` calls included), to pass to ``device_line`` later."""
    from .contract import single_series_calls
    cpu = dict(_TALLY["cpu"])
    single = single_series_calls()
    if single:
        cpu["one series per call"] = cpu.get("one series per call", 0) + single
    return {"gpu": _TALLY["gpu"], "cpu": cpu}


def device_line(since: dict | None = None) -> str:
    """Where the indicator series since ``since`` (a ``tally()``) ran, e.g.
    ``GPU (NVIDIA GeForce RTX 5090, 16,610 series); CPU: compiled per-row kernels (gpu = False) 3,020``."""
    now, before = tally(), since or {"gpu": 0, "cpu": {}}
    gpu = now["gpu"] - before["gpu"]
    cpu = {r: n - before["cpu"].get(r, 0) for r, n in now["cpu"].items()}
    cpu = sorted(((r, n) for r, n in cpu.items() if n > 0), key=lambda item: -item[1])
    parts = []
    if gpu > 0:
        parts.append(f"{device_note()[:-1]}, {gpu:,} series)")
    if cpu:
        parts.append("CPU: " + ", ".join(f"{r} {n:,}" for r, n in cpu))
    return "; ".join(parts) or "no indicator series computed"


def ops_for(device: str = "auto"):
    """The operations of one device: ``"cpu"``, ``"gpu"`` or ``"auto"`` (GPU when available)."""
    if device == "cpu":
        return NUMPY
    torch, dev = _torch()
    if torch is None:
        if device == "gpu":
            raise RuntimeError(f"GPU requested but unavailable ({dev})")
        return NUMPY
    if "ops" not in _TORCH:
        _TORCH["ops"] = _TorchOps(torch, dev)
    return _TORCH["ops"]


# --------------------------------------------------------------------------- numpy (pandas-exact)

class _NumpyOps:
    name = "cpu"

    def asarray(self, a):
        return np.asarray(a, dtype=float)

    def to_numpy(self, a):
        return np.asarray(a)

    def is_array(self, a) -> bool:
        return isinstance(a, np.ndarray)

    def nan_like(self, a):
        return np.full(a.shape, np.nan)

    def full(self, shape, value):
        return np.full(shape, value, dtype=float)

    def arange(self, n):
        return np.arange(n)

    def where(self, cond, a, b):
        return np.where(cond, a, b)

    def isnan(self, a):
        return np.isnan(a)

    def isfinite(self, a):
        return np.isfinite(a)

    def astype_float(self, a):
        return np.asarray(a, dtype=float)

    def copy(self, a):
        return np.array(a, dtype=a.dtype, copy=True)

    # pandas does the windows: a DataFrame's columns are the series
    @staticmethod
    def _frame(a):
        return pd.DataFrame(np.asarray(a, dtype=float).T)

    def rolling(self, a, window, min_periods, how, *args, **kwargs):
        roll = self._frame(a).rolling(window, min_periods=min_periods)
        return np.ascontiguousarray(getattr(roll, how)(*args, **kwargs).to_numpy().T)

    def rolling_pair(self, a, b, window, min_periods, how):
        out = getattr(self._frame(a).rolling(window, min_periods=min_periods), how)(self._frame(b))
        return np.ascontiguousarray(out.to_numpy().T)

    def ewm(self, a, how, spec, *args, **kwargs):
        return np.ascontiguousarray(getattr(self._frame(a).ewm(**spec), how)(*args, **kwargs).to_numpy().T)

    def shift(self, a, k):
        a = np.asarray(a)
        if a.dtype == bool:
            a = a.astype(float)
        out = np.full(a.shape, np.nan)
        if k > 0:
            out[:, k:] = a[:, :-k] if k < a.shape[1] else out[:, k:]
        elif k < 0:
            out[:, :k] = a[:, -k:] if -k < a.shape[1] else out[:, :k]
        else:
            out[:] = a
        return out

    def cum(self, a, how):
        return np.ascontiguousarray(getattr(self._frame(a), how)().to_numpy().T)

    def ffill(self, a, limit=None):
        return np.ascontiguousarray(self._frame(a).ffill(limit=limit).to_numpy().T)

    def clip(self, a, lower, upper):
        out = np.asarray(a, dtype=float)
        if lower is not None:
            out = np.where(np.isnan(out), out, np.where(out < lower, lower, out))
        if upper is not None:
            out = np.where(np.isnan(out), out, np.where(out > upper, upper, out))
        return out

    def max_of(self, arrays, skipna):
        frame = np.stack([np.broadcast_to(np.asarray(x, dtype=float), arrays[0].shape) for x in arrays])
        if not skipna:
            return frame.max(axis=0)
        with np.errstate(all="ignore"):
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", RuntimeWarning)
                return np.nanmax(frame, axis=0)

    def min_of(self, arrays, skipna):
        frame = np.stack([np.broadcast_to(np.asarray(x, dtype=float), arrays[0].shape) for x in arrays])
        if not skipna:
            return frame.min(axis=0)
        import warnings
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)
            return np.nanmin(frame, axis=0)

    def windows(self, a, n):
        """[S, T-n+1, n] read-only view of every complete window."""
        return np.lib.stride_tricks.sliding_window_view(np.asarray(a), n, axis=1)

    def last_index(self, mask):
        """Per bar, the latest bar <= t where mask holds, -1 before any."""
        T = mask.shape[1]
        idx = np.where(mask, np.arange(T), -1)
        return np.maximum.accumulate(idx, axis=1)

    def take_along(self, a, idx):
        return np.take_along_axis(a, idx, axis=1)

    def ufunc(self, ufunc, inputs, kwargs):
        return ufunc(*inputs, **kwargs)

    def function(self, func, args, kwargs):
        return func(*args, **kwargs)

    def stack(self, arrays, axis=0):
        return np.stack(arrays, axis=axis)


NUMPY = _NumpyOps()


# --------------------------------------------------------------------------- torch (CUDA)

_UFUNC_NAMES = {
    "add": "add", "subtract": "sub", "multiply": "mul", "true_divide": "div", "divide": "div",
    "power": "pow", "negative": "neg", "positive": "positive", "absolute": "abs", "fabs": "abs",
    "sign": "sign", "sqrt": "sqrt", "square": "square", "exp": "exp", "exp2": "exp2", "expm1": "expm1",
    "log": "log", "log2": "log2", "log10": "log10", "log1p": "log1p", "sin": "sin", "cos": "cos",
    "tan": "tan", "arcsin": "asin", "arccos": "acos", "arctan": "atan", "arctan2": "atan2",
    "sinh": "sinh", "cosh": "cosh", "tanh": "tanh", "arcsinh": "asinh", "arccosh": "acosh",
    "arctanh": "atanh", "floor": "floor", "ceil": "ceil", "trunc": "trunc", "rint": "round",
    "maximum": "maximum", "minimum": "minimum", "fmax": "fmax", "fmin": "fmin", "hypot": "hypot",
    "isnan": "isnan", "isfinite": "isfinite", "isinf": "isinf", "greater": "gt", "greater_equal": "ge",
    "less": "lt", "less_equal": "le", "equal": "eq", "not_equal": "ne", "logical_and": "logical_and",
    "logical_or": "logical_or", "logical_not": "logical_not", "logical_xor": "logical_xor",
    "copysign": "copysign", "floor_divide": "floor_divide", "remainder": "remainder", "mod": "remainder",
    "reciprocal": "reciprocal", "signbit": "signbit", "deg2rad": "deg2rad", "rad2deg": "rad2deg",
    "cbrt": None, "heaviside": "heaviside",
}


class _TorchOps:
    name = "gpu"

    def __init__(self, torch, device):
        self.torch = torch
        self.device = device
        self.f64 = torch.float64
        # numpy promotes int arrays with a float to float64; torch would pick float32.
        torch.set_default_dtype(torch.float64)

    # conversions
    def asarray(self, a):
        t = self.torch
        if isinstance(a, t.Tensor):
            return a.to(device=self.device, dtype=self.f64)
        return t.as_tensor(np.array(a, dtype=float, copy=True), dtype=self.f64, device=self.device)

    def to_numpy(self, a):
        if isinstance(a, self.torch.Tensor):
            return a.detach().cpu().numpy()
        return np.asarray(a)

    def is_array(self, a) -> bool:
        return isinstance(a, self.torch.Tensor)

    def _t(self, x, like=None):
        """A tensor for x (scalars become 0-d float64 tensors on the device)."""
        t = self.torch
        if isinstance(x, t.Tensor):
            return x
        if isinstance(x, np.ndarray):
            return t.as_tensor(x, device=self.device, dtype=self.f64 if x.dtype != bool else t.bool)
        if isinstance(x, (bool, np.bool_)):
            return t.tensor(bool(x), device=self.device)
        return t.tensor(float(x), dtype=self.f64, device=self.device)

    def nan_like(self, a):
        return self.torch.full(a.shape, float("nan"), dtype=self.f64, device=self.device)

    def full(self, shape, value):
        return self.torch.full(shape, float(value), dtype=self.f64, device=self.device)

    def arange(self, n):
        return self.torch.arange(n, device=self.device)

    def where(self, cond, a, b):
        t = self.torch
        cond = self._t(cond)
        if cond.dtype != t.bool:
            cond = cond != 0
        a, b = self._t(a), self._t(b)
        if a.dtype == t.bool:
            a = a.to(self.f64)
        if b.dtype == t.bool:
            b = b.to(self.f64)
        return t.where(cond, a, b)

    def isnan(self, a):
        return self.torch.isnan(self._t(a))

    def isfinite(self, a):
        return self.torch.isfinite(self._t(a))

    def astype_float(self, a):
        return self._t(a).to(self.f64)

    def copy(self, a):
        return a.clone()

    # ---- windows
    def _counts(self, valid, n):
        """Per bar, how many of the last n bars are valid (exact: integer counts in float64)."""
        t = self.torch
        c = t.cumsum(valid.to(self.f64), dim=1)
        out = c.clone()
        if n < c.shape[1]:
            out[:, n:] = c[:, n:] - c[:, :-n]
        return out

    def _window_sum(self, x, n):
        """Window sums of finite-filled x (zeros for missing), centred per series for precision."""
        t = self.torch
        valid = ~t.isnan(x)
        cnt_all = valid.sum(dim=1, keepdim=True).clamp(min=1).to(self.f64)
        off = t.where(valid, x, t.zeros_like(x)).sum(dim=1, keepdim=True) / cnt_all
        z = t.where(valid, x - off, t.zeros_like(x))
        c = t.cumsum(z, dim=1)
        s = c.clone()
        if n < c.shape[1]:
            s[:, n:] = c[:, n:] - c[:, :-n]
        cnt = self._counts(valid, n)
        return s + off * cnt, cnt, valid

    def _min_periods(self, n, min_periods):
        return n if min_periods is None else int(min_periods)

    def _signs(self, x, n):
        neg = self._counts(x < 0, n)
        return neg

    def rolling(self, a, window, min_periods, how, *args, **kwargs):
        t = self.torch
        x = self._t(a).to(self.f64)
        n = int(window)
        mp = self._min_periods(n, min_periods)
        if how in ("sum", "mean"):
            s, cnt, valid = self._window_sum(x, n)
            # pandas: a window whose valid values are all one value gives that value exactly
            hi, lo = self._extreme(x, n, True, 1), self._extreme(x, n, False, 1)
            same = (hi == lo) & (cnt > 0)
            last = self.ffill(x)
            if how == "mean":
                res = s / cnt
                neg = self._signs(x, n)
                res = t.where((neg == 0) & (res < 0), t.zeros_like(res), res)
                res = t.where((neg == cnt) & (res > 0), t.zeros_like(res), res)
                res = t.where(same, last, res)
                ok = (cnt >= mp) & (cnt > 0)
            else:
                res = t.where(same, last * cnt, s)
                ok = cnt >= mp
                res = t.where((cnt == 0) & ok, t.zeros_like(res), res)
            return t.where(ok, res, t.full_like(res, float("nan")))
        if how == "count":
            cnt = self._counts(~t.isnan(x), n)
            return t.where(cnt >= mp, cnt, t.full_like(cnt, float("nan")))
        if how in ("var", "std"):
            ddof = int(kwargs.get("ddof", args[0] if args else 1))
            valid = ~t.isnan(x)
            cnt = self._counts(valid, n)
            if n <= 256:
                # Two passes per window (mean, then squared deviations): no cancellation.
                S_ = x.shape[0]
                padded = t.cat([t.full((S_, n - 1), float("nan"), dtype=self.f64, device=self.device), x], dim=1)
                win = padded.unfold(1, n, 1)                              # [S, T, n]
                ok_w = ~t.isnan(win)
                zero = t.zeros((), dtype=self.f64, device=self.device)
                cw = ok_w.sum(dim=2, keepdim=True).clamp(min=1).to(self.f64)
                mean_w = t.where(ok_w, win, zero).sum(dim=2, keepdim=True) / cw
                ssq = t.where(ok_w, (win - mean_w).square(), zero).sum(dim=2)
            else:
                cnt_all = valid.sum(dim=1, keepdim=True).clamp(min=1).to(self.f64)
                off = t.where(valid, x, t.zeros_like(x)).sum(dim=1, keepdim=True) / cnt_all
                z = t.where(valid, x - off, t.zeros_like(x))
                c1, c2 = t.cumsum(z, dim=1), t.cumsum(z * z, dim=1)
                s1, s2 = c1.clone(), c2.clone()
                if n < x.shape[1]:
                    s1[:, n:] = c1[:, n:] - c1[:, :-n]
                    s2[:, n:] = c2[:, n:] - c2[:, :-n]
                ssq = s2 - s1 * s1 / cnt.clamp(min=1)
            var = ssq / (cnt - ddof)
            var = t.where(var < 0, t.zeros_like(var), var)
            # pandas: a window of one repeated value has a variance of exactly 0
            hi, lo = self._extreme(x, n, True, 1), self._extreme(x, n, False, 1)
            var = t.where(hi == lo, t.zeros_like(var), var)
            var = t.where(cnt == 1, t.zeros_like(var), var)
            ok = (cnt >= max(mp, 1)) & (cnt > ddof)
            var = t.where(ok, var, t.full_like(var, float("nan")))
            return t.sqrt(var) if how == "std" else var
        if how in ("max", "min"):
            res = self._extreme(x, n, how == "max", mp)
            return res
        if how in ("median", "quantile") and n <= 512 and kwargs.get("interpolation", "linear") == "linear":
            return self._order_stat(x, n, mp, how, float(args[0]) if args else None)
        # rarer statistics: exact through pandas on the CPU, then back (see device_note in reports)
        return self.asarray(NUMPY.rolling(self.to_numpy(x), window, min_periods, how, *args, **kwargs))

    def _order_stat(self, x, n, mp, how, q):
        """pandas' rolling median / linear quantile on sorted windows (NaN sorts last):
        the same picks and the same interpolation arithmetic as pandas' skiplist."""
        t = self.torch
        S, T = x.shape
        padded = t.cat([t.full((S, n - 1), float("nan"), dtype=self.f64, device=self.device), x], dim=1)
        out = t.full((S, T), float("nan"), dtype=self.f64, device=self.device)
        step = max(1, int(os.environ.get("SMITHERY_GPU_CELLS", DEFAULT_GPU_CELLS)) // max(n * S, 1))
        for start in range(0, T, step):
            stop = min(T, start + step)
            win = padded[:, start:stop + n - 1].unfold(1, n, 1)              # [S, w, n]
            srt = t.sort(win, dim=2).values
            m = (~t.isnan(win)).sum(dim=2)                                    # valid values per window
            mm = m.clamp(min=1)
            if how == "median":
                mid = mm // 2
                upper = t.take_along_dim(srt, mid[..., None], dim=2)[..., 0]
                lower = t.take_along_dim(srt, (mid - 1).clamp(min=0)[..., None], dim=2)[..., 0]
                res = t.where(mm % 2 == 1, upper, (upper + lower) / 2)
            else:
                pos = q * (mm - 1).to(self.f64)
                lo = t.floor(pos).to(t.long)
                vlow = t.take_along_dim(srt, lo[..., None], dim=2)[..., 0]
                vhigh = t.take_along_dim(srt, (lo + 1).clamp(max=n - 1)[..., None], dim=2)[..., 0]
                frac = pos - lo.to(self.f64)
                res = t.where(frac == 0, vlow, vlow + (vhigh - vlow) * frac)
            ok = (m >= max(mp, 1))
            out[:, start:stop] = t.where(ok, res, t.full_like(res, float("nan")))
        return out

    def _extreme(self, x, n, is_max, mp):
        """Rolling max/min (van Herk / Gil-Werman: block prefix and suffix extremes), NaN-aware like pandas."""
        t = self.torch
        S, T = x.shape
        valid = ~t.isnan(x)
        fill = float("-inf") if is_max else float("inf")
        y = t.where(valid, x, t.full_like(x, fill))
        blocks = -(-T // n)
        pad = blocks * n - T
        yp = t.cat([y, t.full((S, pad), fill, dtype=self.f64, device=self.device)], dim=1) if pad else y
        v = yp.reshape(S, blocks, n)
        if is_max:
            prefix = t.cummax(v, dim=2).values.reshape(S, -1)[:, :T]
            suffix = t.flip(t.cummax(t.flip(v, [2]), dim=2).values, [2]).reshape(S, -1)[:, :T]
            pick = t.maximum
        else:
            prefix = t.cummin(v, dim=2).values.reshape(S, -1)[:, :T]
            suffix = t.flip(t.cummin(t.flip(v, [2]), dim=2).values, [2]).reshape(S, -1)[:, :T]
            pick = t.minimum
        res = prefix.clone()
        if n > 1 and T >= n:
            res[:, n - 1:] = pick(suffix[:, :T - n + 1], prefix[:, n - 1:])
        # before a full window the window is the prefix of the series
        if n > 1:
            head = (t.cummax(y, dim=1).values if is_max else t.cummin(y, dim=1).values)[:, :min(n - 1, T)]
            res[:, :min(n - 1, T)] = head
        cnt = self._counts(valid, n)
        res = t.where(cnt >= max(mp, 1), res, t.full_like(res, float("nan")))
        return res

    def rolling_pair(self, a, b, window, min_periods, how):
        out = NUMPY.rolling_pair(self.to_numpy(a), self.to_numpy(b), window, min_periods, how)
        return self.asarray(out)

    def ewm(self, a, how, spec, *args, **kwargs):
        if how == "mean" and not spec.get("times"):
            return self._ewm_mean(self._t(a).to(self.f64), spec)
        return self.asarray(NUMPY.ewm(self.to_numpy(a), how, spec, *args, **kwargs))

    def _ewm_mean(self, x, spec):
        """pandas' ewma: as linear scans when the data allow (no bar loop), else pandas'
        recursion bar by bar over all series at once."""
        t = self.torch
        alpha = _ewm_alpha(spec)
        adjust = bool(spec.get("adjust", True))
        ignore_na = bool(spec.get("ignore_na", False))
        minp = max(int(spec.get("min_periods", 0) or 0), 1)
        S, T = x.shape
        if T:
            obs = ~t.isnan(x)
            idx = t.arange(T, device=x.device)
            has = obs.any(dim=1)
            first = t.where(has, obs.to(t.int8).argmax(dim=1), t.full((S,), T, device=x.device, dtype=t.long))
            started = idx[None, :] >= first[:, None]
            holes = (started & ~obs).any()
            nobs = t.cumsum(obs.to(self.f64), dim=1)
            beta = 1.0 - alpha
            xz = t.where(obs, x, t.zeros_like(x))
            if not adjust and not bool(holes):
                at = idx[None, :] == first[:, None]
                u = t.where(at, xz, alpha * xz)
                y = linear_scan(self, beta, t.where(started, u, t.zeros_like(u)))
            elif adjust and (not ignore_na or not bool(holes)):
                num = linear_scan(self, beta, xz)
                den = linear_scan(self, beta, obs.to(self.f64))
                y = num / den
            else:
                y = None
            if y is not None:
                nan = t.full_like(y, float("nan"))
                y = t.where(started, y, nan)
                return t.where(nobs >= minp, y, nan)
        out = t.empty_like(x)
        if T == 0:
            return out
        beta = 1.0 - alpha
        new_wt = 1.0 if adjust else alpha
        weighted = x[:, 0].clone()
        obs0 = ~t.isnan(weighted)
        nobs = obs0.to(self.f64)
        old_wt = t.ones(S, dtype=self.f64, device=self.device)
        nan = t.full((S,), float("nan"), dtype=self.f64, device=self.device)
        out[:, 0] = t.where(nobs >= minp, weighted, nan)
        for i in range(1, T):
            cur = x[:, i]
            obs = ~t.isnan(cur)
            nobs = nobs + obs.to(self.f64)
            have = ~t.isnan(weighted)
            step = have & (obs | (not ignore_na))
            old_wt = t.where(step, old_wt * beta, old_wt)
            upd = have & obs
            changed = upd & (weighted != cur)
            blended = (old_wt * weighted + new_wt * cur) / (old_wt + new_wt)
            weighted = t.where(changed, blended, weighted)
            if adjust:
                old_wt = t.where(upd, old_wt + new_wt, old_wt)
            else:
                old_wt = t.where(upd, t.ones_like(old_wt), old_wt)
            weighted = t.where(~have & obs, cur, weighted)
            out[:, i] = t.where(nobs >= minp, weighted, nan)
        return out

    def shift(self, a, k):
        t = self.torch
        x = self._t(a)
        if x.dtype == t.bool:
            x = x.to(self.f64)
        out = t.full(x.shape, float("nan"), dtype=self.f64, device=self.device)
        T = x.shape[1]
        if k > 0:
            if k < T:
                out[:, k:] = x[:, :-k]
        elif k < 0:
            if -k < T:
                out[:, :k] = x[:, -k:]
        else:
            out[:] = x
        return out

    def cum(self, a, how):
        t = self.torch
        x = self._t(a).to(self.f64)
        valid = ~t.isnan(x)
        if how == "cumsum":
            r = t.cumsum(t.where(valid, x, t.zeros_like(x)), dim=1)
        elif how == "cumprod":
            r = t.cumprod(t.where(valid, x, t.ones_like(x)), dim=1)
        elif how == "cummax":
            r = t.cummax(t.where(valid, x, t.full_like(x, float("-inf"))), dim=1).values
        elif how == "cummin":
            r = t.cummin(t.where(valid, x, t.full_like(x, float("inf"))), dim=1).values
        else:
            raise ValueError(how)
        return t.where(valid, r, t.full_like(r, float("nan")))

    def ffill(self, a, limit=None):
        t = self.torch
        x = self._t(a).to(self.f64)
        if limit is not None:
            return self.asarray(NUMPY.ffill(self.to_numpy(x), limit))
        last = self.last_index(~t.isnan(x))
        got = t.take_along_dim(x, last.clamp(min=0), dim=1)
        return t.where(last >= 0, got, t.full_like(got, float("nan")))

    def clip(self, a, lower, upper):
        t = self.torch
        x = self._t(a).to(self.f64)
        out = x
        if lower is not None:
            lo = self._t(lower)
            out = t.where(t.isnan(out), out, t.where(out < lo, lo.to(self.f64) if lo.dtype != self.f64 else lo, out))
        if upper is not None:
            hi = self._t(upper)
            out = t.where(t.isnan(out), out, t.where(out > hi, hi.to(self.f64) if hi.dtype != self.f64 else hi, out))
        return out

    def max_of(self, arrays, skipna):
        t = self.torch
        xs = [self._t(x).to(self.f64) for x in arrays]
        shape = t.broadcast_shapes(*[x.shape for x in xs])
        stack = t.stack([x.expand(shape) for x in xs])
        if not skipna:
            return stack.max(dim=0).values if not t.isnan(stack).any() else t.where(
                t.isnan(stack).any(dim=0), t.full(shape, float("nan"), dtype=self.f64, device=self.device),
                stack.max(dim=0).values)
        allnan = t.isnan(stack).all(dim=0)
        m = t.where(t.isnan(stack), t.full_like(stack, float("-inf")), stack).max(dim=0).values
        return t.where(allnan, t.full_like(m, float("nan")), m)

    def min_of(self, arrays, skipna):
        neg = self.max_of([-self._t(x).to(self.f64) for x in arrays], skipna)
        return -neg

    def windows(self, a, n):
        return self._t(a).unfold(1, n, 1)

    def last_index(self, mask):
        t = self.torch
        T = mask.shape[1]
        idx = t.where(mask, t.arange(T, device=self.device).expand(mask.shape), t.full(mask.shape, -1, device=self.device, dtype=t.long))
        return t.cummax(idx, dim=1).values

    def take_along(self, a, idx):
        return self.torch.take_along_dim(a, idx, dim=1)

    def ufunc(self, ufunc, inputs, kwargs):
        if kwargs:
            raise TypeError(f"np.{ufunc.__name__} with {sorted(kwargs)} is not supported in a GPU batch")
        name = _UFUNC_NAMES.get(ufunc.__name__)
        if name is None:
            raise TypeError(f"np.{ufunc.__name__} is not supported in a GPU batch")
        t = self.torch
        args = [self._t(x) for x in inputs]
        if name in ("isnan", "isfinite", "isinf", "logical_not", "signbit"):
            return getattr(t, name)(args[0])
        args = [a.to(self.f64) if a.dtype not in (t.bool,) or name not in ("logical_and", "logical_or", "logical_xor") else a
                for a in args]
        return getattr(t, name)(*args)

    def function(self, func, args, kwargs):
        raise TypeError(f"np.{func.__name__} is not supported in a GPU batch; use b.k")

    def stack(self, arrays, axis=0):
        return self.torch.stack([self._t(a) for a in arrays], dim=axis)


def _ewm_alpha(spec: dict) -> float:
    """pandas' get_center_of_mass, as alpha."""
    com, span, halflife, alpha = (spec.get(k) for k in ("com", "span", "halflife", "alpha"))
    if com is not None:
        return 1.0 / (1.0 + float(com))
    if span is not None:
        return 1.0 / (1.0 + (float(span) - 1) / 2.0)
    if halflife is not None:
        com_ = 1.0 / (1.0 - math.exp(math.log(0.5) / float(halflife))) - 1.0
        return 1.0 / (1.0 + com_)
    if alpha is not None:
        return float(alpha)
    raise ValueError("ewm needs one of com, span, halflife, alpha")


# --------------------------------------------------------------------------- columns

class Col:
    """A [series x bars] column with the pandas operations indicators use."""

    __slots__ = ("a", "ops")
    __array_priority__ = 1000

    def __init__(self, a, ops):
        self.a = a
        self.ops = ops

    # --- plumbing
    def _w(self, a):
        return Col(a, self.ops)

    def _v(self, other):
        if isinstance(other, Col):
            return other.a
        if isinstance(other, np.ndarray) and self.ops is not NUMPY:
            return self.ops._t(other)
        return other

    @property
    def shape(self):
        return tuple(self.a.shape)

    @property
    def k(self) -> "Kit":
        """The batch's kit for this column's device and shape."""
        return Kit(self.ops, self.shape)

    def __array__(self, dtype=None, copy=None):
        if self.ops is NUMPY:
            return np.asarray(self.a, dtype=dtype)
        raise TypeError("A GPU column cannot become a numpy array inside an indicator; use b.k and Col methods")

    def __len__(self):
        return self.a.shape[1]

    def __bool__(self):
        raise ValueError("The truth value of a column is ambiguous (use &, |, b.k.where)")

    def to_numpy(self):
        """The values as a numpy [S, T] array (moves GPU data to the CPU: only for results)."""
        return self.ops.to_numpy(self.a)

    # --- arithmetic
    def __add__(self, o): return self._w(self.a + self._v(o))
    def __radd__(self, o): return self._w(self._v(o) + self.a)
    def __sub__(self, o): return self._w(self.a - self._v(o))
    def __rsub__(self, o): return self._w(self._v(o) - self.a)
    def __mul__(self, o): return self._w(self.a * self._v(o))
    def __rmul__(self, o): return self._w(self._v(o) * self.a)
    def __truediv__(self, o): return self._w(_div(self.ops, self.a, self._v(o)))
    def __rtruediv__(self, o): return self._w(_div(self.ops, self._v(o), self.a))
    def __floordiv__(self, o): return self._w(self.a // self._v(o))
    def __mod__(self, o): return self._w(self.a % self._v(o))
    def __pow__(self, o): return self._w(_pow(self.ops, self.a, self._v(o)))
    def __rpow__(self, o): return self._w(_pow(self.ops, self._v(o), self.a))
    def __neg__(self): return self._w(-self.a)
    def __pos__(self): return self
    def __abs__(self): return self._w(abs(self.a))
    def __lt__(self, o): return self._w(self.a < self._v(o))
    def __le__(self, o): return self._w(self.a <= self._v(o))
    def __gt__(self, o): return self._w(self.a > self._v(o))
    def __ge__(self, o): return self._w(self.a >= self._v(o))
    def __eq__(self, o): return self._w(self.a == self._v(o))  # noqa: D105
    def __ne__(self, o): return self._w(self.a != self._v(o))
    def __and__(self, o): return self._w(self.a & self._v(o))
    def __rand__(self, o): return self._w(self._v(o) & self.a)
    def __or__(self, o): return self._w(self.a | self._v(o))
    def __ror__(self, o): return self._w(self._v(o) | self.a)
    def __xor__(self, o): return self._w(self.a ^ self._v(o))
    def __invert__(self): return self._w(~self.a)
    __hash__ = None

    # --- numpy protocol: np.log(col), np.where(cond, a, b) ...
    def __array_ufunc__(self, ufunc, method, *inputs, **kwargs):
        if method != "__call__":
            return NotImplemented
        ops = self.ops
        raw = [x.a if isinstance(x, Col) else x for x in inputs]
        if ops is NUMPY:
            with np.errstate(all="ignore"):
                res = ufunc(*raw, **kwargs)
        else:
            res = ops.ufunc(ufunc, raw, kwargs)
        if isinstance(res, tuple):
            return tuple(Col(r, ops) for r in res)
        return Col(res, ops)

    def __array_function__(self, func, types, args, kwargs):
        ops = self.ops
        handler = _FUNCTIONS.get(func.__name__)
        if handler is not None:
            return handler(ops, *args, **kwargs)
        if ops is NUMPY:
            raw = [x.a if isinstance(x, Col) else x for x in args]
            with np.errstate(all="ignore"):
                res = func(*raw, **kwargs)
            return Col(res, ops) if isinstance(res, np.ndarray) and res.ndim == 2 else res
        return ops.function(func, args, kwargs)

    # --- pandas methods
    def astype(self, dtype):
        if dtype in (float, "float", "float64", np.float64):
            return self._w(self.ops.astype_float(self.a))
        if dtype in (bool, "bool"):
            return self._w(self.a != 0)
        raise TypeError(f"astype({dtype}) is not supported in a batch")

    def copy(self):
        return self._w(self.ops.copy(self.a))

    def abs(self):
        return self._w(abs(self.a))

    def isna(self):
        return self._w(self.ops.isnan(self.a))

    isnull = isna

    def notna(self):
        return self._w(~self.ops.isnan(self.a))

    notnull = notna

    def shift(self, periods: int = 1, fill_value=None):
        """pandas' shift; with ``fill_value`` the opened bars take it and a boolean column stays
        boolean (``positive.shift(1, fill_value=False)``)."""
        k = int(periods)
        if fill_value is None:
            return self._w(self.ops.shift(self.a, k))
        shifted = self.ops.shift(self.a, k)
        T = self.shape[1]
        opened = self.ops.arange(T) < k if k >= 0 else self.ops.arange(T) >= T + k
        out = self.ops.where(opened, fill_value, shifted)
        is_bool = (self.a.dtype == bool) if self.ops is NUMPY else (self.a.dtype == self.ops.torch.bool)
        return self._w(out != 0 if is_bool else out)

    def diff(self, periods: int = 1):
        return self._w(_float(self.ops, self.a) - self.ops.shift(self.a, int(periods)))

    def pct_change(self, periods: int = 1, fill_method=None):
        if fill_method not in (None,):
            raise TypeError("pct_change(fill_method=...) is not supported in a batch")
        prev = self.ops.shift(self.a, int(periods))
        return self._w(_div(self.ops, _float(self.ops, self.a), prev) - 1)

    # pandas' method spellings of the operators
    def add(self, o): return self + o
    def sub(self, o): return self - o
    def mul(self, o): return self * o
    def div(self, o): return self / o
    truediv = div
    def pow(self, o): return self ** o
    def radd(self, o): return o + self
    def rsub(self, o): return o - self
    def rmul(self, o): return o * self
    def rdiv(self, o): return o / self
    def lt(self, o): return self < o
    def le(self, o): return self <= o
    def gt(self, o): return self > o
    def ge(self, o): return self >= o
    def eq(self, o): return self == o
    def ne(self, o): return self != o
    def sqrt(self): return np.sqrt(self)
    def neg(self): return -self

    def where(self, cond, other=np.nan):
        return self._w(self.ops.where(_raw(cond), _float(self.ops, self.a), self._v(other)))

    def mask(self, cond, other=np.nan):
        c = _raw(cond)
        return self._w(self.ops.where(~c, _float(self.ops, self.a), self._v(other)))

    def fillna(self, value):
        a = _float(self.ops, self.a)
        return self._w(self.ops.where(self.ops.isnan(a), self._v(value), a))

    def ffill(self, limit=None):
        return self._w(self.ops.ffill(self.a, limit))

    def clip(self, lower=None, upper=None):
        return self._w(self.ops.clip(self.a, self._v(lower) if lower is not None else None,
                                     self._v(upper) if upper is not None else None))

    def cumsum(self): return self._w(self.ops.cum(self.a, "cumsum"))
    def cumprod(self): return self._w(self.ops.cum(self.a, "cumprod"))
    def cummax(self): return self._w(self.ops.cum(self.a, "cummax"))
    def cummin(self): return self._w(self.ops.cum(self.a, "cummin"))

    def first(self):
        """Each series' first bar (``series.iloc[0]``) as a [S, 1] column that broadcasts over bars."""
        return self._w(_float(self.ops, self.a)[:, :1])

    def replace(self, values, value):
        """``series.replace(values, value)`` for numbers (e.g. infinities to NaN)."""
        a = _float(self.ops, self.a)
        hit = None
        for v in (values if isinstance(values, (list, tuple)) else [values]):
            m = (a == v) if not (isinstance(v, float) and math.isnan(v)) else self.ops.isnan(a)
            hit = m if hit is None else (hit | m)
        return self._w(self.ops.where(hit, value, a)) if hit is not None else self

    def round(self, decimals: int = 0):
        if self.ops is NUMPY:
            return self._w(np.round(self.a, decimals))
        return self._w(self.ops.torch.round(self.a, decimals=decimals))

    def rolling(self, window: int, min_periods: int | None = None):
        if not isinstance(window, (int, np.integer)) or isinstance(window, bool):
            raise TypeError("rolling window must be an integer number of bars")
        return Rolling(self, int(window), min_periods)

    def ewm(self, com=None, span=None, halflife=None, alpha=None, min_periods=0, adjust=True,
            ignore_na=False, times=None):
        spec = dict(com=com, span=span, halflife=halflife, alpha=alpha, min_periods=min_periods,
                    adjust=adjust, ignore_na=ignore_na)
        spec = {k: v for k, v in spec.items() if v is not None}
        if times is not None:
            raise TypeError("ewm(times=...) is not supported in a batch")
        return Ewm(self, spec)


def _raw(x):
    return x.a if isinstance(x, Col) else x


def _float(ops, a):
    if ops is NUMPY:
        return a if (isinstance(a, np.ndarray) and a.dtype.kind == "f") else np.asarray(a, dtype=float)
    return ops._t(a).to(ops.f64)


def _div(ops, a, b):
    if ops is NUMPY:
        with np.errstate(all="ignore"):
            return np.true_divide(a, b)
    return ops._t(a).to(ops.f64) / ops._t(b).to(ops.f64)


def _pow(ops, a, b):
    if ops is NUMPY:
        with np.errstate(all="ignore"):
            return np.power(np.asarray(a, dtype=float) if isinstance(a, np.ndarray) else a, b)
    t = ops.torch
    return t.pow(ops._t(a).to(ops.f64), ops._t(b).to(ops.f64) if isinstance(b, (t.Tensor, np.ndarray)) else float(b))


class Rolling:
    __slots__ = ("col", "n", "mp")

    def __init__(self, col, n, mp):
        self.col, self.n, self.mp = col, n, mp

    def _go(self, how, *args, **kwargs):
        c = self.col
        return Col(c.ops.rolling(c.a, self.n, self.mp, how, *args, **kwargs), c.ops)

    def sum(self): return self._go("sum")
    def mean(self): return self._go("mean")
    def min(self): return self._go("min")
    def max(self): return self._go("max")
    def count(self): return self._go("count")
    def median(self): return self._go("median")
    def skew(self): return self._go("skew")
    def kurt(self): return self._go("kurt")

    def std(self, ddof: int = 1): return self._go("std", ddof=ddof)
    def var(self, ddof: int = 1): return self._go("var", ddof=ddof)

    def quantile(self, q: float, interpolation: str = "linear"):
        return self._go("quantile", q, interpolation=interpolation)

    def rank(self, method: str = "average", ascending: bool = True, pct: bool = False):
        """pandas' rolling rank of the newest bar in each window (through pandas)."""
        return self._go("rank", method=method, ascending=ascending, pct=pct)

    def corr(self, other):
        c = self.col
        return Col(c.ops.rolling_pair(c.a, other.a, self.n, self.mp, "corr"), c.ops)

    def cov(self, other):
        c = self.col
        return Col(c.ops.rolling_pair(c.a, other.a, self.n, self.mp, "cov"), c.ops)

    def apply(self, *_args, **_kwargs):
        raise TypeError("rolling().apply runs Python per window; write it with b.k.windows instead")


class Ewm:
    __slots__ = ("col", "spec")

    def __init__(self, col, spec):
        self.col, self.spec = col, spec

    def mean(self):
        c = self.col
        return Col(c.ops.ewm(c.a, "mean", self.spec), c.ops)

    def std(self, bias: bool = False):
        c = self.col
        return Col(c.ops.ewm(c.a, "std", self.spec, bias=bias), c.ops)

    def var(self, bias: bool = False):
        c = self.col
        return Col(c.ops.ewm(c.a, "var", self.spec, bias=bias), c.ops)


# numpy functions a column answers itself (np.where, np.clip, ...)
def _np_where(ops, cond, a=None, b=None):
    if a is None:
        raise TypeError("np.where(cond) without values is not supported in a batch")
    ref = next(x for x in (cond, a, b) if isinstance(x, Col))
    shape = ref.a.shape
    va = a.a if isinstance(a, Col) else a
    vb = b.a if isinstance(b, Col) else b
    vc = _raw(cond)
    if ops is NUMPY:
        return Col(np.where(vc, va, vb), ops)
    return Col(ops.where(vc, va, vb).expand(shape) if ops.where(vc, va, vb).dim() else ops.where(vc, va, vb), ops)


def _np_clip(ops, a, lo, hi, **_):
    return Col(ops.clip(_raw(a), _raw(lo), _raw(hi)), ops)


def _np_nan_to_num(ops, a, copy=True, nan=0.0, posinf=None, neginf=None):
    x = _float(ops, _raw(a))
    if ops is NUMPY:
        return Col(np.nan_to_num(x, nan=nan, posinf=posinf, neginf=neginf), ops)
    return Col(ops.torch.nan_to_num(x, nan=nan, posinf=posinf, neginf=neginf), ops)


def _np_select(ops, condlist, choicelist, default=0.0):
    out = None
    for cond, choice in reversed(list(zip(condlist, choicelist))):
        base = default if out is None else out
        out = ops.where(_raw(cond), _raw(choice), _raw(base))
    return Col(out, ops)


_FUNCTIONS = {"where": _np_where, "clip": _np_clip, "nan_to_num": _np_nan_to_num, "select": _np_select}


# --------------------------------------------------------------------------- per-bar loops

from .jit import njit, prange  # noqa: E402


@njit
def _wilder_rows(a, v, n, out):
    """research_common.rma's loop, compiled, one row per series."""
    for s in prange(a.shape[0]):
        started = False
        prev = np.nan
        for i in range(a.shape[1]):
            if started:
                prev = prev + (a[s, i] - prev) / n
            elif np.isfinite(v[s, i]):
                prev = v[s, i]
                started = True
            out[s, i] = prev


class _NumpyXP:
    """numpy for [S] vectors in per-bar loops: the same IEEE operations as the scalar loops
    the scripts had, so a CPU batch stays bit for bit."""
    nan = float("nan")

    @staticmethod
    def where(c, a, b): return np.where(c, a, b)
    @staticmethod
    def minimum(a, b): return np.minimum(a, b)
    @staticmethod
    def maximum(a, b): return np.maximum(a, b)
    @staticmethod
    def isfinite(a): return np.isfinite(a)
    @staticmethod
    def isnan(a): return np.isnan(a)
    @staticmethod
    def abs(a): return np.abs(a)
    @staticmethod
    def sqrt(a): return np.sqrt(a)
    @staticmethod
    def exp(a): return np.exp(a)
    @staticmethod
    def log(a): return np.log(a)
    @staticmethod
    def full(n, v): return np.full(n, v, dtype=float)
    @staticmethod
    def flags(n, v=False): return np.full(n, bool(v))


NUMPY_XP = _NumpyXP()


class _TorchXP:
    nan = float("nan")

    def __init__(self, ops):
        self.ops = ops
        self.t = ops.torch

    def _x(self, a):
        return self.ops._t(a)

    def where(self, c, a, b): return self.ops.where(c, a, b)
    def minimum(self, a, b): return self.t.minimum(self._x(a).to(self.ops.f64), self._x(b).to(self.ops.f64))
    def maximum(self, a, b): return self.t.maximum(self._x(a).to(self.ops.f64), self._x(b).to(self.ops.f64))
    def isfinite(self, a): return self.t.isfinite(self._x(a))
    def isnan(self, a): return self.t.isnan(self._x(a))
    def abs(self, a): return self.t.abs(self._x(a))
    def sqrt(self, a): return self.t.sqrt(self._x(a))
    def exp(self, a): return self.t.exp(self._x(a))
    def log(self, a): return self.t.log(self._x(a))
    def full(self, n, v): return self.t.full((n,), float(v), dtype=self.ops.f64, device=self.ops.device)
    def flags(self, n, v=False): return self.t.full((n,), bool(v), dtype=self.t.bool, device=self.ops.device)


# --------------------------------------------------------------------------- the batch

class Kit:
    """What pandas spells differently on a frame, for one batch."""

    nan = float("nan")

    def __init__(self, ops, shape):
        self.ops = ops
        self.shape = shape

    def col(self, a) -> Col:
        """A column from an array or a scalar (broadcast to the batch)."""
        if isinstance(a, Col):
            return a
        if np.isscalar(a):
            return Col(self.ops.full(self.shape, a), self.ops)
        return Col(self.ops.asarray(a), self.ops)

    def full(self, value) -> Col:
        return Col(self.ops.full(self.shape, value), self.ops)

    def where(self, cond, a, b) -> Col:
        return _np_where(self.ops, cond if isinstance(cond, Col) else self.col(cond), a, b)

    def max_of(self, *cols, skipna: bool = True) -> Col:
        """``pd.concat(cols, axis=1).max(axis=1)`` per series: the largest finite value of each bar."""
        return Col(self.ops.max_of([_raw(c) for c in cols], skipna), self.ops)

    def min_of(self, *cols, skipna: bool = True) -> Col:
        return Col(self.ops.min_of([_raw(c) for c in cols], skipna), self.ops)

    def bar(self) -> Col:
        """The bar number 0..T-1 on every series."""
        S, T = self.shape
        if self.ops is NUMPY:
            return Col(np.broadcast_to(np.arange(T, dtype=float), (S, T)).copy(), self.ops)
        return Col(self.ops.torch.arange(T, device=self.ops.device, dtype=self.ops.f64).expand(S, T).clone(), self.ops)

    def warmup(self, col: Col, bars: int) -> Col:
        """The column with its first ``bars`` bars set to NaN (bars counted on the batch's grid)."""
        a = self.ops.copy(_float(self.ops, col.a))
        a[:, :max(0, int(bars))] = float("nan")
        return Col(a, self.ops)

    def windows(self, col: Col, n: int):
        """Every complete window of n bars as a [S, T-n+1, n] array (the window ending at bar n-1+j is [:, j])."""
        return self.ops.windows(_float(self.ops, col.a), int(n))

    def from_windows(self, values, n: int) -> Col:
        """A [S, T-n+1] result per complete window back on the grid (NaN before the first full window)."""
        out = self.ops.full(self.shape, float("nan"))
        out[:, int(n) - 1:] = values
        return Col(out, self.ops)

    @property
    def xp(self):
        """Array functions for per-bar loops on [S] vectors (numpy or the GPU's)."""
        return NUMPY_XP if self.ops is NUMPY else _TorchXP(self.ops)

    @property
    def np(self):
        """numpy (CPU) or the GPU's numpy-like namespace, for raw [S, ...] arrays."""
        return NP if self.ops is NUMPY else _TorchNP(self.ops)

    def array(self, col) -> object:
        """The raw [S, T] float array of a column (numpy or GPU), for per-bar loops."""
        return _float(self.ops, col.a if isinstance(col, Col) else self.col(col).a)

    def empty(self) -> object:
        """A raw [S, T] array of NaN to fill bar by bar."""
        return self.ops.full(self.shape, float("nan"))

    def wrap(self, a) -> Col:
        return Col(a, self.ops)

    def wilder(self, x: Col, seed: Col, n) -> Col:
        """Wilder's recurrence: from the first bar where ``seed`` is finite (the value there),
        y = y + (x - y) / n; NaN before it and for good after a missing x. On the CPU the scripts'
        loop over all series at once (bit for bit); on the GPU a parallel scan."""
        ops = self.ops
        a, v = _float(ops, x.a), _float(ops, seed.a)
        if ops is not NUMPY:
            return Col(rma_gpu(ops, a, v, n), ops)
        out = np.full(a.shape, np.nan)
        if a.shape[1]:
            _wilder_rows(np.ascontiguousarray(a), np.ascontiguousarray(v), float(n), out)
        return Col(out, ops)

    def convolve_valid(self, col: Col, weights) -> object:
        """``np.convolve(series, weights, 'valid')`` for every series: a [S, T-n+1] array.
        On the CPU it is np.convolve itself, row by row (bit for bit as before)."""
        w = np.asarray(weights, dtype=float)
        a = _float(self.ops, col.a)
        if self.ops is NUMPY:
            return np.stack([np.convolve(row, w, "valid") for row in a])
        kernel = self.ops.asarray(w[::-1].copy())
        return self.ops.windows(a, len(w)) @ kernel

    def hysteresis(self, score: Col, enter: float) -> Col:
        """The contract's hysteresis machine for every series: +1/-1 when the score crosses
        ``enter`` with a sign change, otherwise the previous state; 0 before the first commitment.
        Exactly ``contract.hysteresis_flip`` per series (a state is the last committing event)."""
        ops = self.ops
        s = _float(ops, score.a)
        enter = float(enter)
        if enter < 0:
            # Both branches can fire on one bar: the state machine is not a last-event fill.
            return self._hysteresis_loop(s, enter)
        fin = ops.isfinite(s)
        up = fin & (s > enter)
        down = fin & (s < -enter)
        if ops is NUMPY:
            event = np.where(up, 1.0, np.where(down, -1.0, 0.0))
        else:
            event = ops.where(up, 1.0, ops.where(down, -1.0, 0.0))
        last = ops.last_index(event != 0)
        if ops is NUMPY:
            got = np.take_along_axis(event, np.maximum(last, 0), axis=1)
            return Col(np.where(last >= 0, got, 0.0), ops)
        got = ops.take_along(event, last.clamp(min=0))
        return Col(ops.where(last >= 0, got, 0.0), ops)

    def _hysteresis_loop(self, s, enter):
        from .contract import hysteresis_flip
        rows = self.ops.to_numpy(s)
        return Col(self.ops.asarray(np.stack([hysteresis_flip(r, enter) for r in rows])), self.ops)


class Batch:
    """S series on one time grid: what an indicator's ``_compute_batch`` gets."""

    def __init__(self, columns: dict, index: pd.DatetimeIndex, ops):
        self._cols = columns
        self.index = index
        self.ops = ops
        first = next(iter(columns.values()))
        self.shape = tuple(first.shape)
        self.k = Kit(ops, self.shape)

    @property
    def size(self) -> int:
        return self.shape[0]

    @property
    def bars(self) -> int:
        return self.shape[1]

    def __len__(self):
        return self.shape[1]

    def __contains__(self, name):
        return name in self._cols

    def __getitem__(self, name) -> Col:
        try:
            return Col(self._cols[name], self.ops)
        except KeyError:
            raise KeyError(name) from None

    def __getattr__(self, name):
        if name in OHLCV:
            return self[name]
        raise AttributeError(name)

    @property
    def device(self) -> str:
        return self.ops.name

    def frames(self) -> list:
        """The batch as one pandas frame per series (CPU), for members without the batch form."""
        cols = {name: self.ops.to_numpy(a) for name, a in self._cols.items()}
        return [pd.DataFrame({name: arr[i] for name, arr in cols.items()}, index=self.index)
                for i in range(self.size)]

    def signals_of(self, indicator):
        """Another indicator's -1/0/+1 votes for every series of this batch as a numpy [S, T] array:
        its batch form when it has one, else its signal() per series (consensus members)."""
        if type(indicator).has_batch():
            out = indicator._compute_batch(self)
            values = out.to_numpy() if hasattr(out, "to_numpy") else np.asarray(out)
            return np.stack([_check_output(indicator.name, row, self.bars) for row in values])
        return np.stack([indicator.signal(df).to_numpy() for df in self.frames()])

    @classmethod
    def from_frames(cls, frames: list, ops=None) -> "Batch":
        """Frames on one index (checked) as one batch."""
        ops = ops or NUMPY
        index = frames[0].index
        for f in frames[1:]:
            if not f.index.equals(index):
                raise ValueError("A batch needs frames on one time grid; group them with group_frames")
        cols = {}
        for name in OHLCV:
            if all(name in f.columns for f in frames):
                cols[name] = ops.asarray(np.stack([f[name].to_numpy(dtype=float) for f in frames]))
        return cls(cols, index, ops)

    @classmethod
    def of(cls, df: pd.DataFrame, ops=None) -> "Batch":
        return cls.from_frames([df], ops)


def group_frames(frames: list) -> list[list[int]]:
    """Positions of the frames that share one time grid, in first-seen order."""
    groups: dict = {}
    order: list = []
    for i, f in enumerate(frames):
        idx = f.index
        key = (len(idx), int(idx.asi8[0]) if len(idx) else 0, int(idx.asi8[-1]) if len(idx) else 0,
               hash(idx.asi8.tobytes()) if len(idx) else 0)
        if key not in groups:
            groups[key] = []
            order.append(key)
        groups[key].append(i)
    return [groups[k] for k in order]


# --------------------------------------------------------------------------- running indicators

# GPU working set per call: series x bars per chunk (a [S, T] float64 array is 8 bytes a cell, and
# an indicator keeps a few dozen of them alive). SMITHERY_GPU_CELLS overrides it.
DEFAULT_GPU_CELLS = 6_000_000


def _check_frame(name: str, df: pd.DataFrame) -> None:
    """The input checks of TrendIndicator.signal, for one frame."""
    if not isinstance(df.index, pd.DatetimeIndex):
        raise TypeError(f"{name}: df must have a DatetimeIndex")
    missing = [c for c in OHLCV[:4] if c not in df.columns]
    if missing:
        raise ValueError(f"{name}: missing columns {missing}")
    if not df.index.is_monotonic_increasing or not df.index.is_unique:
        raise ValueError(f"{name}: timestamps must be ordered and unique")
    prices = df[list(OHLCV[:4])].to_numpy(dtype=float)
    if not np.isfinite(prices).all() or (prices <= 0).any():
        raise ValueError(f"{name}: OHLC prices must be finite and positive")


def _check_output(name: str, raw: np.ndarray, bars: int) -> np.ndarray:
    """The output checks of TrendIndicator.signal, for one series."""
    raw = np.asarray(raw, dtype=float)
    if raw.shape != (bars,):
        raise ValueError(f"{name}: signal length mismatch")
    bad = ~np.isin(raw, (-1.0, 0.0, 1.0))
    if bad.any():
        raise ValueError(f"{name}: non-ternary values at {bad.sum()} bars")
    nz = np.flatnonzero(raw != 0)
    if len(nz) and (raw[nz[0]:] == 0).any():
        raise ValueError(f"{name}: signal returns to 0 after warm-up (Law: no neutral cop-out)")
    return raw


def _chunks(positions: list[int], bars: int, ops) -> list[list[int]]:
    if ops is NUMPY:
        return [positions]
    cells = int(os.environ.get("SMITHERY_GPU_CELLS", DEFAULT_GPU_CELLS))
    from . import pace
    size = pace.batch(max(1, cells // max(bars, 1)))
    return [positions[i:i + size] for i in range(0, len(positions), size)]


def signals(indicator, frames: list, device: str = "auto", stats: dict | None = None) -> list:
    """Every frame's signal from one indicator instance: a float array of -1/0/+1 per frame,
    or the exception that frame's own ``indicator.signal(frame)`` would raise.

    Frames on one time grid are computed in one call, on the GPU when the indicator has the
    batched form and CUDA is available (``device`` ``"auto"``), else frame by frame as before.
    A batch that fails is retried frame by frame on the CPU, so one bad series cannot sink the
    others. ``stats`` (optional) counts ``gpu_series``, ``cpu_series`` and ``retried``.
    """
    name = getattr(indicator, "name", type(indicator).__name__)
    out: list = [None] * len(frames)
    ok: list[int] = []
    for i, df in enumerate(frames):
        try:
            _check_frame(name, df)
        except Exception as exc:  # noqa: BLE001 — the frame's own signal() raises the same
            out[i] = exc
            continue
        if df.empty:
            out[i] = np.zeros(0, dtype=float)
        else:
            ok.append(i)
    if stats is None:
        stats = {}
    for key in ("gpu_series", "cpu_series", "retried"):
        stats.setdefault(key, 0)

    def one(i, reason):
        try:
            out[i] = _check_output(name, indicator._compute(frames[i]), len(frames[i]))
        except Exception as exc:  # noqa: BLE001
            out[i] = exc
        stats["cpu_series"] += 1
        _count("cpu", 1, reason)

    where, reason = device_for(indicator, device)
    if not type(indicator).has_batch():
        for i in ok:
            one(i, reason)
        return out
    # A script whose logic steps bar by bar sets ``gpu = False``: batched on the CPU it is faster
    # than thousands of tiny GPU launches. "auto" honours that; an explicit device does not.
    if device == "auto" and getattr(indicator, "gpu", True) is False:
        device = "cpu"
    ops = ops_for(device)
    stats["device"] = "gpu" if ops is not NUMPY else "cpu"
    sub = [frames[i] for i in ok]
    for group in group_frames(sub):
        positions = [ok[g] for g in group]
        for chunk in _chunks(positions, len(frames[positions[0]]), ops):
            from . import pace
            pacer = pace.Pacer(sync=(ops.torch.cuda.synchronize if ops is not NUMPY else None))
            pacer.start()
            try:
                b = Batch.from_frames([frames[i] for i in chunk], ops)
                res = indicator._compute_batch(b)
                values = res.to_numpy() if hasattr(res, "to_numpy") else np.asarray(res)
            except Exception:  # noqa: BLE001 — isolate the failing series on the CPU
                pacer.done()
                stats["retried"] += len(chunk)
                for i in chunk:
                    one(i, "batch failed, retried per series")
                continue
            pacer.done()
            for row, i in zip(values, chunk):
                try:
                    out[i] = _check_output(name, row, len(frames[i]))
                except Exception as exc:  # noqa: BLE001
                    out[i] = exc
            stats["gpu_series" if ops is not NUMPY else "cpu_series"] += len(chunk)
            _count("gpu" if ops is not NUMPY else "cpu", len(chunk), reason)
    return out


# --------------------------------------------------------------------------- numpy-like namespaces for window functions

class _NP:
    """numpy itself for window functions on [rows, length] blocks (CPU, bit for bit as before),
    plus the few helpers numpy lacks (rankdata from scipy)."""

    def __getattr__(self, name):
        return getattr(np, name)

    @staticmethod
    def rankdata(a, axis=1, method="average"):
        from scipy.stats import rankdata
        return rankdata(a, axis=axis, method=method)

    @staticmethod
    def is_gpu(_a=None) -> bool:
        return False

    @staticmethod
    def asnumpy(a):
        return np.asarray(a)

    @staticmethod
    def ndtr(a):
        from scipy.special import ndtr
        return ndtr(a)

    @staticmethod
    def ndtri(a):
        from scipy.special import ndtri
        return ndtri(a)

    @staticmethod
    def asfloat(a):
        return np.asarray(a).astype(float)

    @staticmethod
    def pinv(a, rcond=1e-15):
        return np.linalg.pinv(a, rcond=rcond)

    @staticmethod
    def quantiles(a, qs, axis=1):
        return np.quantile(a, list(qs), axis=axis, method='linear')

    @staticmethod
    def rankdata_last(a):
        from scipy.stats import rankdata
        return rankdata(a, axis=a.ndim - 1, method='average')

    @staticmethod
    def rfft(a, axis=-1):
        return np.fft.rfft(a, axis=axis)

    @staticmethod
    def irfft(a, n=None, axis=-1):
        return np.fft.irfft(a, n=n, axis=axis)


NP = _NP()


class _TorchNP:
    """The numpy calls window functions use, on torch tensors (float64, CUDA)."""

    def __init__(self, ops):
        self.ops = ops
        self.t = ops.torch
        self.pi, self.e, self.inf, self.nan = math.pi, math.e, math.inf, math.nan
        self.newaxis = None
        self.float64 = self.t.float64

    # construction
    def asarray(self, a, dtype=None):
        if isinstance(a, (list, tuple)):
            return self.t.as_tensor(np.asarray(a, dtype=float), dtype=self.t.float64, device=self.ops.device)
        return self.ops._t(a)

    def asnumpy(self, a):
        return self.ops.to_numpy(a)

    def is_gpu(self, _a=None) -> bool:
        return True

    def arange(self, *args, dtype=None):
        if dtype in (float, "float", np.float64) or any(isinstance(x, float) for x in args):
            return self.t.arange(*args, dtype=self.t.float64, device=self.ops.device)
        return self.t.arange(*args, device=self.ops.device)

    def full(self, shape, value, dtype=None):
        shape = shape if isinstance(shape, tuple) else (shape,)
        if dtype is bool:
            return self.t.full(shape, bool(value), device=self.ops.device)
        return self.t.full(shape, float(value), dtype=self.t.float64, device=self.ops.device)

    def zeros(self, shape, dtype=None):
        return self.full(shape, 0.0, dtype)

    def ones(self, shape, dtype=None):
        return self.full(shape, 1.0, dtype)

    def zeros_like(self, a):
        return self.t.zeros_like(a)

    def ones_like(self, a):
        return self.t.ones_like(a)

    def full_like(self, a, v):
        return self.t.full_like(a, float(v))

    def triu_indices(self, n, k=0):
        i = self.t.triu_indices(n, n, offset=k, device=self.ops.device)
        return i[0], i[1]

    def tril_indices(self, n, k=0):
        i = self.t.tril_indices(n, n, offset=k, device=self.ops.device)
        return i[0], i[1]

    # elementwise
    def _f(self, a):
        x = self.ops._t(a)
        return x if x.is_complex() else x.to(self.t.float64)

    def rfft(self, a, axis=-1):
        return self.t.fft.rfft(self._f(a), dim=axis)

    def irfft(self, a, n=None, axis=-1):
        return self.t.fft.irfft(a, n=n, dim=axis)

    def where(self, c, a, b): return self.ops.where(c, a, b)
    def abs(self, a): return self.t.abs(self.ops._t(a))
    def sign(self, a): return self.t.sign(self.ops._t(a))
    def sqrt(self, a): return self.t.sqrt(self._f(a))
    def exp(self, a): return self.t.exp(self._f(a))
    def log(self, a): return self.t.log(self._f(a))
    def log1p(self, a): return self.t.log1p(self._f(a))
    def log2(self, a): return self.t.log2(self._f(a))
    def tanh(self, a): return self.t.tanh(self._f(a))
    def arcsinh(self, a): return self.t.asinh(self._f(a))
    def arccosh(self, a): return self.t.acosh(self._f(a))
    def arctanh(self, a): return self.t.atanh(self._f(a))
    def sinh(self, a): return self.t.sinh(self._f(a))
    def cosh(self, a): return self.t.cosh(self._f(a))
    def expm1(self, a): return self.t.expm1(self._f(a))
    def square(self, a): return self.t.square(self._f(a))
    def log10(self, a): return self.t.log10(self._f(a))
    def arctan(self, a): return self.t.atan(self._f(a))
    def arctan2(self, a, b): return self.t.atan2(self._f(a), self._f(b))
    def cos(self, a): return self.t.cos(self._f(a))
    def sin(self, a): return self.t.sin(self._f(a))
    def round(self, a, decimals=0): return self.t.round(self._f(a), decimals=int(decimals))
    def floor(self, a): return self.t.floor(self.ops._t(a))
    def ceil(self, a): return self.t.ceil(self.ops._t(a))
    def isfinite(self, a): return self.t.isfinite(self.ops._t(a))
    def isnan(self, a): return self.t.isnan(self.ops._t(a))
    def real(self, a): return self.t.real(a)
    def imag(self, a): return self.t.imag(a)
    def conj(self, a): return self.t.conj(a)
    def maximum(self, a, b): return self.t.maximum(self._f(a), self._f(b))
    def minimum(self, a, b): return self.t.minimum(self._f(a), self._f(b))

    def clip(self, a, lo, hi):
        x = self._f(a)
        if lo is not None:
            x = self.t.maximum(x, self._f(lo))
        if hi is not None:
            x = self.t.minimum(x, self._f(hi))
        return x

    def divide(self, a, b, out=None, where=None):
        a, b = self._f(a), self._f(b)
        if where is None:
            return a / b
        w = self.ops._t(where)
        shape = self.t.broadcast_shapes(a.shape, b.shape)
        base = self._f(out) if out is not None else self.t.zeros(shape, dtype=self.t.float64, device=self.ops.device)
        return self.t.where(w, a / self.t.where(w, b, self.t.ones_like(b)), base)

    def eye(self, n):
        return self.t.eye(n, dtype=self.t.float64, device=self.ops.device)

    def ndtr(self, a):
        return self.t.special.ndtr(self._f(a))

    def ndtri(self, a):
        return self.t.special.ndtri(self._f(a))

    def asfloat(self, a):
        return self._num(self.ops._t(a)).to(self.t.float64)

    def pinv(self, a, rcond=1e-15):
        return self.t.linalg.pinv(a, rtol=rcond)

    def quantiles(self, a, qs, axis=1):
        return self.t.quantile(a, self.t.as_tensor(list(qs), dtype=self.t.float64, device=self.ops.device), dim=axis)

    def rankdata_last(self, a):
        """rankdata along the last axis of any shape (average ties)."""
        shape = a.shape
        return self.rankdata(a.reshape(-1, shape[-1]), axis=1).reshape(shape)

    def flip(self, a, axis):
        return self.t.flip(a, [axis] if isinstance(axis, int) else list(axis))

    def trace(self, a, axis1=0, axis2=1):
        return self.t.diagonal(a, dim1=axis1, dim2=axis2).sum(-1)

    # reductions (bools count as 0/1, like numpy)
    def _num(self, a):
        return a.to(self.t.float64) if a.dtype == self.t.bool else a

    def sum(self, a, axis=None, keepdims=False):
        a = self._num(a)
        return a.sum() if axis is None else a.sum(dim=axis, keepdim=keepdims)

    def mean(self, a, axis=None, keepdims=False):
        a = self._num(a)
        return a.mean() if axis is None else a.mean(dim=axis, keepdim=keepdims)

    def std(self, a, axis=None, ddof=0, keepdims=False):
        return a.std(correction=ddof) if axis is None else a.std(dim=axis, correction=ddof, keepdim=keepdims)

    def var(self, a, axis=None, ddof=0, keepdims=False):
        return a.var(correction=ddof) if axis is None else a.var(dim=axis, correction=ddof, keepdim=keepdims)

    def median(self, a, axis=None, keepdims=False):
        """numpy's median: the mean of the two middle values for an even count."""
        if axis is None:
            a, axis = a.reshape(-1), 0
        n = a.shape[axis]
        s = self.t.sort(a, dim=axis).values
        lo = s.narrow(axis, (n - 1) // 2, 1)
        hi = s.narrow(axis, n // 2, 1)
        m = (lo + hi) / 2 if n % 2 == 0 else lo
        return m if keepdims else m.squeeze(axis)

    def quantile(self, a, q, axis=None, keepdims=False):
        return self.t.quantile(a, float(q), dim=axis, keepdim=keepdims, interpolation="linear")

    def max(self, a, axis=None, keepdims=False):
        return a.max() if axis is None else a.amax(dim=axis, keepdim=keepdims)

    def min(self, a, axis=None, keepdims=False):
        return a.min() if axis is None else a.amin(dim=axis, keepdim=keepdims)

    def ptp(self, a, axis=None):
        return self.max(a, axis) - self.min(a, axis)

    def argmax(self, a, axis=None):
        return a.argmax() if axis is None else a.argmax(dim=axis)

    def argmin(self, a, axis=None):
        return a.argmin() if axis is None else a.argmin(dim=axis)

    def all(self, a, axis=None):
        return a.all() if axis is None else a.all(dim=axis)

    def any(self, a, axis=None):
        return a.any() if axis is None else a.any(dim=axis)

    def cumsum(self, a, axis=-1):
        return self.t.cumsum(a, dim=axis)

    def cumprod(self, a, axis=-1):
        return self.t.cumprod(a, dim=axis)

    def diff(self, a, axis=-1):
        return self.t.diff(a, dim=axis)

    def sort(self, a, axis=-1):
        return self.t.sort(a, dim=axis).values

    def argsort(self, a, axis=-1, kind=None):
        return self.t.argsort(a, dim=axis, stable=True)

    def take_along_axis(self, a, idx, axis):
        return self.t.take_along_dim(a, idx, dim=axis)

    def stack(self, arrays, axis=0):
        return self.t.stack([self.ops._t(x) for x in arrays], dim=axis)

    def concatenate(self, arrays, axis=0):
        return self.t.cat([self.ops._t(x) for x in arrays], dim=axis)

    def outer(self, a, b):
        return self.t.outer(self.ops._t(a), self.ops._t(b))

    def dot(self, a, b):
        return self.ops._t(a) @ self.ops._t(b)

    def einsum(self, spec, *args):
        return self.t.einsum(spec, *[self.ops._t(x) for x in args])

    def rankdata(self, a, axis=1, method="average"):
        """scipy's average ranks along axis 1 (1-based; ties share the mean of their ranks)."""
        if method != "average" or axis not in (1, -1) or a.dim() != 2:
            raise TypeError("rankdata on the GPU: method='average' on axis 1 of a 2-D block only")
        t = self.t
        n = a.shape[1]
        order = t.argsort(a, dim=1, stable=True)
        s = t.take_along_dim(a, order, dim=1)
        new = t.ones_like(s, dtype=t.bool)
        new[:, 1:] = s[:, 1:] != s[:, :-1]
        group = t.cumsum(new.to(t.long), dim=1) - 1
        pos = t.arange(1, n + 1, device=a.device, dtype=t.float64).expand_as(s).contiguous()
        total = t.zeros_like(s).scatter_add_(1, group, pos)
        count = t.zeros_like(s).scatter_add_(1, group, t.ones_like(s))
        avg = t.take_along_dim(total / count.clamp(min=1), group, dim=1)
        ranks = t.empty_like(s)
        ranks.scatter_(1, order, avg)
        return ranks

    @property
    def fft(self):
        return self.t.fft

    @property
    def linalg(self):
        return self.t.linalg


def _positional_count(function) -> int:
    import inspect
    try:
        params = inspect.signature(function).parameters.values()
    except (TypeError, ValueError):
        return 2
    params = list(params)
    if any(p.kind == p.VAR_POSITIONAL for p in params):
        return 2
    # (w, xp=np) counts as two: such functions take the namespace and run on the GPU too
    return sum(1 for p in params if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD))


def rolling_apply(col: Col, length: int, function, chunk: int | None = None) -> Col:
    """``function(block, xp)`` on every complete, all-finite window of ``length`` bars of every
    series: ``block`` is [rows, length], ``xp`` numpy (CPU) or the GPU's numpy-like namespace,
    and it returns one value per row. Windows with a NaN give NaN. On the CPU this is the old
    per-series window arithmetic, row for row."""
    ops = col.ops
    S, T = col.shape
    length = int(length)
    out = ops.full((S, T), float("nan"))
    if T < length:
        return Col(out, ops)
    if _positional_count(function) == 1:
        # An old one-argument window function (numpy blocks): the same call as before.
        one = function
        function = (lambda block, xp: one(block)) if ops is NUMPY else (lambda block, xp: xp.asarray(one(xp.asnumpy(block))))
    win = ops.windows(_float(ops, col.a), length)            # [S, W, L]
    W = win.shape[1]
    block = win.reshape(S * W, length)
    xp = NP if ops is NUMPY else _TorchNP(ops)
    values = ops.full((S * W,), float("nan"))
    if chunk is None:
        cells = int(os.environ.get("SMITHERY_GPU_CELLS", DEFAULT_GPU_CELLS))
        chunk = 256 if ops is NUMPY else max(256, cells // max(length * length, 1))
    finite = np.isfinite(block).all(axis=1) if ops is NUMPY else ops.torch.isfinite(block).all(dim=1)
    for start in range(0, S * W, chunk):
        part = block[start:start + chunk]
        valid = finite[start:start + chunk]
        if bool(valid.any()):
            seg = values[start:start + chunk]
            seg[valid] = function(part[valid], xp)
    out[:, length - 1:] = values.reshape(S, W)
    return Col(out, ops)


# --------------------------------------------------------------------------- linear recurrences on the GPU

SCAN_BLOCK = 64


def linear_scan(ops, coef: float, u):
    """y[t] = coef * y[t-1] + u[t] with y[-1] = 0, for every series, without a bar loop on the GPU:
    blocks of SCAN_BLOCK bars as one matrix product, then one carry step per block.
    ``u`` must be finite (callers mask NaN themselves). Float-close to the sequential loop."""
    t = ops.torch
    S, T = u.shape
    if T == 0:
        return u.clone()
    L = SCAN_BLOCK
    C = -(-T // L)
    pad = C * L - T
    U = t.cat([u, t.zeros((S, pad), dtype=u.dtype, device=u.device)], dim=1) if pad else u
    U = U.reshape(S, C, L)
    k = t.arange(L, device=u.device, dtype=u.dtype)
    expo = k[:, None] - k[None, :]
    M = t.where(expo >= 0, t.pow(t.tensor(float(coef), dtype=u.dtype, device=u.device), expo.clamp(min=0)),
                t.zeros((), dtype=u.dtype, device=u.device))
    local = U @ M.T                                                   # [S, C, L]
    grow = t.pow(t.tensor(float(coef), dtype=u.dtype, device=u.device), k + 1)   # coef^(k+1)
    carry = t.zeros((S, C), dtype=u.dtype, device=u.device)          # y at the end of the previous block
    end = local[:, :, -1]
    last = float(coef) ** L
    prev = t.zeros(S, dtype=u.dtype, device=u.device)
    for c in range(C):
        carry[:, c] = prev
        prev = last * prev + end[:, c]
    y = local + carry[:, :, None] * grow[None, None, :]
    return y.reshape(S, C * L)[:, :T]


def rma_gpu(ops, a, valid, n):
    """Wilder's recurrence (research_common.rma) without a bar loop: seeded with the first finite
    window mean, then y = y + (a - y) / n, NaN for good after a missing value."""
    t = ops.torch
    S, T = a.shape
    idx = t.arange(T, device=a.device)
    fin = t.isfinite(valid)
    has = fin.any(dim=1)
    seed = t.where(has, fin.to(t.int8).argmax(dim=1), t.full((S,), T, device=a.device, dtype=t.long))
    after = idx[None, :] > seed[:, None]
    at = idx[None, :] == seed[:, None]
    seed_val = t.take_along_dim(valid, seed.clamp(max=T - 1)[:, None], dim=1)
    u = t.where(after, a / n, t.zeros_like(a))
    u = t.where(at, seed_val.expand(S, T), u)
    broken = t.cummax((after & t.isnan(a)).to(t.int8), dim=1).values.bool()
    u = t.where(t.isnan(u), t.zeros_like(u), u)
    y = linear_scan(ops, 1.0 - 1.0 / n, u)
    y = t.where(after | at, y, t.full_like(y, float("nan")))
    return t.where(broken, t.full_like(y, float("nan")), y)


def per_row(fn, *args, **kwargs):
    """A one-series numpy function on every series of batch columns (CPU, the same code per row):
    each Col argument is replaced by one row; the results are stacked into a column. For library
    helpers that are not vectorised across series yet."""
    cols = [a for a in args if isinstance(a, Col)]
    if not cols:
        return fn(*args, **kwargs)
    first = cols[0]
    arrays = [a.to_numpy() if isinstance(a, Col) else a for a in args]
    rows = []
    for i in range(first.shape[0]):
        # A Series row suits both kinds of helper (numpy-style np.asarray, pandas-style .rolling).
        row_args = [pd.Series(arr[i]) if isinstance(a, Col) else arr for arr, a in zip(arrays, args)]
        rows.append(np.asarray(fn(*row_args, **kwargs), dtype=float))
    return first.k.col(np.stack(rows))


def per_frame(fn, b, *args, **kwargs):
    """A one-frame helper (it takes a pandas OHLCV frame) on every series of a batch (CPU, the
    same code per series, with the series' own dates); results stacked into a column."""
    rows = [np.asarray(fn(df, *args, **kwargs), dtype=float) for df in b.frames()]
    return b.k.col(np.stack(rows))
