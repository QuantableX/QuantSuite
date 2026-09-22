"""The forge's command line for the suite — what QuantAlgo's Smithery page
runs (PLAN-QUANTALGO §6). Every long job prints one JSON object per line on
stdout (an ``event`` field names it); the short queries print one JSON
document. Nothing here changes what the vault scripts do: reports and the
ledger still land in the vault (``smithery.data.ROOT``).

    python -m smithery.forge info                          # vault, shelf, reports, roster
    python -m smithery.forge source --indicator hilbert    # an indicator's module
    python -m smithery.forge gauntlet --indicator dc page [--timeframe all|1d|4h|1h|1m] [--fast] [--perm N --boot N --garch N --seed N]
    python -m smithery.forge walkforward --indicator mk [--timeframe 1d|4h|1h|1m] [--folds N]
    python -m smithery.forge refresh [--timeframe all|1d|4h|1h|1m]  # the shelf, from the exchanges

Job events:  job · begin · contract · axis · verdict · fold · walkforward ·
series · log · error · done.
"""
from __future__ import annotations

import argparse
import datetime as dt
import inspect
import json
import os
import re
import sys
import time
import traceback
from pathlib import Path
from .robustness import TIMEFRAMES


def emit(event: str, **fields) -> None:
    from .evidence import finite_json
    line = json.dumps(finite_json({"event": event, **fields}), default=str, allow_nan=False)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()


# ---------------------------------------------------------------- info

def _csv_summary(path: Path) -> dict:
    """Bars and date span of a shelf CSV without pandas: the header, the first
    data line and the last line. Timestamps are the first column."""
    bars, first, last = 0, None, None
    try:
        with path.open("rb") as fh:
            header = fh.readline()
            first_line = fh.readline()
            if first_line:
                bars = 1
                first = first_line.split(b",", 1)[0].decode("utf-8", "replace")
                # count the remaining newlines and grab the final line
                fh.seek(0, os.SEEK_END)
                size = fh.tell()
                pos = len(header) + len(first_line)
                fh.seek(pos)
                tail = b""
                chunk = 1 << 16
                while pos < size:
                    block = fh.read(chunk)
                    if not block:
                        break
                    bars += block.count(b"\n")
                    pos += len(block)
                    tail = (tail + block)[-4096:]
                if not tail.endswith(b"\n"):
                    bars += 1          # a last line without a newline
                lines = [ln for ln in tail.split(b"\n") if ln.strip()]
                if lines:
                    last = lines[-1].split(b",", 1)[0].decode("utf-8", "replace")
                if first_line.endswith(b"\n") is False and bars == 1:
                    last = first
    except OSError:
        pass
    return {"bars": bars, "first": (first or "")[:10] or None, "last": (last or "")[:10] or None}


def shelf_listing() -> list[dict]:
    from .data import PRICE_DIR

    rows = []
    if not PRICE_DIR.is_dir():
        return rows
    for path in sorted(PRICE_DIR.glob("*.csv")):
        parts = path.stem.split("_")
        exchange = parts[0] if parts else ""
        symbol = parts[1] if len(parts) > 1 else ""
        timeframe = parts[2] if len(parts) > 2 else ""
        stat = path.stat()
        rows.append({
            "key": path.stem,
            "exchange": exchange,
            "symbol": symbol,
            "timeframe": timeframe,
            "file": str(path),
            "size_bytes": stat.st_size,
            "modified": dt.datetime.fromtimestamp(stat.st_mtime, dt.timezone.utc).isoformat(timespec="seconds"),
            **_csv_summary(path),
        })
    return rows


# `2026-09-08 PageTrend Gauntlet`, `… Gauntlet (4h)`, `… Gauntlet (1h, fast)`
_REPORT_NAME = re.compile(r"^(\d{4}-\d{2}-\d{2}) (.+?) Gauntlet(?: \(([^)]*)\))?$")
_TRACKS = TIMEFRAMES
_SCORE = re.compile(r"Robustness Score: \*\*(\d+(?:\.\d+)?)/100\*\* — Grade \*\*([^*]+)\*\*")
_PERM = re.compile(r"permutation test \| \d+ \| p = ([0-9.]+)")
_RUNTIME = re.compile(r"\*\*runtime:\*\* (\d+)s")
_PARAMS = re.compile(r"\*\*params:\*\* `([^`]*)`")


