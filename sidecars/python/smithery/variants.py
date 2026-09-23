"""Parameter versions of an editable base script, under ``indicators/versions/<base>/``.

Two kinds of JSON files live there:

* Research subversions (format 1): immutable children the walk-forward
  writes, random keys ``<base>_v_<id>``, status ``research``.
* Role versions (format 2, user 2026-09-23): every indicator's fixed version
  slots under stable keys — ``standard`` (the base itself: its file only
  carries the Standard's evidence), ``optimized`` (the general optimization,
  ``<base>_opt``) and ``optimized_1h`` / ``optimized_4h`` / ``optimized_1d``
  (``<base>_opt_1h`` …) for 1h, 4h and daily bots.

Every file is bound to the SIGNATURE of the script that implements its base:
the normalized syntax tree (docstrings, comments and formatting ignored) of
that script and of every script it declares in ``REQUIRES``, plus
``contract.CONTRACT_VERSION``. A code change makes that indicator's versions —
and its dependents' — unavailable until they are re-forged instead of
silently changing their signals; an edit elsewhere in the library changes
nothing. Existing consumer keys remain aliases for historical children.
"""
from __future__ import annotations

import ast
import datetime as dt
import hashlib
import inspect
import json
import os
from pathlib import Path
import uuid

from .contract import CONTRACT_VERSION
from .workspace import indicators_dir, library_metadata


def __getattr__(name):
    if name.startswith("Forge_"):
        from . import indicators
        if name in globals():
            return globals()[name]
    raise AttributeError(name)


# role → (key suffix, label). The order is the order editors show them in.
ROLES = {
    "standard": ("", "Standard"),
    "optimized": ("_opt", "Optimized (general)"),
    "optimized_1h": ("_opt_1h", "Optimized 1H"),
    "optimized_4h": ("_opt_4h", "Optimized 4H"),
    "optimized_1d": ("_opt_1d", "Optimized 1D"),
}
# Registry keys are at most 40 characters; the longest suffix is 7.
MAX_BASE_KEY = 33

UNAVAILABLE: list[dict] = []
# base → {role: key} for every valid role file (the standard slot only when
# its evidence file is valid), and key → the loaded format-2 document.
VERSIONS: dict[str, dict[str, str]] = {}
VERSION_DOCS: dict[str, dict] = {}

LEGACY = library_metadata().get("legacy_variants", {})


def version_key(base: str, role: str) -> str:
    if role not in ROLES:
        raise ValueError(f"unknown version role '{role}' — one of {list(ROLES)}")
    return base + ROLES[role][0]


# ---------------------------------------------------------------- signatures

_NORMALIZED: dict[tuple, bytes | None] = {}


def _normalized(path: Path) -> bytes | None:
    """The script's syntax tree without docstrings or positions — None when
    the file is missing or does not parse."""
    try:
        stat = path.stat()
    except OSError:
        return None
    cache_key = (str(path), stat.st_mtime_ns, stat.st_size)
    if cache_key not in _NORMALIZED:
        try:
            tree = ast.parse(path.read_text(encoding="utf-8"))
        except (OSError, SyntaxError, UnicodeDecodeError, ValueError):
            _NORMALIZED[cache_key] = None
        else:
            for node in ast.walk(tree):
                if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                    body = node.body
                    if (body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant)
                            and isinstance(body[0].value.value, str)):
                        node.body = body[1:] or [ast.Pass()]
            _NORMALIZED[cache_key] = ast.dump(tree, include_attributes=False).encode("utf-8")
    return _NORMALIZED[cache_key]


def requirements_of_source(text: str) -> tuple[str, ...] | None:
    """The module-level ``REQUIRES`` of a script's source: a tuple of script
    names, () when it declares none, None when the declaration is not a
    literal tuple / list of names (or the source does not parse)."""
    try:
        tree = ast.parse(text)
    except (SyntaxError, ValueError):
        return None
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "REQUIRES" for t in node.targets):
            if not isinstance(node.value, (ast.Tuple, ast.List)):
                return None
            names = []
            for element in node.value.elts:
                if not (isinstance(element, ast.Constant) and isinstance(element.value, str)):
                    return None
                names.append(element.value)
            return tuple(names)
    return ()


