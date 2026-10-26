"""Private, durable research runs shared by the desktop UI and MCP.

This module owns orchestration and storage only. Certification is always the
native Forge; system profitability is always the rotation engine.
"""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import uuid
from contextlib import contextmanager
from pathlib import Path

PROTOCOL = dict(seed=42, perm=120, boot=200, garch=100, folds=4,
                timeframes=["1h", "4h", "1d"], fast=False)
# gauntlet: new candidates through the native Gauntlet; gauntlet_overview: the
# library's existing evidence as one report; profit: the most profitable singles and
# aggregates in a saved system; aggregates: a variable-size search against its aggregate.
KINDS = ("gauntlet", "gauntlet_overview", "profit", "aggregates")


def root() -> Path:
    return Path(os.environ.get("QUANTSUITE_HOME", Path.home() / ".quantsuite")) / "modules/script/research"


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=True, allow_nan=False, default=str), encoding="utf-8")
    tmp.replace(path)


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def folder(run_id):
    if not isinstance(run_id, str) or not re.fullmatch(r"[a-f0-9]{32}", run_id):
        raise ValueError("Invalid research run id")
    return root() / run_id


@contextmanager
def locked(path):
    lock = path / ".lock"
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        raise ValueError("This research run is busy") from None
    try:
        yield
    finally:
        os.close(fd)
        lock.unlink()


def alive(pid):
    if not pid:
        return False
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        handle = kernel.OpenProcess(0x1000, False, int(pid))
        if not handle:
            return False
        code = wintypes.DWORD()
        try:
            return bool(kernel.GetExitCodeProcess(handle, ctypes.byref(code)) and code.value == 259)
        finally:
            kernel.CloseHandle(handle)
    try:
        os.kill(int(pid), 0)
        return True
    except OSError:
        return False


def summary(path):
    doc = read(path / "run.json")
    if doc["status"] == "running" and not alive(doc.get("pid")):
        doc = {**doc, "status": "interrupted", "error": "Worker stopped; partial evidence is preserved."}
    return doc


def list_runs():
    runs = []
    for path in root().glob("*/run.json"):
        try:
            runs.append(summary(path.parent))
        except (OSError, ValueError):
            continue
    return sorted(runs, key=lambda d: d["created_at"], reverse=True)