def _report_meta(path: Path) -> dict:
    name = path.stem
    m = _REPORT_NAME.match(name)
    comparison = " Smithery Comparison (" in name
    tags = [t.strip() for t in (m.group(3) or "").split(",") if t.strip()] if m else []
    meta = {
        "name": name,
        "file": str(path),
        "date": m.group(1) if m else name[:10] if comparison else None,
        "indicator": m.group(2) if m else "Smithery Comparison" if comparison else None,
        "timeframe": "all" if comparison else next((t for t in tags if t in _TRACKS), "1d"),
        "fast": "fast" in tags,
        "score": None, "grade": None, "perm_p": None, "runtime_s": None, "params": None,
        "certified": None,
    }
    try:
        stat = path.stat()
        meta["modified"] = dt.datetime.fromtimestamp(stat.st_mtime, dt.timezone.utc).isoformat(timespec="seconds")
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return meta
    if comparison:
        try:
            manifest = json.loads(path.with_suffix(".manifest.json").read_text(encoding="utf-8"))
            tracks = manifest.get("timeframes", [])
            if len(tracks) == 1 and tracks[0] in _TRACKS:
                meta["timeframe"] = tracks[0]
        except (OSError, ValueError, TypeError, AttributeError):
            pass  # older reports without a manifest retain the all-track label
    if sm := _SCORE.search(text):
        meta["score"] = float(sm.group(1))
        meta["grade"] = sm.group(2).strip()
    if pm := _PERM.search(text):
        meta["perm_p"] = float(pm.group(1))
    if rm := _RUNTIME.search(text):
        meta["runtime_s"] = int(rm.group(1))
    if pp := _PARAMS.search(text):
        meta["params"] = pp.group(1)
    meta["certified"] = "Battle-Ready" in text
    return meta


def reports_listing() -> list[dict]:
    from .data import OUTPUT_DIR

    if not OUTPUT_DIR.is_dir():
        return []
    rows = [_report_meta(p) for p in OUTPUT_DIR.glob("*.md")]
    rows.sort(key=lambda r: (r.get("modified") or "", r["name"]), reverse=True)
    return rows


def info() -> dict:
    from . import __version__
    from .data import DEFAULT_VAULT, DOCS_DIR, OUTPUT_DIR, PRICE_DIR, ROOT
    from .indicators import REGISTRY
    from .registry import TIMEFRAMES, certified_keys
    from .robustness import available_timeframes

    root = Path(ROOT)
    env = os.environ.get("SMITHERY_VAULT")
    if env:
        location = "env"
    elif root == DEFAULT_VAULT:
        location = "vault"
    else:
        location = "private"
    return {
        "version": __version__,
        "python": sys.version.split()[0],
        "package_dir": str(Path(__file__).resolve().parent),
        "root": str(root),
        "root_exists": root.is_dir(),
        "location": location,       # env: $SMITHERY_VAULT · vault: the Obsidian vault · private: ~/.quantsuite/modules/algo/smithery
        "price_dir": str(PRICE_DIR),
        "output_dir": str(OUTPUT_DIR),
        "docs_dir": str(DOCS_DIR),
        "ledger": str(DOCS_DIR / "04 Indicator Registry.md"),
        "indicators": list(REGISTRY),
        # certified on EVERY track — the overall verdict, the roster's one score
        "certified": certified_keys(),
        # the certification tracks the shelf can run, and who is certified on each
        "timeframes": available_timeframes(),
        "supported_timeframes": list(TIMEFRAMES),
        "certified_by_timeframe": {tf: certified_keys(tf) for tf in TIMEFRAMES},
        "workers": _workers(),
        "shelf": shelf_listing(),
        "reports": reports_listing(),
    }


def _workers() -> int:
    from .parallel import WORKERS
    return int(WORKERS)


def source(indicator: str) -> dict:
    from .indicators import REGISTRY

    cls = REGISTRY.get(indicator)
    if cls is None:
        raise KeyError(f"unknown indicator '{indicator}' — known: {sorted(REGISTRY)}")
    module = sys.modules[cls.__module__]
    file = inspect.getsourcefile(module) or getattr(module, "__file__", "")
    return {
        "key": indicator,
        "name": cls.name,
        "module": cls.__module__,
        "file": str(file),
        "source": Path(file).read_text(encoding="utf-8") if file else inspect.getsource(module),
    }


# ---------------------------------------------------------------- jobs

def _keys(args_keys: list[str], timeframe: str = "all") -> list[str]:
    from .indicators import REGISTRY
    from .registry import certified_keys

    keys: list[str] = []
    for k in args_keys:
        if k == "all":
            keys.extend(REGISTRY)
        elif k == "certified":
            keys.extend(certified_keys(timeframe))
        elif k in REGISTRY:
            keys.append(k)
        else:
            raise KeyError(f"unknown indicator '{k}' — known: {sorted(REGISTRY)}")
    seen: set[str] = set()
    return [k for k in keys if not (k in seen or seen.add(k))]