_REQUIREMENTS: dict[str, tuple[tuple, dict]] = {}
_SIGNATURES: dict[tuple, str | None] = {}


def _library_state(folder: Path) -> tuple:
    state = []
    for path in sorted(folder.glob("*.py")):
        try:
            stat = path.stat()
        except OSError:
            continue
        state.append((path.name, stat.st_mtime_ns, stat.st_size))
    return tuple(state)


def declared_requirements(folder: Path | None = None) -> dict[str, tuple[str, ...]]:
    """``REQUIRES`` of every script in the library, read without importing
    (cached until a script file changes)."""
    folder = Path(folder or indicators_dir())
    state = _library_state(folder)
    cached = _REQUIREMENTS.get(str(folder))
    if cached and cached[0] == state:
        return cached[1]
    out = {}
    for path in sorted(folder.glob("*.py")):
        if path.stem.startswith("_"):
            continue
        try:
            out[path.stem] = requirements_of_source(path.read_text(encoding="utf-8")) or ()
        except (OSError, UnicodeDecodeError):
            out[path.stem] = ()
    _REQUIREMENTS[str(folder)] = (state, out)
    return out


def requirement_closure(stem: str, requirements: dict[str, tuple[str, ...]]) -> list[str]:
    """The script and everything it needs, transitively — sorted."""
    seen, todo = set(), [stem]
    while todo:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        todo.extend(requirements.get(current, ()))
    return sorted(seen)


def script_signature(stem: str, folder: Path | None = None) -> str | None:
    """The signature of one script: its normalized tree and those of its
    REQUIRES closure, and the contract generation. None when the script
    itself cannot be read."""
    folder = Path(folder or indicators_dir())
    cache_key = (str(folder), stem, _library_state(folder))
    if cache_key in _SIGNATURES:
        return _SIGNATURES[cache_key]
    requirements = declared_requirements(folder)
    own = _normalized(folder / f"{stem}.py")
    if own is None:
        _SIGNATURES[cache_key] = None
        return None
    digest = hashlib.sha256(f"contract:{CONTRACT_VERSION}".encode())
    for name in requirement_closure(stem, requirements):
        digest.update(b"\0" + name.encode("utf-8") + b"\0")
        digest.update(_normalized(folder / f"{name}.py") or b"<missing>")
    _SIGNATURES[cache_key] = digest.hexdigest()
    return _SIGNATURES[cache_key]


def stem_of(cls) -> str | None:
    """The script that implements an indicator class (a version child's is
    its base's)."""
    for klass in getattr(cls, "__mro__", ()):
        module = getattr(klass, "__module__", "") or ""
        if module.startswith("smithery.indicators."):
            return module.rsplit(".", 1)[-1]
    return None


def source_signature(key: str) -> str | None:
    """The signature of the script behind a registry key."""
    from .indicators import REGISTRY
    stem = stem_of(REGISTRY[key])
    return script_signature(stem) if stem else None


def directory() -> Path:
    return indicators_dir() / "versions"


# ---------------------------------------------------------------- writing

def _normalize(value):
    return json.loads(json.dumps(value, sort_keys=True, default=list))


def _write_atomic(target: Path, doc: dict, *, replace: bool) -> None:
    # Publish only complete JSON. A crash leaves a non-discoverable .tmp.
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(f"{target.stem}.{uuid.uuid4().hex[:8]}.tmp")
    with temporary.open("x", encoding="utf-8") as stream:
        json.dump(doc, stream, indent=2, allow_nan=False)
    if replace:
        os.replace(temporary, target)
    else:
        temporary.rename(target)


def create_variant(key: str, params: dict, evidence: dict, *, expected_source: str | None = None) -> dict:
    """A research subversion (format 1) of a registry key."""
    from .indicators import REGISTRY, VARIANTS
    from .evidence import finite_json

    signature = source_signature(key)
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
    target = directory() / parent / (child_key + ".json")
    _write_atomic(target, doc, replace=False)
    return {**doc, "path": str(target)}


