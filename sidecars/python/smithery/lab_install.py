"""Explicit, conflict-refusing handoff of a tested research script to the library."""
import shutil
import tempfile

from .lab import digest, folder, locked, python_json, read, save


def install(run_id, indicator, timeframe):
    from .workspace import indicators_dir
    from .variants import declared_requirements, requirement_closure
    path = folder(run_id)
    with locked(path):
        doc = read(path / "run.json")
        if doc["status"] != "done" or doc["kind"] != "gauntlet":
            raise ValueError("Only a completed native Gauntlet run can be installed")
        for name, expected in read(path / "evidence-hashes.json").items():
            if digest(path / name) != expected:
                raise ValueError("Research evidence changed after evaluation")
        rows = read(path / "results.json")["rows"]
        row = next((r for r in rows if r["key"] == indicator), None)
        if timeframe not in ("1h", "4h", "1d") or not row or not (row["timeframes"].get(timeframe) or {}).get("certified"):
            raise ValueError("Select an indicator certified on the target timeframe")
        inputs = read(path / "inputs.json")
        # No source may have changed after freezing; optimize may only add versions.
        for name, expected in inputs.items():
            if name.startswith("engine/") or (name.startswith("library/") and name.endswith(".py")):
                if digest(path / name) != expected:
                    raise ValueError("Frozen source changed after evaluation")
        source, target = path / "library", indicators_dir()
        files = [s + ".py" for s in requirement_closure(row["script"][:-3], declared_requirements(source))]
        versions = source / "versions" / row["base_key"]
        if versions.exists():
            files += [str(f.relative_to(source)) for f in versions.glob("*.json")]
        from .data import OUTPUT_DIR
        # Every installed version file names its own native reports; keep the evidence with them.
        family = [r for r in rows if r["base_key"] == row["base_key"]]
        names = {(verdict or {}).get("report") for r in family for verdict in r["timeframes"].values()}
        reports = []
        for name in sorted(n for n in names if n):
            if type(path)(name).name == name:
                for extension in (".json", ".md"):
                    origin = path / "vault/Output" / (name + extension)
                    if origin.exists():
                        dest = OUTPUT_DIR / origin.name
                        if dest.exists() and digest(dest) != digest(origin):
                            raise ValueError(f"Existing report differs: {origin.name}")
                        reports.append((origin, dest))
        # Existing differing files are never overwritten, including old versions.
        for name in files:
            if (target / name).exists() and digest(target / name) != digest(source / name):
                raise ValueError(f"Library conflict: {name}. Preserve the current version and use a new research key.")
        with tempfile.TemporaryDirectory(prefix="qs-research-install-") as tmp:
            stage = type(path)(tmp)
            shutil.copytree(target, stage / "library")
            for name in files:
                (stage / "library" / name).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source / name, stage / "library" / name)
            registry = python_json(stage, ["smithery.registry", "--json"])
            if not any(r["key"] == indicator and r["params"] == row["params"] for r in registry["indicators"]):
                raise ValueError("Selected tested parameters are unavailable in the current engine")
            created = []
            try:
                for name in files:
                    dest = target / name
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    try:
                        with dest.open("xb") as stream:
                            created.append(dest)
                            stream.write((source / name).read_bytes())
                    except FileExistsError:
                        if digest(dest) != digest(source / name):
                            raise ValueError(f"Concurrent library change: {name}") from None
            except Exception:
                for dest in created:
                    if dest.exists() and digest(dest) == digest(source / dest.relative_to(target)):
                        dest.unlink()
                raise
        # Registry reads Standard certification from native report files; role
        # versions also reference these names. Never overwrite existing evidence.
        OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
        for origin, dest in reports:
            try:
                with dest.open("xb") as stream:
                    stream.write(origin.read_bytes())
            except FileExistsError:
                if digest(dest) != digest(origin):
                    raise ValueError(f"Concurrent report change: {dest.name}") from None
        record = {"indicator": indicator, "timeframe": timeframe, "files": files,
                  "params": row["params"], "next_tool": "quantsuite.algo.create_strategy_from_indicator"}
        save(path / "installed.json", record)
        return record