def run_comparison(args) -> int:
    from .research import compare
    from .registry import TIMEFRAMES
    keys = _keys(args.indicator, args.timeframe)
    tracks = list(TIMEFRAMES) if args.timeframe == "all" else [args.timeframe]
    emit("job", kind="compare", indicators=keys, timeframe=args.timeframe, tracks=tracks)
    t0 = time.time()
    result = compare(keys, timeframes=tracks, n_folds=args.folds,
                     progress=lambda payload: emit("comparison", **payload))
    emit("comparison_done", winner=result["winner"], passed=result["passed"],
         reasons=result["reasons"], report=result["report"], report_name=result["report_name"])
    emit("done", ok=True, failures=0, elapsed_s=round(time.time() - t0, 1))
    return 0


def run_gauntlet(args) -> int:
    from .data import ROOT
    from .indicators import REGISTRY
    from .registry import CERTIFY_PERM_P, CERTIFY_SCORE, TIMEFRAMES
    from .robustness import Gauntlet

    keys = _keys(args.indicator, args.timeframe)
    # `all` runs every track for each indicator in turn — THE score is earned
    # on every track (registry.overall_verdict), so that is the default run.
    tracks = list(TIMEFRAMES) if args.timeframe == "all" else [args.timeframe]
    counts = {"perm": 25, "boot": 40, "garch": 20} if args.fast else \
             {"perm": args.perm, "boot": args.boot, "garch": args.garch}
    emit("job", kind="gauntlet", indicators=keys, fast=bool(args.fast), seed=args.seed,
         timeframe=args.timeframe, tracks=tracks, vault=str(ROOT), **counts)
    failures = 0
    t_job = time.time()
    for key in keys:
        for timeframe in tracks:
            ind = REGISTRY[key]()
            emit("begin", indicator=key, name=ind.name, params=ind.params, timeframe=timeframe)
            t0 = time.time()

            def progress(payload: dict, _key=key, _tf=timeframe) -> None:
                emit(payload.pop("stage", "progress"), indicator=_key, timeframe=_tf, **payload)

            try:
                res = Gauntlet(ind, seed=args.seed, perm_n=args.perm, boot_n=args.boot,
                               garch_n=args.garch, fast=args.fast, progress=progress,
                               timeframe=timeframe).run()
                report = Path(res["report"])
                perm_p = res["perm_p"]
                # A fast run never certifies — reduced MC counts are a smoke test.
                certified = res["certified"]
                emit("verdict", indicator=key, name=ind.name, score=round(float(res["score"]), 1),
                     grade=res["grade"], perm_p=perm_p, timeframe=timeframe,
                     scores={k: (None if v != v else round(float(v), 1)) for k, v in res["scores"].items()},
                     certified=bool(certified), fast=bool(args.fast), report=str(report),
                     reasons=res["reasons"],
                     report_name=report.stem, elapsed_s=round(time.time() - t0, 1))
            except Exception as e:  # noqa: BLE001 — the next run still happens
                failures += 1
                emit("error", indicator=key, timeframe=timeframe, message=f"{type(e).__name__}: {e}",
                     traceback=traceback.format_exc())
    emit("done", ok=failures == 0, failures=failures, elapsed_s=round(time.time() - t_job, 1))
    return 0 if failures == 0 else 1


def run_walkforward(args) -> int:
    from .backtest import DEFAULT_COST_BPS
    from .data import ROOT, asset_class, load
    from .indicators import REGISTRY
    from .optimize import walk_forward
    from .robustness import find_primary

    keys = _keys(args.indicator, args.timeframe)
    series = args.series or find_primary(args.timeframe)
    df = load(series)
    cost = DEFAULT_COST_BPS[asset_class(series)]
    emit("job", kind="walkforward", indicators=keys, series=series, bars=len(df), folds=args.folds,
         timeframe=args.timeframe, vault=str(ROOT))
    failures = 0
    t_job = time.time()
    for key in keys:
        ind = REGISTRY[key]()
        emit("begin", indicator=key, name=ind.name, params=ind.params)
        t0 = time.time()
        try:
            from .variants import create_variant, source_signature
            source_before = source_signature()
            res = walk_forward(ind, df, cost, n_folds=args.folds)
            from .evidence import frame_fingerprint
            variant = create_variant(key, res["final_choice"], {
                "kind": "walkforward", "series": series, "timeframe": args.timeframe,
                "cost_bps": cost, "data_sha256": frame_fingerprint(df), "result": res,
                "note": "Fold parameters were tested chronologically. The final fixed parameters are a research candidate, not the stitched OOS strategy.",
            }, expected_source=source_before)
            for f in res["folds"]:
                emit("fold", indicator=key, **f)
            emit("walkforward", indicator=key, name=ind.name, series=series, timeframe=args.timeframe,
                 oos_sharpe=round(float(res["oos_sharpe"]), 3),
                 is_sharpe_mean=round(float(res["is_sharpe_mean"]), 3),
                 wfe=(None if res["wfe"] != res["wfe"] else round(float(res["wfe"]), 3)),
                 oos_total_log_ret=round(float(res["oos_total_log_ret"]), 4),
                 final_choice=res["final_choice"], defaults=ind.params, variant=variant,
                 oos_metrics=res.get("oos_metrics"), objective=res.get("objective"),
                 elapsed_s=round(time.time() - t0, 1))
        except Exception as e:  # noqa: BLE001
            failures += 1
            emit("error", indicator=key, message=f"{type(e).__name__}: {e}",
                 traceback=traceback.format_exc())
    emit("done", ok=failures == 0, failures=failures, elapsed_s=round(time.time() - t_job, 1))
    return 0 if failures == 0 else 1