def write_version(base: str, role: str, params: dict, evidence: dict, *,
                  status: str = "released", expected_source: str | None = None) -> dict:
    """Write (or replace) one role version of a base indicator (format 2).

    The standard role only records the Standard's evidence: its params must
    be the script's defaults. Every other role registers ``<base><suffix>``
    with these params on the next load."""
    from .indicators import REGISTRY, VARIANTS
    from .evidence import finite_json

    key = version_key(base, role)
    if base not in REGISTRY or base in VARIANTS:
        raise ValueError(f"'{base}' is not a base indicator")
    if role != "standard" and len(base) > MAX_BASE_KEY:
        raise ValueError(f"'{base}' is too long for version keys (at most {MAX_BASE_KEY} characters)")
    if status not in ("released", "research"):
        raise ValueError("status must be released or research")
    cls = REGISTRY[base]
    unknown = set(params) - set(cls.default_params())
    if unknown:
        raise ValueError(f"Unknown parameters: {sorted(unknown)}")
    resolved = cls(**params).params
    if role == "standard" and _normalize(resolved) != _normalize(cls().params):
        raise ValueError("The Standard version carries the script's own defaults")
    signature = source_signature(base)
    if signature is None:
        raise ValueError(f"The script behind '{base}' cannot be read")
    if expected_source is not None and signature != expected_source:
        raise ValueError("Scripts changed during optimization; no version was saved")
    doc = finite_json(dict(
        format=2, key=key, base_key=base, role=role, label=ROLES[role][1],
        created_at=dt.datetime.now(dt.timezone.utc).isoformat(), params=resolved,
        status=status, source_sha256=signature, contract=CONTRACT_VERSION,
        base_file=Path(inspect.getfile(cls)).name, evidence=evidence,
    ))
    target = directory() / base / f"{key}.json"
    _write_atomic(target, doc, replace=True)
    VERSION_DOCS[key] = {**doc, "path": str(target)}
    return VERSION_DOCS[key]


def _merge(into: dict, update: dict) -> dict:
    out = dict(into)
    for name, value in update.items():
        out[name] = _merge(out[name], value) if isinstance(value, dict) and isinstance(out.get(name), dict) else value
    return out


def update_evidence(key: str, evidence: dict) -> dict:
    """Merge new evidence into a valid role version — its params never
    change. A base without a standard file gets one."""
    from .indicators import REGISTRY, VARIANTS
    from .evidence import finite_json

    doc = VERSION_DOCS.get(key)
    if doc is None:
        if key in REGISTRY and key not in VARIANTS:
            return write_version(key, "standard", REGISTRY[key]().params, evidence)
        raise KeyError(f"no valid version file for '{key}'")
    path = Path(doc["path"])
    current = json.loads(path.read_text(encoding="utf-8"))
    if current.get("source_sha256") != source_signature(current["base_key"]):
        raise ValueError(f"The script behind '{key}' changed; re-forge this version first")
    current["evidence"] = _merge(current.get("evidence") or {}, finite_json(evidence))
    current["evidence_updated_at"] = dt.datetime.now(dt.timezone.utc).isoformat()
    _write_atomic(path, current, replace=True)
    VERSION_DOCS[key] = {**current, "path": str(path)}
    return VERSION_DOCS[key]


def version_evidence(timeframe: str) -> dict[str, dict]:
    """The released evidence of every valid role version on one track, in
    the registry's verdict shape (source ``release``)."""
    out = {}
    for key, doc in VERSION_DOCS.items():
        verdict = ((doc.get("evidence") or {}).get("timeframes") or {}).get(timeframe)
        if isinstance(verdict, dict) and verdict.get("score") is not None:
            out[key] = {**verdict, "source": "release"}
    return out


# ---------------------------------------------------------------- loading

def _child(registry: dict, key: str, cls, params: dict, label: str):
    # Module globals make the generated class importable by spawn workers.
    name = "Forge_" + key
    defaults = {**cls.default_params(), **params}
    for pname, value in defaults.items():
        if isinstance(cls.default_params().get(pname), tuple) and isinstance(value, list):
            defaults[pname] = tuple(value)

    def default_params(_cls, values=defaults):
        return dict(values)
    child = type(name, (cls,), {
        "__module__": __name__, "name": cls.name + " · " + label,
        "default_params": classmethod(default_params),
    })
    globals()[name] = child
    return child


