"""Forge parameter subversions: immutable JSON children of an editable base.

Existing consumer keys remain aliases for historical children. New children
carry source and parameter identity; editing the base makes them unavailable
until they are re-forged, rather than silently changing their signals.
"""
from __future__ import annotations

import datetime as dt
import hashlib
import inspect
import json
from pathlib import Path
import uuid

from .workspace import indicators_dir, library_metadata

def __getattr__(name):
    if name.startswith("Forge_"):
        from . import indicators
        if name in globals():
            return globals()[name]
    raise AttributeError(name)


UNAVAILABLE: list[dict] = []

LEGACY = library_metadata().get("legacy_variants", {})


def source_signature() -> str:
    digest = hashlib.sha256()
    for path in sorted(indicators_dir().glob("*.py")):
        if path.name.startswith("_"):
            continue
        digest.update(path.name.encode())
        digest.update(path.read_bytes())
    # Contract and loader changes also invalidate parameter evidence.
    digest.update((Path(__file__).parent / "contract.py").read_bytes())
    return digest.hexdigest()


def directory() -> Path:
    return indicators_dir() / "versions"


def create_variant(key: str, params: dict, evidence: dict, *, expected_source: str | None = None) -> dict:
    from .indicators import REGISTRY, VARIANTS
    from .evidence import finite_json

    signature = source_signature()
    if expected_source is not None and signature != expected_source:
        raise ValueError("Scripts changed during optimization; no subversion was saved")
    cls = REGISTRY[key]
    parent = VARIANTS.get(key, {}).get("base_key", key)
    implementation = VARIANTS.get(key, {}).get("implementation_key", key)
    if parent not in REGISTRY:
        raise ValueError("Missing base indicator")
    # Use the selected class defaults, including inherited fixed parameters.
    resolved = cls(**params).params
    unknown = set(params) - set(cls.default_params())
    if unknown:
        raise ValueError(f"Unknown parameters: {sorted(unknown)}")
    stamp = dt.datetime.now(dt.timezone.utc).isoformat()
    ident = uuid.uuid4().hex[:10]
    child_key = parent[:27] + "_v_" + ident
    doc = finite_json(dict(
        format=1, key=child_key, base_key=parent, parent_key=key, implementation_key=implementation,
        label=f"Forge {stamp[:10]} · {ident[:6]}", created_at=stamp,
        params=resolved, source_sha256=signature,
        base_file=Path(inspect.getfile(REGISTRY[parent])).name,
        evidence=evidence, status="research",
    ))
    folder = directory() / parent
    folder.mkdir(parents=True, exist_ok=True)
    target = folder / (child_key + ".json")
    # Publish only complete JSON. A crash leaves a non-discoverable .tmp.
    temporary = target.with_suffix(".tmp")
    with temporary.open("x", encoding="utf-8") as stream:
        json.dump(doc, stream, indent=2, allow_nan=False)
    temporary.rename(target)
    return {**doc, "path": str(target)}


def load_variants(registry: dict, warmup: dict, errors: dict) -> dict:
    UNAVAILABLE.clear()
    metadata = {key: dict(base_key=base, label=label, status="legacy")
                for key, (base, label) in LEGACY.items()
                if key in registry and base in registry}
    signature = source_signature()
    from .indicators._discover import KEY_PATTERN
    for path in sorted(directory().glob("*/*.json")):
        doc = {}
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
            key, base = doc["key"], doc["base_key"]
            if not KEY_PATTERN.fullmatch(key) or key in registry or base not in registry or base in metadata:
                raise ValueError("Invalid, duplicate or missing base/key")
            if path.stem != key or path.parent.name != base or doc.get("format") != 1:
                raise ValueError("Invalid subversion path/format")
            if doc["source_sha256"] != signature:
                raise ValueError("Base scripts changed; re-forge this subversion")
            implementation = doc.get("implementation_key", base)
            if implementation not in registry or (implementation in metadata and metadata[implementation].get("status") != "legacy"):
                raise ValueError("Missing base implementation")
            cls = registry[implementation]
            params = doc["params"]
            if not isinstance(params, dict) or set(params) - set(cls.default_params()):
                raise ValueError("Unknown subversion parameters")
            # Module globals make the generated class importable by spawn workers.
            name = "Forge_" + key
            def defaults(_cls, values=dict(params)):
                return dict(values)
            child = type(name, (cls,), {
                "__module__": __name__, "name": cls.name + " / " + doc["label"],
                "default_params": classmethod(defaults),
            })
            globals()[name] = child
            registry[key] = child
            warmup[key] = warmup[base]
            metadata[key] = {**doc, "path": str(path)}
        except Exception as exc:
            error = f"{type(exc).__name__}: {exc}"
            errors["versions/" + path.parent.name + "/" + path.name] = error
            UNAVAILABLE.append(dict(key=path.stem, base_key=path.parent.name,
                label=doc.get("label", path.stem) if isinstance(doc, dict) else path.stem,
                error=error, path=str(path)))
    return metadata