def run_refresh(args) -> int:
    from .data import ROOT, SHELF_CRYPTO, refresh

    timeframe = args.timeframe
    emit("job", kind="refresh", timeframe=timeframe,
         series=[f"{ex.upper()} {sym} {tf}" for ex, sym, tf, _ in SHELF_CRYPTO if timeframe == "all" or tf == timeframe],
         vault=str(ROOT))
    t0 = time.time()

    def log(message: str) -> None:
        emit("log", message=message.strip())

    try:
        results = refresh(verbose=False, log=log, timeframe=timeframe)
    except Exception as e:  # noqa: BLE001
        emit("error", message=f"{type(e).__name__}: {e}", traceback=traceback.format_exc())
        emit("done", ok=False, failures=1, elapsed_s=round(time.time() - t0, 1))
        return 1
    failed = 0
    for key, status in results.items():
        ok = status.startswith("ok")
        failed += 0 if ok else 1
        emit("series", key=key, ok=ok, status=status)
    emit("done", ok=failed == 0, failures=failed, elapsed_s=round(time.time() - t0, 1),
         shelf=shelf_listing())
    return 0 if failed == 0 else 1


# ---------------------------------------------------------------- main

def _main(argv: list[str]) -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(prog="python -m smithery.forge")
    sub = ap.add_subparsers(dest="command", required=True)

    sub.add_parser("info", help="vault, shelf, reports and roster as one JSON document")

    p = sub.add_parser("source", help="an indicator's module source")
    p.add_argument("--indicator", required=True)

    p = sub.add_parser("gauntlet", help="the five-axis robustness gauntlet (JSON lines)")
    p.add_argument("--indicator", nargs="+", required=True,
                   help="registry keys, or 'all' / 'certified'")
    p.add_argument("--timeframe", default="all", choices=("all", *TIMEFRAMES),
                   help="certification track, or 'all' for every track in turn (the default — "
                        "the score is earned on every track)")
    p.add_argument("--fast", action="store_true", help="reduced MC counts (smoke test)")
    p.add_argument("--perm", type=int, default=120)
    p.add_argument("--boot", type=int, default=200)
    p.add_argument("--garch", type=int, default=100)
    p.add_argument("--seed", type=int, default=42)

    p = sub.add_parser("walkforward", help="anchored walk-forward re-selection (JSON lines)")
    p.add_argument("--indicator", nargs="+", required=True)
    p.add_argument("--timeframe", default="1d", choices=TIMEFRAMES,
                   help="track whose primary series to re-select on")
    p.add_argument("--series", default=None, help="shelf key (default: the track's primary BTC series)")
    p.add_argument("--folds", type=int, default=4)

    p = sub.add_parser("compare", help="frozen candidates, cost stress, final holdout and LCES ratio transfer")
    p.add_argument("--indicator", nargs="+", required=True)
    p.add_argument("--timeframe", default="all", choices=("all", *TIMEFRAMES))
    p.add_argument("--folds", type=int, default=3)

    p = sub.add_parser("refresh", help="fetch / refresh the price shelf (JSON lines)")
    p.add_argument("--timeframe", default="all", choices=("all", *TIMEFRAMES))

    args = ap.parse_args(argv)
    try:
        if args.command == "info":
            print(json.dumps(info()))
            return 0
        if args.command == "source":
            print(json.dumps(source(args.indicator)))
            return 0
        if args.command == "gauntlet":
            return run_gauntlet(args)
        if args.command == "walkforward":
            return run_walkforward(args)
        if args.command == "compare":
            return run_comparison(args)
        if args.command == "refresh":
            return run_refresh(args)
    except Exception as e:  # noqa: BLE001 — one line the caller can show
        emit("error", message=f"{type(e).__name__}: {e}", traceback=traceback.format_exc())
        return 2
    return 2


# The gauntlet's process pool re-imports this module in every worker.
if __name__ == "__main__":
    sys.exit(_main(sys.argv[1:]))