def load_variants(registry: dict, warmup: dict, errors: dict) -> dict:
    UNAVAILABLE.clear()
    VERSIONS.clear()
    VERSION_DOCS.clear()
    metadata = {key: dict(base_key=base, label=label, status="legacy")
                for key, (base, label) in LEGACY.items()
                if key in registry and base in registry}
    from .indicators._discover import KEY_PATTERN
    signatures: dict[str, str | None] = {}

    def signature_of(base: str) -> str | None:
        if base not in signatures:
            stem = stem_of(registry[base])
            signatures[base] = script_signature(stem) if stem else None
        return signatures[base]

    for path in sorted(directory().glob("*/*.json")):
        doc = {}
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
            key, base = doc["key"], doc["base_key"]
            if doc.get("format") == 2:
                _load_role(registry, warmup, metadata, path, doc, signature_of, KEY_PATTERN)
                continue
            if not KEY_PATTERN.fullmatch(key) or key in registry or base not in registry or base in metadata:
                raise ValueError("Invalid, duplicate or missing base/key")
            if path.stem != key or path.parent.name != base or doc.get("format") != 1:
                raise ValueError("Invalid subversion path/format")
            if doc["source_sha256"] != signature_of(base):
                raise ValueError("Base scripts changed; re-forge this subversion")
            implementation = doc.get("implementation_key", base)
            if implementation not in registry or (implementation in metadata and metadata[implementation].get("status") != "legacy"):
                raise ValueError("Missing base implementation")
            cls = registry[implementation]
            params = doc["params"]
            if not isinstance(params, dict) or set(params) - set(cls.default_params()):
                raise ValueError("Unknown subversion parameters")
            registry[key] = _child(registry, key, cls, params, doc["label"])
            warmup[key] = warmup[base]
            metadata[key] = {**doc, "path": str(path)}
        except Exception as exc:
            error = f"{type(exc).__name__}: {exc}"
            errors["versions/" + path.parent.name + "/" + path.name] = error
            UNAVAILABLE.append(dict(key=path.stem, base_key=path.parent.name,
                label=doc.get("label", path.stem) if isinstance(doc, dict) else path.stem,
                role=doc.get("role") if isinstance(doc, dict) else None,
                error=error, path=str(path)))
    return metadata


def _load_role(registry, warmup, metadata, path: Path, doc: dict, signature_of, key_pattern) -> None:
    key, base, role = doc["key"], doc["base_key"], doc.get("role")
    if role not in ROLES:
        raise ValueError(f"Unknown version role '{role}'")
    if base not in registry or base in metadata or key != version_key(base, role):
        raise ValueError("Invalid or missing base / key for this role")
    if path.stem != key or path.parent.name != base:
        raise ValueError("Invalid version path")
    if role != "standard" and (key in registry or not key_pattern.fullmatch(key)):
        raise ValueError(f"'{key}' is already registered or not a registry key")
    if doc.get("contract", CONTRACT_VERSION) > CONTRACT_VERSION:
        raise ValueError(f"Written for contract {doc.get('contract')}; this engine speaks {CONTRACT_VERSION}")
    if doc.get("source_sha256") != signature_of(base):
        raise ValueError("The script changed; re-forge this version")
    cls = registry[base]
    params = doc.get("params")
    if not isinstance(params, dict) or set(params) - set(cls.default_params()):
        raise ValueError("Unknown version parameters")
    if role == "standard":
        if _normalize({**cls.default_params(), **params}) != _normalize(cls().params):
            raise ValueError("The script's defaults changed; re-run the Standard's verdicts")
    else:
        registry[key] = _child(registry, key, cls, params, doc.get("label") or ROLES[role][1])
        warmup[key] = warmup.get(base, 400)
        metadata[key] = {**doc, "path": str(path)}
    VERSIONS.setdefault(base, {})[role] = key
    VERSION_DOCS[key] = {**doc, "path": str(path)}
