"""Compiled per-bar loops for batched indicators (Numba), with a plain-Python fallback.

An indicator whose logic steps bar by bar (a state machine, an online learner, a nonlinear
recurrence) keeps that loop as written, over one row at a time, and compiles it::

    from ..jit import njit, prange

    @njit
    def kernel(c, out):              # c, out: [rows, T] float64 numpy arrays
        for s in prange(c.shape[0]):
            ...                      # the scalar loop over bars, unchanged

Numba compiles it without fast-math, so the arithmetic is the IEEE arithmetic of the Python
loop (bit for bit), and ``prange`` spreads the rows over the CPU cores. Without Numba the same
function runs as plain Python. ``NUMBA_NUM_THREADS`` limits the threads (research workers set 1).
"""
from __future__ import annotations

import math
import os
import sys

import numpy as np

try:
    import numba as _numba
except Exception:  # noqa: BLE001 — Numba is optional; the loops still run, slower
    _numba = None

AVAILABLE = _numba is not None


def njit(fn=None, **options):
    """``numba.njit(parallel=True, cache=True)`` when Numba is there, else the function itself."""
    def wrap(f):
        if _numba is None or os.environ.get("SMITHERY_JIT", "1") == "0":
            return f
        opts = {"parallel": True, "cache": True, **options}
        return _numba.njit(**opts)(f)
    return wrap(fn) if fn is not None else wrap


if _numba is not None:
    prange = _numba.prange
else:  # pragma: no cover - exercised only without Numba
    prange = range


# --------------------------------------------------------------------------- per-series loops on the GPU
#
# A loop that steps bar by bar runs on the GPU as one thread per series (all series in one launch):
#
#     from ..jit import per_series
#
#     def _row(s, price, widths, out):      # plain Python: series s of [series x bars] arrays
#         for t in range(price.shape[1]):
#             ...                          # the scalar loop over bars, as written for the CPU
#
#     _gpu_rows = per_series(_row)          # call it with torch tensors on the GPU (zero copy)
#
# numba-cuda compiles the function on first use (it needs ``pip install numba-cuda[cu13]`` with
# CUDA wheels of torch's CUDA version, see requirements-gpu-kernels.txt). Plain IEEE arithmetic;
# libdevice's exp/log/tanh and fused multiply-adds can move the last bit, so parity reports every
# bar that differs, as for every GPU path. Without numba-cuda ``available()`` is False and callers
# keep their CPU loop (and say so).

_CUDA: dict = {}


def cuda_module():
    """numba-cuda's ``cuda`` when GPU kernels can run (installed, a CUDA GPU, SMITHERY_GPU not 0)."""
    if "mod" not in _CUDA:
        mod, reason = None, ""
        if os.environ.get("SMITHERY_GPU", "1").strip() == "0":
            reason = "SMITHERY_GPU=0"
        else:
            try:
                # torch first: its CUDA libraries (nvJitLink) are the ones the process keeps.
                import torch  # noqa: F401
                from numba import cuda as mod
                if not mod.is_available():
                    mod, reason = None, "numba-cuda finds no CUDA GPU"
                else:
                    _exact_multiply_add()
            except Exception as exc:  # noqa: BLE001 — optional: the CPU loops still run
                mod, reason = None, f"numba-cuda not installed ({type(exc).__name__})"
        _CUDA["mod"], _CUDA["reason"] = mod, reason
    return _CUDA["mod"]


def _exact_multiply_add():
    """NVVM contracts a*b + c into one fused multiply-add by default (one rounding instead of two);
    the CPU loops round twice. Off for smithery's kernels, so a twin does its loop's arithmetic."""
    from numba.cuda.cudadrv import nvvm
    if getattr(nvvm.compile_ir, "_smithery_exact", False):
        return
    original = nvvm.compile_ir

    def compile_ir(llvmir, **options):
        options.setdefault("fma", False)
        return original(llvmir, **options)

    compile_ir._smithery_exact = True
    nvvm.compile_ir = compile_ir
    # Kernels are linked with link-time optimization, where nvJitLink contracts again.
    try:
        import dataclasses

        from numba.cuda.cudadrv import driver
        linker = driver._Linker
        linker_options = linker._get_linker_options

        def _get_linker_options(self, ptx):
            options = linker_options(self, ptx)
            if options.link_time_optimization:
                options = dataclasses.replace(options, fma=False)
            return options

        linker._get_linker_options = _get_linker_options
    except Exception as exc:  # noqa: BLE001 — another numba-cuda: kernels run, with fused multiply-adds
        print(f"smithery.jit: fused multiply-adds stay on in linked kernels ({type(exc).__name__}: {exc})", file=sys.stderr)


