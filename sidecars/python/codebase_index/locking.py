"""Process-wide workspace coordination, separate from SQLite transactions."""
from contextlib import contextmanager
import errno
import os
from pathlib import Path
import time


class IndexBusyError(Exception):
    pass


@contextmanager
def codebase_lock(db_path: Path, timeout: float = 120.0):
    """Serialize CLI operations for one DB; the OS releases the lock on exit.

    Keep the file after unlocking: unlinking it could let another process lock
    a new inode while a queued process still holds the old one.
    """
    lock_path = db_path.with_suffix(db_path.suffix + ".lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+b") as handle:
        if handle.seek(0, os.SEEK_END) == 0:
            handle.write(b"\0")
            handle.flush()
        if os.name == "nt":
            import msvcrt

            def acquire():
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)

            def release():
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            def acquire():
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

            def release():
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)

        deadline = time.monotonic() + timeout
        while True:
            try:
                acquire()
                break
            except OSError as error:
                if error.errno not in (errno.EACCES, errno.EAGAIN, errno.EDEADLK):
                    raise
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise IndexBusyError(
                        "Another indexing operation is still running for this workspace. "
                        "Wait for it to finish, then retry."
                    ) from None
                time.sleep(min(0.1, remaining))
        try:
            yield
        finally:
            release()
