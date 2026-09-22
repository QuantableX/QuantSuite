import os
import hashlib
from pathlib import Path

# File extension to tree-sitter language name mapping
EXTENSION_MAP: dict[str, str] = {
    ".py": "python",
    ".js": "javascript",
    ".jsx": "javascript",
    ".ts": "typescript",
    ".tsx": "typescript",
    ".rs": "rust",
    ".go": "go",
    ".java": "java",
    ".c": "c",
    ".h": "c",
    ".cpp": "cpp",
    ".hpp": "cpp",
    ".cc": "cpp",
    ".cxx": "cpp",
    ".cs": "c_sharp",
    ".rb": "ruby",
    ".php": "php",
    ".swift": "swift",
    ".kt": "kotlin",
    ".kts": "kotlin",
    ".scala": "scala",
    ".lua": "lua",
    ".r": "r",
    ".R": "r",
    ".sh": "bash",
    ".bash": "bash",
    ".zsh": "bash",
    ".vue": "vue",
    ".svelte": "svelte",
    ".html": "html",
    ".htm": "html",
    ".css": "css",
    ".scss": "scss",
    ".sass": "scss",
    ".sql": "sql",
    ".yaml": "yaml",
    ".yml": "yaml",
    ".toml": "toml",
    ".json": "json",
    ".xml": "xml",
    ".md": "markdown",
    ".markdown": "markdown",
    ".ex": "elixir",
    ".exs": "elixir",
    ".erl": "erlang",
    ".zig": "zig",
    ".dart": "dart",
    ".ps1": "powershell",
    ".psm1": "powershell",
}

# Directories to skip during codebase walking
SKIP_DIRS: set[str] = {
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    ".env",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".output",
    "target",
    ".idea",
    ".vscode",
    ".vs",
    "bin",
    "obj",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "coverage",
    ".svn",
    ".hg",
    ".cache",
    ".gradle",
    ".dart_tool",
    ".pub-cache",
    "vendor",
    "Pods",
    ".terraform",
}

# File extensions to skip (binary / non-code)
SKIP_EXTENSIONS: set[str] = {
    ".pyc", ".pyo", ".so", ".dll", ".dylib", ".exe",
    ".o", ".obj", ".a", ".lib", ".class", ".jar", ".war",
    ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar",
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".svg", ".webp",
    ".mp3", ".mp4", ".avi", ".mov", ".wav", ".flac", ".ogg", ".webm",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".lock", ".min.js", ".min.css", ".map",
    ".woff", ".woff2", ".ttf", ".eot", ".otf",
    ".sqlite", ".db", ".db-shm", ".db-wal",
    ".DS_Store", ".suo", ".user",
}

# Extensions skipped in "smart" filter mode (config, docs, styles, markup).
# These files rarely contain searchable code logic.
SMART_SKIP_EXTENSIONS: set[str] = {
    ".json", ".yaml", ".yml", ".toml", ".xml",
    ".md", ".markdown",
    ".html", ".htm",
    ".css", ".scss", ".sass",
}

# Common lockfiles and generated artifacts that add cost but little search value.
SKIP_FILENAMES: set[str] = {
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "cargo.lock",
    "poetry.lock",
    "pipfile.lock",
    "composer.lock",
    "deno.lock",
    "uv.lock",
}

# Relative path prefixes (from codebase root) to skip.
SKIP_RELATIVE_PREFIXES: tuple[str, ...] = (
    "src-tauri/gen/schemas/",
)

# Max file size to index (1 MB)
MAX_FILE_SIZE: int = 1_000_000


def compute_file_hash(path: str) -> str:
    """Compute SHA-256 hash of a file."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def detect_language(path: str) -> str | None:
    """Detect programming language from file extension."""
    ext = Path(path).suffix
    if not ext:
        return None
    # Try exact match first
    lang = EXTENSION_MAP.get(ext)
    if lang:
        return lang
    # Try lowercase
    return EXTENSION_MAP.get(ext.lower())


def walk_codebase(root: str, filter_mode: str = "everything") -> list[str]:
    """Walk a codebase directory and return all indexable source files.

    filter_mode:
        "everything" – index all recognised file types (default).
        "smart"      – skip config, docs, styles, and markup files
                       (json, yaml, toml, xml, md, html, css, scss).
    """
    files: list[str] = []
    root_path = Path(root).resolve()
    smart = filter_mode == "smart"

    if not root_path.is_dir():
        raise ValueError(f"Not a directory: {root}")

    for dirpath, dirnames, filenames in os.walk(root_path):
        # Remove skip directories in-place to prevent os.walk from descending
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]

        for fname in filenames:
            fpath = Path(dirpath) / fname
            rel_path = str(fpath.relative_to(root_path)).replace("\\", "/").lower()

            # Skip lockfiles and generated artifacts.
            if fname.lower() in SKIP_FILENAMES:
                continue
            if any(rel_path.startswith(prefix) for prefix in SKIP_RELATIVE_PREFIXES):
                continue

            # Skip by extension
            ext_lower = fpath.suffix.lower()
            if ext_lower in SKIP_EXTENSIONS:
                continue

            # Smart mode: additionally skip config/docs/style extensions
            if smart and ext_lower in SMART_SKIP_EXTENSIONS:
                continue

            # Skip files that are too large
            try:
                if fpath.stat().st_size > MAX_FILE_SIZE:
                    continue
            except OSError:
                continue

            # Only index files with recognized languages
            if detect_language(str(fpath)) is not None:
                files.append(str(fpath.resolve()))

    return sorted(files)


def get_data_dir() -> Path:
    """Get the directory for storing index databases."""
    env_dir = os.environ.get("QUANTMCP_INDEX_DIR")
    if env_dir:
        p = Path(env_dir)
    else:
        p = Path.home() / ".quantmcp" / "indexes"
    p.mkdir(parents=True, exist_ok=True)
    return p


def codebase_name_from_path(path: str) -> str:
    """Derive a codebase name from a directory path."""
    name = Path(path).resolve().name
    # Sanitize: lowercase, replace spaces/special chars with underscores
    sanitized = "".join(c if c.isalnum() or c in "-_" else "_" for c in name.lower())
    return sanitized.strip("_") or "codebase"


def get_db_path(codebase_name: str) -> Path:
    """Get the full path to a codebase's database file."""
    return get_data_dir() / f"{codebase_name}.db"


def list_db_files() -> list[Path]:
    """List all .db files in the data directory."""
    data_dir = get_data_dir()
    return sorted(data_dir.glob("*.db"))


def format_file_path(path: str, root: str) -> str:
    """Format a file path as relative to the codebase root."""
    try:
        return str(Path(path).resolve().relative_to(Path(root).resolve()))
    except ValueError:
        return path


def is_index_stale(codebase_path: str, last_indexed_ts: int, indexed_file_count: int, filter_mode: str = "everything") -> bool:
    """Quick check: are any files newer than last_indexed_ts?

    Returns True on first stale file found (fast bail-out).
    Also returns True if file count changed (new/deleted files).
    """
    try:
        current_files = walk_codebase(codebase_path, filter_mode=filter_mode)
    except (ValueError, OSError):
        return False

    # File count changed → new or deleted files
    if len(current_files) != indexed_file_count:
        return True

    # Check mtimes — bail on first stale file
    for file_path in current_files:
        try:
            mtime = os.path.getmtime(file_path)
            if mtime > last_indexed_ts:
                return True
        except OSError:
            continue

    return False