def gpu_kernels() -> bool:
    """Whether per-series GPU kernels can run in this process."""
    return cuda_module() is not None


def gpu_kernels_reason() -> str:
    cuda_module()
    return _CUDA.get("reason", "")


class _PerSeries:
    def __init__(self, row):
        self.row = row
        self.kernels: dict = {}

    def _kernel(self, count):
        if count not in self.kernels:
            cuda = cuda_module()
            names = ", ".join(f"a{i}" for i in range(count))
            source = "\n".join([f"def kernel({names}):",
                                "    s = cuda.grid(1)",
                                "    if s < a0.shape[0]:",
                                f"        row(s, {names})", ""])
            scope = {"cuda": cuda, "row": cuda.jit(device=True)(self.row)}
            exec(source, scope)
            self.kernels[count] = cuda.jit(scope["kernel"])
        return self.kernels[count]

    def __call__(self, *args):
        """Run ``row(s, *args)`` for every series s (rows of the first argument, a torch tensor on
        the GPU); tensors pass without a copy, other arguments as scalars; torch's current stream."""
        cuda = cuda_module()
        if cuda is None:
            raise RuntimeError(f"GPU kernels unavailable: {gpu_kernels_reason()}")
        import torch
        rows = int(args[0].shape[0])
        if rows == 0:
            return
        values = [cuda.as_cuda_array(a.detach()) if torch.is_tensor(a) else a for a in args]
        stream = cuda.external_stream(torch.cuda.current_stream().cuda_stream)
        threads = 32
        self._kernel(len(args))[(rows + threads - 1) // threads, threads, stream](*values)


def per_series(row):
    """A GPU kernel from a per-series loop ``row(s, *arrays_and_scalars)``: one thread per series."""
    return _PerSeries(row)


class _PerCell(_PerSeries):
    def _kernel(self, count):
        if count not in self.kernels:
            cuda = cuda_module()
            names = ", ".join(f"a{i}" for i in range(count))
            source = "\n".join([f"def kernel({names}):",
                                "    t, s = cuda.grid(2)",
                                "    if s < a0.shape[0] and t < a0.shape[1]:",
                                f"        cell(s, t, {names})", ""])
            scope = {"cuda": cuda, "cell": cuda.jit(device=True)(self.row)}
            exec(source, scope)
            self.kernels[count] = cuda.jit(scope["kernel"])
        return self.kernels[count]

    def __call__(self, *args):
        """Run ``cell(s, t, *args)`` for every series s and bar t of the first argument (a torch
        tensor on the GPU): one thread per cell, for bars that do not depend on each other."""
        cuda = cuda_module()
        if cuda is None:
            raise RuntimeError(f"GPU kernels unavailable: {gpu_kernels_reason()}")
        import torch
        rows, bars = int(args[0].shape[0]), int(args[0].shape[1])
        if rows == 0 or bars == 0:
            return
        values = [cuda.as_cuda_array(a.detach()) if torch.is_tensor(a) else a for a in args]
        stream = cuda.external_stream(torch.cuda.current_stream().cuda_stream)
        threads = (64, 4)
        blocks = ((bars + threads[0] - 1) // threads[0], (rows + threads[1] - 1) // threads[1])
        self._kernel(len(args))[blocks, threads, stream](*values)


def per_cell(cell):
    """A GPU kernel from ``cell(s, t, *arrays_and_scalars)``: one thread per series and bar."""
    return _PerCell(cell)


# --------------------------------------------------------------------------- CUDA twins of compiled loops
#
# A compiled per-series loop (``@njit`` with ``for s in prange(rows): ...``) runs on the GPU as the
# same code: its source becomes a CUDA kernel with one thread per series (the prange body unchanged,
# numpy's scalar functions mapped to math / libdevice). ``launch(fn, *args)`` calls ``fn`` itself on
# numpy arrays (the CPU path, bit for bit as before) and its twin when the arrays are CUDA tensors.

_NP_MATH = {"isfinite": "isfinite", "isnan": "isnan", "isinf": "isinf", "sqrt": "sqrt", "log": "log",
            "exp": "exp", "log1p": "log1p", "expm1": "expm1", "log10": "log10", "log2": "log2",
            "arctan2": "atan2", "arctan": "atan", "arcsin": "asin", "arccos": "acos", "tanh": "tanh",
            "sinh": "sinh", "cosh": "cosh", "sin": "sin", "cos": "cos", "tan": "tan", "floor": "floor",
            "ceil": "ceil", "hypot": "hypot", "erf": "erf", "erfc": "erfc", "nan": "nan", "inf": "inf",
            "pi": "pi", "e": "e"}
_NP_BUILTIN = {"abs": "abs", "absolute": "abs", "fabs": "abs", "maximum": "max", "minimum": "min",
               "float64": "float", "int64": "int"}
_NP_LIBDEVICE = {"rint": "rint", "trunc": "trunc"}
_TWINS: dict = {}
_BOUNDS: dict = {}
_DEVICE: dict = {}
_TWIN_WARNED: list = []


class _ToCuda:
    """Rewrites a compiled loop's AST for numba-cuda (scalar numpy calls only; no allocation)."""

    def __init__(self, name):
        import ast
        self.ast, self.name = ast, name

    def run(self, tree):
        ast = self.ast
        name = self.name

        class T(ast.NodeTransformer):
            def visit_Attribute(self, node):
                self.generic_visit(node)
                if isinstance(node.value, ast.Name) and node.value.id == "np":
                    attr = node.attr
                    if attr in _NP_MATH:
                        return ast.copy_location(ast.Attribute(ast.Name("math", ast.Load()), _NP_MATH[attr], ast.Load()), node)
                    if attr in _NP_BUILTIN:
                        return ast.copy_location(ast.Name(_NP_BUILTIN[attr], ast.Load()), node)
                    if attr in _NP_LIBDEVICE:
                        return ast.copy_location(ast.Attribute(ast.Name("libdevice", ast.Load()), attr, ast.Load()), node)
                    raise NotImplementedError(f"{name}: np.{attr} has no CUDA form (write the twin by hand)")
                return node

        return T().visit(tree)


def _parsed(fn):
    import ast
    import inspect
    import textwrap
    py = getattr(fn, "py_func", fn)
    tree = ast.parse(textwrap.dedent(inspect.getsource(py)))
    func = tree.body[0]
    func.decorator_list = []
    body = list(func.body)
    if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant) and isinstance(body[0].value.value, str):
        body = body[1:]
    return py, func, body


def _scope(py, func):
    """The globals a twin compiles against: the module's, with its compiled helpers as device functions."""
    import ast
    cuda = cuda_module()
    from numba import float64, int64
    scope = dict(py.__globals__)
    scope.update(cuda=cuda, math=math, libdevice=cuda.libdevice, float64=float64, int64=int64)
    for node in ast.walk(func):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            target = py.__globals__.get(node.func.id)
            if target is pairwise_sum:
                scope[node.func.id] = _cuda_pairwise_sum()
            elif target is not None and hasattr(target, "py_func"):
                scope[node.func.id] = _device_twin(target)
    return scope


def _device_twin(fn):
    """A compiled helper (called from a loop) as a CUDA device function."""
    import ast
    if fn not in _DEVICE:
        py, func, body = _parsed(fn)
        func.body = body or [ast.Pass()]
        func = _ToCuda(py.__name__).run(func)
        ast.fix_missing_locations(func)
        scope = _scope(py, func)
        exec(compile(ast.unparse(func), f"<cuda twin of {py.__name__}>", "exec"), scope)
        _DEVICE[fn] = cuda_module().jit(device=True)(scope[py.__name__])
    return _DEVICE[fn]


def _twin(fn):
    """The CUDA kernel of a compiled ``for s in prange(...)`` loop: one thread per series."""
    import ast
    import textwrap
    if fn in _TWINS:
        return _TWINS[fn]
    py, func, body = _parsed(fn)
    found = [k for k, stmt in enumerate(body) if isinstance(stmt, ast.For) and isinstance(stmt.iter, ast.Call)
             and getattr(stmt.iter.func, "id", "") == "prange"]
    if len(found) != 1 or body[found[0] + 1:]:
        raise NotImplementedError(f"{py.__name__}: not one prange loop at the end (write the twin by hand)")
    k = found[0]
    loop = body[k]
    bounds = loop.iter.args
    if len(bounds) not in (1, 2) or not isinstance(loop.target, ast.Name):
        raise NotImplementedError(f"{py.__name__}: prange(start, stop) only")
    # The launch's thread count: the prange bound evaluated on the host (a loop may run over
    # series x checkpoints x grid values, not only the first dimension of an array).
    host_pre = "\n".join(ast.unparse(stmt) for stmt in body[:k])
    host = (f"def bound({ast.unparse(func.args)}):\n"
            + (textwrap.indent(host_pre, "    ") + "\n" if host_pre else "")
            + f"    return int({ast.unparse(bounds[-1])}) - int({ast.unparse(bounds[0]) if len(bounds) == 2 else '0'})\n")
    host_scope = dict(py.__globals__)
    exec(compile(host, f"<prange bound of {py.__name__}>", "exec"), host_scope)
    _BOUNDS[fn] = host_scope["bound"]
    func.body = body
    func = _ToCuda(py.__name__).run(func)
    ast.fix_missing_locations(func)
    pre = "\n".join(ast.unparse(stmt) for stmt in func.body[:k])
    loop = func.body[k]
    start = ast.unparse(loop.iter.args[0]) if len(loop.iter.args) == 2 else "0"
    stop = ast.unparse(loop.iter.args[-1])
    inner = "\n".join(ast.unparse(stmt) for stmt in loop.body)
    source = (f"def kernel({ast.unparse(func.args)}):\n"
              + (textwrap.indent(pre, "    ") + "\n" if pre else "")
              + f"    _first = cuda.grid(1) + ({start})\n"
              + f"    for {loop.target.id} in range(_first, min(_first + 1, {stop})):\n"
              + textwrap.indent(inner, "        ") + "\n")
    scope = _scope(py, func)
    exec(compile(source, f"<cuda twin of {py.__name__}>", "exec"), scope)
    _TWINS[fn] = cuda_module().jit(scope["kernel"])
    return _TWINS[fn]


_PAIRWISE_SOURCE = """
def leaf(a, start, n):
    if n < 8:
        res = 0.
        for i in range(n):
            res += a[start + i]
        return res
    r0 = a[start]
    r1 = a[start + 1]
    r2 = a[start + 2]
    r3 = a[start + 3]
    r4 = a[start + 4]
    r5 = a[start + 5]
    r6 = a[start + 6]
    r7 = a[start + 7]
    i = 8
    stop = n - (n % 8)
    while i < stop:
        r0 += a[start + i]
        r1 += a[start + i + 1]
        r2 += a[start + i + 2]
        r3 += a[start + i + 3]
        r4 += a[start + i + 4]
        r5 += a[start + i + 5]
        r6 += a[start + i + 6]
        r7 += a[start + i + 7]
        i += 8
    res = ((r0 + r1) + (r2 + r3)) + ((r4 + r5) + (r6 + r7))
    while i < n:
        res += a[start + i]
        i += 1
    return res


def pairwise(a, start, n):
    if n <= 128:
        return leaf(a, start, n)
    st_start = cuda.local.array(64, int64)
    st_n = cuda.local.array(64, int64)
    st_state = cuda.local.array(64, int64)
    st_left = cuda.local.array(64, float64)
    for j in range(64):
        st_state[j] = 0
        st_left[j] = 0.
    sp = 0
    st_start[0] = start
    st_n[0] = n
    ret = 0.
    while True:
        s0 = st_start[sp]
        n0 = st_n[sp]
        if n0 <= 128:
            ret = leaf(a, s0, n0)
            sp -= 1
            if sp < 0:
                return ret
            continue
        n2 = n0 // 2
        n2 -= n2 % 8
        state = st_state[sp]
        if state == 0:
            st_state[sp] = 1
            sp += 1
            st_start[sp] = s0
            st_n[sp] = n2
            st_state[sp] = 0
        elif state == 1:
            st_left[sp] = ret
            st_state[sp] = 2
            sp += 1
            st_start[sp] = s0 + n2
            st_n[sp] = n0 - n2
            st_state[sp] = 0
        else:
            ret = st_left[sp] + ret
            st_state[sp] = 0
            sp -= 1
            if sp < 0:
                return ret
    return ret
"""


def _cuda_pairwise_sum():
    """``pairwise_sum`` as a CUDA device function (numpy's pairwise order, the same additions)."""
    if "pairwise" not in _DEVICE:
        cuda = cuda_module()
        from numba import float64, int64
        scope = {"cuda": cuda, "float64": float64, "int64": int64}
        exec(_PAIRWISE_SOURCE, scope)
        scope["leaf"] = cuda.jit(device=True)(scope["leaf"])
        _DEVICE["pairwise"] = cuda.jit(device=True)(scope["pairwise"])
    return _DEVICE["pairwise"]


cuda_pairwise_sum = _cuda_pairwise_sum   # for hand-written kernels: pairwise(a, start, n) on the device


def launch(fn, *args):
    """Run a compiled per-series loop: ``fn(*args)`` on numpy arrays (unchanged), its CUDA twin when
    the arrays are CUDA tensors (one thread per series, torch's stream, written in place). Without
    GPU kernels the tensors make a round trip through ``fn`` on the CPU (said once on stderr)."""
    torch = sys.modules.get("torch")
    if torch is None or not any(torch.is_tensor(a) and a.is_cuda for a in args):
        return fn(*args)
    if not gpu_kernels():
        if not _TWIN_WARNED:
            _TWIN_WARNED.append(1)
            print(f"smithery.jit: compiled loops on the CPU ({gpu_kernels_reason()})", file=sys.stderr)
        host = [a.detach().cpu().numpy().copy() if torch.is_tensor(a) else a for a in args]
        result = fn(*host)
        for a, h in zip(args, host):
            if torch.is_tensor(a):
                a.copy_(torch.as_tensor(h, device=a.device))
        return result
    cuda = cuda_module()
    kernel = _twin(fn)
    stream = cuda.external_stream(torch.cuda.current_stream().cuda_stream)
    values, back = [], []
    for a in args:
        if torch.is_tensor(a):
            values.append(cuda.as_cuda_array(a.detach()))
        elif isinstance(a, np.ndarray):
            device = cuda.to_device(np.ascontiguousarray(a), stream=stream)
            values.append(device)
            back.append((device, a))
        else:
            values.append(a)
    try:
        rows = max(int(_BOUNDS[fn](*args)), 0)
    except Exception:  # noqa: BLE001 - a bound the host cannot evaluate: one thread per row
        rows = max((int(a.shape[0]) for a in args if getattr(a, "ndim", 0) >= 1), default=0)
    if rows:
        threads = 32
        kernel[(rows + threads - 1) // threads, threads, stream](*values)
    for device, a in back:
        if a.flags.writeable and a.flags.c_contiguous:
            device.copy_to_host(a, stream=stream)
    if back:
        stream.synchronize()


def note() -> str:
    """For result files: whether per-bar loops ran compiled."""
    if _numba is None:
        return "loops: Python (Numba not installed)"
    return f"loops: Numba {_numba.__version__} ({_numba.get_num_threads()} threads)"


@njit(parallel=False)
def _leaf_sum(a, start, n):
    """numpy's pairwise summation for n <= 128: plain loop below 8, eight accumulators above."""
    if n < 8:
        res = 0.
        for i in range(n):
            res += a[start + i]
        return res
    r0 = a[start]
    r1 = a[start + 1]
    r2 = a[start + 2]
    r3 = a[start + 3]
    r4 = a[start + 4]
    r5 = a[start + 5]
    r6 = a[start + 6]
    r7 = a[start + 7]
    i = 8
    stop = n - (n % 8)
    while i < stop:
        r0 += a[start + i]
        r1 += a[start + i + 1]
        r2 += a[start + i + 2]
        r3 += a[start + i + 3]
        r4 += a[start + i + 4]
        r5 += a[start + i + 5]
        r6 += a[start + i + 6]
        r7 += a[start + i + 7]
        i += 8
    res = ((r0 + r1) + (r2 + r3)) + ((r4 + r5) + (r6 + r7))
    while i < n:
        res += a[start + i]
        i += 1
    return res


@njit(parallel=False)
def pairwise_sum(a, start, n):
    """numpy's pairwise summation of a[start:start+n] (umath loops), for compiled loops that must
    reproduce ``x.sum()`` / ``x.mean()`` bit for bit. Above 128 numpy splits at n//2 rounded down
    to a multiple of 8 and adds left + right; walked here with an explicit stack (Numba's on-disk
    cache cannot link a recursive function called from other cached functions)."""
    if n <= 128:
        return _leaf_sum(a, start, n)
    st_start = np.empty(64, dtype=np.int64)
    st_n = np.empty(64, dtype=np.int64)
    st_state = np.zeros(64, dtype=np.int64)
    st_left = np.zeros(64)
    sp = 0
    st_start[0] = start
    st_n[0] = n
    ret = 0.
    while True:
        s0 = st_start[sp]
        n0 = st_n[sp]
        if n0 <= 128:
            ret = _leaf_sum(a, s0, n0)
            sp -= 1
            if sp < 0:
                return ret
            continue
        n2 = n0 // 2
        n2 -= n2 % 8
        state = st_state[sp]
        if state == 0:                      # evaluate the left half
            st_state[sp] = 1
            sp += 1
            st_start[sp] = s0
            st_n[sp] = n2
            st_state[sp] = 0
        elif state == 1:                    # left done: keep it, evaluate the right half
            st_left[sp] = ret
            st_state[sp] = 2
            sp += 1
            st_start[sp] = s0 + n2
            st_n[sp] = n0 - n2
            st_state[sp] = 0
        else:                               # right done: left + right
            ret = st_left[sp] + ret
            st_state[sp] = 0
            sp -= 1
            if sp < 0:
                return ret
