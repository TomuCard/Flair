#!/usr/bin/env python3
"""Writes the release manifest.json (read by `> update`).

Usage: make_manifest.py <dist dir> <version> <channel> <commit> <notes file>
Environment: MIN_UPDATER_VERSION, REQUIRES_RESET ("true"/"false").
The config version is read from crates/flair-core/src/settings/config.rs.
"""

import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

ASSETS = {
    "windows-x86_64": "flair-windows-x86_64.exe",
    "linux-x86_64": "flair-linux-x86_64",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def config_version() -> int:
    source = Path("crates/flair-core/src/settings/config.rs").read_text(encoding="utf-8")
    match = re.search(r"pub const CONFIG_VERSION: u32 = (\d+);", source)
    if not match:
        sys.exit("CONFIG_VERSION not found")
    return int(match.group(1))


def main() -> None:
    dist, version, channel, commit, notes_file = sys.argv[1:6]
    dist = Path(dist)
    notes = [line.strip() for line in Path(notes_file).read_text(encoding="utf-8").splitlines() if line.strip()]
    assets = {}
    for key, name in ASSETS.items():
        path = dist / name
        if not path.exists():
            sys.exit(f"missing asset {name}")
        assets[key] = {"file": name, "sha256": sha256(path), "signature": f"{name}.minisig"}
    manifest = {
        "schema": 1,
        "version": version,
        "channel": channel,
        "commit": commit[:7],
        "date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "notes": notes[:30],
        "min_updater_version": os.environ.get("MIN_UPDATER_VERSION", "0.1.0-0"),
        "config_version": config_version(),
        "requires_reset": os.environ.get("REQUIRES_RESET", "false") == "true",
        "assets": assets,
    }
    (dist / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