def keys_checked(keys):
    if not isinstance(keys, list) or len(keys) > 1000 or any(
            not isinstance(k, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,100}", k) for k in keys):
        raise ValueError("Indicators must be a list of registry keys (maximum 1000)")
    return list(dict.fromkeys(keys))


def create(request):
    kind = request.get("kind", "gauntlet")
    if kind not in KINDS:
        raise ValueError("Research kind must be one of: " + ", ".join(KINDS))
    keys = keys_checked(request.get("indicators", []))
    from .workspace import indicators_dir
    path = folder(uuid.uuid4().hex)
    path.mkdir(parents=True)
    try:
        shutil.copytree(indicators_dir(), path / "library", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        doc = dict(id=path.name, title=str(request.get("title") or "Indicator research")[:160], kind=kind,
                   status="draft", created_at=now(), indicators=keys, protocol=PROTOCOL, candidates=[], path=str(path))
        if kind in ("profit", "aggregates"):
            # A saved system by name or id, explicit closed dates; empty indicators = the default set.
            from .lab_compare import validate_request
            from .lab_profit import prepare
            doc.update(prepare(request, {"budget": 200_000} if kind == "aggregates" else None))
            validate_request(doc)
        save(path / "run.json", doc)
        return doc
    except Exception as exc:
        # Leave failed preparation visible, retaining the private input copy.
        save(path / "run.json", dict(id=path.name, title="Preparation failed", kind=kind,
             status="failed", created_at=now(), indicators=keys, error=str(exc)[:500] or "Invalid research configuration"))
        raise


def env_for(path, frozen=False):
    env = {**os.environ, "QUANTSCRIPT_INDICATORS_DIR": str(path / "library"),
           "PYTHONIOENCODING": "utf-8", "PYTHONDONTWRITEBYTECODE": "1"}
    if frozen:
        env.update(PYTHONPATH=str(path / "engine"), SMITHERY_VAULT=str(path / "vault"),
                   ROTATION_LAB_CACHE=str(path / "cache.sqlite"), QUANTSUITE_HOME=str(path / "suite"))
    return env


def python_json(path, args, frozen=False):
    proc = subprocess.run([sys.executable, "-m", *args], env=env_for(path, frozen),
                          cwd=path / "engine" if frozen else Path(__file__).resolve().parents[1],
                          capture_output=True, text=True, encoding="utf-8", timeout=600)
    if proc.returncode:
        raise ValueError(proc.stderr[-3000:] or proc.stdout[-3000:] or "Python query failed")
    for line in reversed(proc.stdout.splitlines()):
        try:
            return json.loads(line)
        except ValueError:
            pass
    raise ValueError("Python query returned no JSON")


def candidate(run_id, file, content, keys):
    path = folder(run_id)
    if not isinstance(file, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,60}\.py", file):
        raise ValueError("Use a single new Python file name")
    if not isinstance(content, str) or len(content.encode()) > 1_000_000:
        raise ValueError("Candidate source must be text, at most 1 MB")
    keys = keys_checked(keys)
    if not keys:
        raise ValueError("Declare the candidate's registered keys")
    with locked(path):
        doc = read(path / "run.json")
        if doc["status"] != "draft":
            raise ValueError("Only drafts accept candidates; create a new run after evaluation")
        if (path / "library" / file).exists() and file not in doc["candidates"]:
            raise ValueError("Existing library scripts are frozen; use a new file and new keys")
        staging = path / "candidates" / (uuid.uuid4().hex + ".py")
        staging.parent.mkdir(exist_ok=True)
        staging.write_text(content, encoding="utf-8")
        check = python_json(path, ["smithery.quantscript", "check", "--file", file,
                                  "--candidate", str(staging), "--depth", "full"])
        save(staging.with_suffix(".check.json"), check)
        if not check.get("ok"):
            return {"saved": False, "check": check}
        # Check runs in the native sandbox, including registration collisions.
        old = path / "library" / file
        previous = old.read_bytes() if old.exists() else None
        old.write_text(content, encoding="utf-8")
        try:
            registry = python_json(path, ["smithery.registry", "--json"])
            actual = {r["key"] for r in registry["indicators"] if r.get("script") == file and r.get("role") == "standard"}
            if actual != set(keys):
                raise ValueError(f"Declared keys differ from registered keys: {sorted(actual)}")
        except Exception:
            old.write_bytes(previous) if previous is not None else old.unlink()
            raise
        doc["indicators"] = list(dict.fromkeys(doc["indicators"] + keys))
        doc["candidates"] = list(dict.fromkeys(doc["candidates"] + [file]))
        save(path / "run.json", doc)
        return {"saved": True, "run": doc, "check": check}


def import_report(source, title=None):
    source = Path(source).expanduser().resolve()
    if source.suffix.lower() not in (".html", ".htm") or source.stat().st_size > 30_000_000:
        raise ValueError("Import a local HTML report of at most 30 MB")
    data = source.read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    for doc in list_runs():
        if doc.get("source_sha256") == sha:
            return doc
    path = folder(uuid.uuid4().hex)
    path.mkdir(parents=True)
    (path / "original.html").write_bytes(data)
    text = data.decode("utf-8-sig")
    # Keep linked CSV exports usable inside the app's offline sandbox.
    def embedded_csv(match):
        name = match[2]
        if Path(name).name != name or not name.lower().endswith(".csv"):
            return match[0]
        csv = source.parent / name
        if not csv.is_file() or csv.stat().st_size > 10_000_000:
            return match[0]
        encoded = base64.b64encode(csv.read_bytes()).decode("ascii")
        return f'{match[1]}data:text/csv;base64,{encoded}{match[3]}'
    rendered = re.sub(r'(href=["\x27])([^"\x27]+)(["\x27])', embedded_csv, text)
    (path / "report.html").write_text(rendered, encoding="utf-8")
    match = re.search(r"<title[^>]*>(.*?)</title>", text, re.I | re.S)
    doc = dict(id=path.name, title=str(title or (match[1] if match else source.stem))[:160],
               kind="import", status="imported", created_at=now(), source=str(source),
               source_sha256=sha, indicators=[], path=str(path),
               provenance="Historical report; imported without rerunning or recertifying.")
    # Parse embedded JSON without executing report JavaScript.
    for match in re.finditer(r"\bconst\s+(?:DATA|D)\s*=\s*", text):
        try:
            payload, _ = json.JSONDecoder().raw_decode(text[match.end():])
            save(path / "legacy-data.json", payload)
            doc["structured_data"] = True
            break
        except ValueError:
            pass
    save(path / "run.json", doc)
    return doc


def compacted(value, depth=0):
    """Summarize long nested arrays and very wide objects so an agent's read stays small."""
    if isinstance(value, list):
        if depth and len(value) > 50:
            return {"omitted_items": len(value), "first": compacted(value[0], depth + 1),
                    "last": compacted(value[-1], depth + 1)}
        return [compacted(v, depth + 1) for v in value]
    if isinstance(value, dict):
        if len(value) > 100:
            return {"omitted_keys": len(value), "sample_keys": list(value)[:10]}
        return {k: compacted(v, depth + 1) for k, v in value.items()}
    return value


def details(run_id, section="results", offset=0, limit=100, field=None, compact=True):
    path = folder(run_id)
    doc = summary(path)
    offset, limit = max(0, int(offset)), max(1, min(200, int(limit)))
    if section == "html":
        return {"run": doc, "html": (path / "report.html").read_text(encoding="utf-8-sig")}
    if section == "legacy":
        # Imported payloads can be megabytes: page through one list, describe the rest.
        data = read(path / "legacy-data.json")
        if not isinstance(data, dict):
            data = {"items": data}
        lists = {k: v for k, v in data.items() if isinstance(v, list)}
        if field is None and lists:
            field = max(lists, key=lambda k: (any(isinstance(v, dict) for v in lists[k][:1]), len(lists[k])))
        if field is not None and field not in lists:
            raise ValueError(f"Unknown list field {field}; available: {sorted(lists)}")
        rows = lists.get(field, [])[offset:offset + limit]
        return {"run": doc, "field": field, "total": len(lists.get(field, [])),
                "rows": [compacted(r, 1) for r in rows] if compact else rows,
                "fields": {k: (f"list of {len(v)}" if isinstance(v, list) else
                               f"object with {len(v)} keys" if isinstance(v, dict) else v)
                           for k, v in data.items()}}
    if section == "events":
        lines = (path / "events.jsonl").read_text(encoding="utf-8").splitlines() if (path / "events.jsonl").exists() else []
        return {"run": doc, "events": [json.loads(s) for s in lines[-100:]]}
    result = read(path / "results.json") if (path / "results.json").exists() else {"rows": []}
    rows = result.get("rows", [])[offset:offset + limit]
    return {"run": doc, **result, "total": len(result.get("rows", [])),
            "rows": [compacted(r, 1) for r in rows] if compact else rows}


def freeze(path, kind="gauntlet"):
    from .data import PRICE_DIR
    engine = Path(__file__).resolve().parents[1]
    for name in ("smithery", "rotation_lab"):
        shutil.copytree(engine / name, path / "engine" / name,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    if not PRICE_DIR.is_dir() or not list(PRICE_DIR.glob("*.csv")):
        raise ValueError("No Forge price shelf. Refresh Market data first.")
    shutil.copytree(PRICE_DIR, path / "vault/Price History")
    (path / "vault/Output").mkdir()
    (path / "vault/Docs").mkdir()
    # Only system comparisons read the systems price cache (~200 MB); a Gauntlet uses the shelf.
    cache = Path(os.environ.get("ROTATION_LAB_CACHE", root().parents[1] / "systems/cache/rotation_lab.sqlite"))
    if kind == "profit" and cache.is_file():
        with sqlite3.connect(cache.as_uri() + "?mode=ro", uri=True) as src, sqlite3.connect(path / "cache.sqlite") as dst:
            src.backup(dst)
    files = [f for directory in ("engine", "library", "vault/Price History")
             for f in (path / directory).rglob("*") if f.is_file()]
    if (path / "cache.sqlite").exists():
        files.append(path / "cache.sqlite")
    save(path / "inputs.json", {str(f.relative_to(path)): digest(f) for f in files})


def emit(path, event, **values):
    item = {"event": event, "at": now(), **values}
    with (path / "events.jsonl").open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(item, default=str, allow_nan=False) + "\n")
    print(json.dumps(item, default=str, allow_nan=False), flush=True)


def run(run_id):
    path = folder(run_id)
    with locked(path):
        doc = read(path / "run.json")
        if doc["status"] != "draft" or (not doc["indicators"] and doc["kind"] == "gauntlet"):
            raise ValueError("Choose indicators in a fresh draft before running")
        doc.update(status="running", pid=os.getpid(), started_at=now())
        save(path / "run.json", doc)
    try:
        # The overview reads existing evidence only: the library snapshot taken
        # at creation and the vault's reports; nothing to freeze or re-run.
        frozen = doc["kind"] != "gauntlet_overview"
        if frozen:
            emit(path, "progress", message="Freezing engine, private library and market inputs")
            freeze(path, doc["kind"])
        command = [sys.executable, "-u", "-m", "smithery.lab", "worker", "--path", str(path)]
        # Parent stays in the Forge process tree, so native cancellation kills every child.
        with (path / "worker.stderr.log").open("w", encoding="utf-8") as errors:
            with subprocess.Popen(command, cwd=path / "engine" if frozen else Path(__file__).resolve().parents[1],
                                  env=env_for(path, frozen),
                                  stdout=subprocess.PIPE, stderr=errors, text=True, encoding="utf-8") as proc:
                for line in proc.stdout:
                    print(line.rstrip(), flush=True)
                code = proc.wait()
        if code:
            raise ValueError((path / "worker.stderr.log").read_text(encoding="utf-8")[-3000:] or f"Research worker exited {code}")
        doc.update(status="done", finished_at=now())
        emit(path, "done", run_id=run_id)
    except Exception as exc:
        doc.update(status="failed", error=str(exc), finished_at=now())
        emit(path, "error", message=str(exc), run_id=run_id)
    save(path / "run.json", doc)
    if (path / "results.json").exists():
        from .lab_report import render
        (path / "report.html").write_text(render(doc, read(path / "results.json")), encoding="utf-8")
        evidence = [path / "results.json", *(path / "vault/Output").glob("*"),
                    *(path / "library/versions").rglob("*.json"), *(path / "data").glob("*"),
                    path / "plan.json", path / "verification.json"]
        if doc["kind"] == "gauntlet_overview":
            evidence += list((path / "library").glob("*.py"))
        save(path / "evidence-hashes.json", {str(f.relative_to(path)): digest(f) for f in evidence if f.is_file()})
    return 0 if doc["status"] == "done" else 2


def gauntlet_worker(path, doc):
    from .indicators import REGISTRY, VARIANTS
    from .contract import validate_causality, validate_scale_invariance
    from .data import available, load
    from .registry import describe_all
    checks = []
    for key in doc["indicators"]:
        if key not in REGISTRY:
            raise ValueError(f"Unknown indicator {key}")
        if key in VARIANTS:
            raise ValueError(f"Use the base indicator for optimization, not the version {key}")
        for series in available():
            frame = load(series)
            for name, validator in (("causality", validate_causality), ("scale", validate_scale_invariance)):
                ok, message = validator(REGISTRY[key](), frame)
                checks.append(dict(indicator=key, series=series, check=name, ok=bool(ok), message=message))
            save(path / "historical-checks.json", checks)
        emit(path, "progress", indicator=key, message="Historical validators complete")
    if not checks or not all(c["ok"] for c in checks):
        raise ValueError("Historical contract validation failed; evidence retained")
    args = [sys.executable, "-u", "-m", "smithery.forge", "optimize", "--indicator", *doc["indicators"],
            "--timeframes", "1h", "4h", "1d", "--seed", "42", "--perm", "120", "--boot", "200",
            "--garch", "100", "--folds", "4"]
    with (path / "native.jsonl").open("w", encoding="utf-8") as log:
        proc = subprocess.Popen(args, stdout=subprocess.PIPE, text=True, encoding="utf-8")
        for line in proc.stdout:
            log.write(line)
            log.flush()
            print(line.rstrip(), flush=True)
        code = proc.wait()
    # A fresh interpreter loads versions written by optimize.
    registry = python_json(path, ["smithery.registry", "--json"], frozen=True)
    rows = [r for r in registry["indicators"] if r["base_key"] in doc["indicators"]]
    from .data import OUTPUT_DIR
    for row in rows:
        row["evidence"] = {}
        for tf, verdict in row["timeframes"].items():
            name = (verdict or {}).get("report")
            if name and Path(name).name == name and (OUTPUT_DIR / (name + ".json")).is_file():
                row["evidence"][tf] = read(OUTPUT_DIR / (name + ".json"))
    save(path / "results.json", {"rows": rows, "protocol": PROTOCOL, "kind": "gauntlet"})
    for key in doc["indicators"]:
        standard = next((r for r in rows if r["key"] == key), {})
        for tf in PROTOCOL["timeframes"]:
            evidence = standard.get("evidence", {}).get(tf, {})
            if evidence.get("fast") is not False or evidence.get("counts") != {"perm": 120, "boot": 200, "garch": 100}:
                raise ValueError(f"Missing full native Gauntlet evidence for {key} on {tf}")
    if code:
        raise ValueError(f"Native Forge returned {code}; partial results retained")


def gauntlet_overview_worker(path, doc):
    from .data import OUTPUT_DIR
    from .lab_gauntlet import overview
    emit(path, "progress", message="Reading the library snapshot's registry")
    registry = python_json(path, ["smithery.registry", "--json"])
    emit(path, "progress", message=f"Compiling native Gauntlet evidence from {OUTPUT_DIR}")
    results = overview(registry, OUTPUT_DIR, doc["indicators"] or None)
    results["vault"] = str(OUTPUT_DIR)
    save(path / "results.json", results)
    # The reports live in the shared vault: record exactly what was read.
    save(path / "report-hashes.json", {name: digest(OUTPUT_DIR / (name + ".json")) for name in results["reports"]})
    emit(path, "progress", message=f"{results['summary']['indicators']} indicators, {results['summary']['reports']} native reports")


def context():
    from .workspace import indicators_dir
    from .quantscript import template
    config_path = root().parents[1] / "systems/config.json"
    saved_systems = read(config_path) if config_path.is_file() else None
    return {"language": "en", "library": str(indicators_dir()), "research": str(root()), "protocol": PROTOCOL,
            "saved_systems": saved_systems,
            "contract": Path(__file__).with_name("contract.py").read_text(encoding="utf-8"),
            "template": template("research_example", "ResearchExample", "Research example"),
            "workflow": [
                "Use research_create(kind=gauntlet, title=the user's request) to stage a private library copy.",
                "Research distinct mechanisms and cite their original sources in each hypothesis. Do not invent optimization history.",
                "Write all hypotheses, titles, UI-facing explanations and HTML reports in English.",
                "Submit each new file, full source, and registered keys with research_candidate. Fix failed checks before continuing.",
                "Call quantsuite.algo.research_run with the run id. Poll research_read and forge_jobs; never call a smoke test certification.",
                "Read every failure and track verdict. Overall certification requires all three tracks; profits are a separate question.",
                "For system comparisons use kind=profit with system (a saved system's name or id) or an explicit config; dates default to the saved start and yesterday; indicators default to each base's version for the system's track; sizes limits aggregate sizes (2+ distinct bases); budget caps the aggregates tested.",
                "Report tool: quantsuite.algo.research_gauntlet_overview compiles the library's existing Gauntlet evidence (no re-test) into the English Gauntlet report in one call.",
                "Report tool: quantsuite.algo.research_aggregate_comparison searches aggregates of any size (2+ distinct bases) against the system's current aggregate, with the top 10, the best of each size and independent later starts, verified natively.",
                "Report tool: quantsuite.algo.research_profit_overview ranks every candidate alone and aggregates of 2+ distinct bases in a saved system by final capital (bulk evaluation identical to the engine, shown candidates verified natively).",
                "Research runs preserve sources and evidence privately. Use research_install only for selected, track-certified results; then create_strategy_from_indicator.",
                "Do not start a bot or publish private sources to a public repository. Past inspected history is retrospective, not pristine out-of-sample."],
            "runs": list_runs()[:20]}


def dispatch(request):
    action = request.pop("action")
    if action == "context":
        return context()
    if action == "list":
        return list_runs()
    if action == "create":
        return create(request)
    if action == "candidate":
        return candidate(**request)
    if action == "import":
        return import_report(**request)
    if action == "read":
        return details(**request)
    if action == "install":
        from .lab_install import install
        return install(**request)
    raise ValueError("Unknown research action")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["query", "run", "worker"])
    parser.add_argument("--id")
    parser.add_argument("--path", type=Path)
    args = parser.parse_args()
    if args.action == "query":
        try:
            print(json.dumps(dispatch(json.load(sys.stdin)), default=str, allow_nan=False))
            return 0
        except Exception as exc:
            print(json.dumps({"error": str(exc)}))
            return 2
    if args.action == "run":
        return run(args.id)
    doc = read(args.path / "run.json")
    if doc["kind"] == "gauntlet":
        gauntlet_worker(args.path, doc)
    elif doc["kind"] == "gauntlet_overview":
        gauntlet_overview_worker(args.path, doc)
    elif doc["kind"] == "aggregates":
        from .lab_aggregates import run as aggregates
        aggregates(args.path, doc)
    else:
        from .lab_profit import run as profit
        profit(args.path, doc)
    return 0


if __name__ == "__main__":
    sys.exit(main())
