#!/usr/bin/env python3
"""Explicit offline migration into a NEW native profile; never imports accounts."""
import argparse
import fcntl
import json
from pathlib import Path
import shutil
import sqlite3
import time

CONFIG_FILES = ("session.json", "playback_settings.json", "hotkeys.json")


def import_profile(data_dir, config_dir, destination):
    source = Path(data_dir).resolve(strict=True)
    config = Path(config_dir).resolve(strict=True)
    target = Path(destination).absolute()
    database = source / "muzon.sqlite3"
    if not database.is_file():
        raise ValueError("Source data directory must contain muzon.sqlite3")
    # Never nest the destination inside the source data/config directories.
    resolved_target = target.resolve()
    if any(resolved_target == parent or parent in resolved_target.parents for parent in (source, config)):
        raise ValueError("Destination must be separate from source data and config")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.mkdir()  # Exclusive reservation: even an existing empty directory is refused.
    try:
        with (target / "native.lock").open("xb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            (target / "config").mkdir()
            # SQLite backup includes committed WAL pages; copying the .db alone does not.
            with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True, timeout=10) as src:
                src.execute("PRAGMA query_only=ON")
                if src.execute("PRAGMA quick_check").fetchone() != ("ok",):
                    raise ValueError("Source SQLite quick_check failed")
                with sqlite3.connect(target / "muzon.sqlite3") as dst:
                    deadline = time.monotonic() + 120
                    def progress(_status, _remaining, _total):
                        if time.monotonic() > deadline:
                            raise TimeoutError("SQLite backup exceeded two minutes; close the source player and retry")
                    src.backup(dst, pages=256, progress=progress)
                    if dst.execute("PRAGMA quick_check").fetchone() != ("ok",):
                        raise ValueError("Copied SQLite quick_check failed")
                    # Old native handoff records must not be replayed into another account.
                    dst.execute("DROP TABLE IF EXISTS native_listen_outbox")
            for name in CONFIG_FILES:
                path = config / name
                if path.exists():
                    if not path.is_file() or path.stat().st_size > 2 * 1024 * 1024:
                        raise ValueError(f"Invalid or oversized config: {name}")
                    payload = path.read_bytes()
                    if not isinstance(json.loads(payload), dict):
                        raise ValueError(f"Expected JSON object: {name}")
                    (target / "config" / name).write_bytes(payload)
            theme = config / "theme.json"
            if theme.exists():
                if theme.stat().st_size > 2 * 1024 * 1024:
                    raise ValueError("Theme exceeds 2 MiB")
                payload = theme.read_bytes()
                if not isinstance(json.loads(payload), dict):
                    raise ValueError("Expected theme JSON object")
                copied_theme = target / "config/imported-theme.json"
                copied_theme.write_bytes(payload)
                (target / "config/appearance.json").write_text(json.dumps({"themeFile": str(copied_theme)}, indent=2))
            # No account JSON, credentials, sockets, locks, cache or desktop settings copied.
    except BaseException:
        shutil.rmtree(target)
        raise
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", required=True, type=Path, help="source directory containing muzon.sqlite3")
    parser.add_argument("--config", required=True, type=Path, help="source config directory, separate in Tauri")
    parser.add_argument("--destination", required=True, type=Path, help="new, nonexistent native profile directory")
    args = parser.parse_args()
    try:
        result = import_profile(args.data, args.config, args.destination)
    except (OSError, ValueError, sqlite3.Error) as error:
        parser.exit(1, f"Import failed: {error}\n")
    print(f"Imported profile: {result}\nAccounts must be connected separately. Music stays at its original paths.")


if __name__ == "__main__":
    main()
