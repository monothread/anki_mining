#!/usr/bin/env python3
"""Writes the `latest.json` that the app reads to find out whether there is a newer version.

Usage: latest_json.py <version> <owner/repo> <notes file> <linux AppImage> <windows installer>

Next to each file there must be its signature (`<file>.sig`, made when the file was built with
TAURI_SIGNING_PRIVATE_KEY). The file names are the ones of the release assets.
"""
import json
import sys
from datetime import datetime, timezone
from pathlib import Path


def platform_entry(repo: str, tag: str, file: Path) -> dict:
    signature = Path(f"{file}.sig").read_text(encoding="utf-8").strip()
    if not signature:
        raise ValueError(f"the signature of {file.name} is empty")
    return {
        "signature": signature,
        "url": f"https://github.com/{repo}/releases/download/{tag}/{file.name}",
    }


def build_manifest(version: str, repo: str, notes: str, linux: Path, windows: Path, now: datetime) -> dict:
    tag = f"v{version}"
    return {
        "version": version,
        "notes": notes.strip(),
        "pub_date": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "linux-x86_64": platform_entry(repo, tag, linux),
            "windows-x86_64": platform_entry(repo, tag, windows),
        },
    }


def main(arguments: list[str]) -> int:
    if len(arguments) != 5:
        print(__doc__, file=sys.stderr)
        return 2
    version, repo, notes_file, linux, windows = arguments
    manifest = build_manifest(
        version,
        repo,
        Path(notes_file).read_text(encoding="utf-8"),
        Path(linux),
        Path(windows),
        datetime.now(timezone.utc),
    )
    print(json.dumps(manifest, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
