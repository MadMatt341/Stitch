#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (c) 2026 MadMatt.
"""Copy license notices from the captured runtime source trees."""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys


def collect(source_root: Path, runtime_root: Path) -> None:
    source_root = source_root.resolve(strict=True)
    runtime_root = runtime_root.resolve(strict=True)
    destination_root = runtime_root / "licenses"
    destination_root.mkdir()
    notice_name = re.compile(
        r"^(?:licen[cs]e|copying|notice|copyright|unlicense)(?:[._-].*)?$", re.I
    )
    records = []
    seen_names = set()
    for package in sorted(source_root.iterdir()):
        if not package.is_dir() or package.is_symlink():
            continue
        for directory, folders, files in os.walk(package, followlinks=False):
            folders[:] = sorted(
                name for name in folders
                if name != ".git" and not (Path(directory) / name).is_symlink()
            )
            relative_directory = Path(directory).relative_to(package)
            license_directory = any(
                part.lower() in {"licenses", "licences", "copyrights"}
                for part in relative_directory.parts
            )
            for name in sorted(files):
                if not license_directory and not notice_name.fullmatch(name):
                    continue
                source = Path(directory) / name
                if source.is_symlink() or not source.is_file():
                    continue
                relative = source.relative_to(source_root)
                normalized = relative.as_posix().casefold()
                if normalized in seen_names:
                    raise RuntimeError(f"Windows license path collision: {relative}")
                seen_names.add(normalized)
                destination = destination_root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
                records.append({
                    "path": relative.as_posix(),
                    "sizeBytes": source.stat().st_size,
                    "sha256": hashlib.sha256(source.read_bytes()).hexdigest().upper(),
                })
    for package in ("ffmpeg", "x264", "x265", "libzimg"):
        if not any(record["path"].startswith(package + "/") for record in records):
            raise RuntimeError(f"No license notices found for {package}")
    (runtime_root / "LICENSES.json").write_text(
        json.dumps(records, indent=2) + "\n", encoding="utf-8"
    )
    (runtime_root / "THIRD-PARTY-NOTICES.txt").write_text(
        "FFmpeg runtime notices\n\n"
        "The FFmpeg tools use GPL-3.0-or-later.\n"
        "Each dependency retains its license and copyright notices.\n"
        "The licenses folder contains unchanged notices from the captured sources.\n"
        "LICENSES.json gives the file hashes and source paths.\n"
        "The collection includes build tools and unused source packages.\n"
        "It does not identify which dependencies the tools link.\n"
        "The companion source archive contains the resolved sources and build records.\n"
        "Keep these notices with the runtime.\n",
        encoding="utf-8",
    )
    print(f"Collected {len(records)} runtime license notices")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("Usage: collect-licenses.py <source-root> <runtime-root>")
    collect(Path(sys.argv[1]), Path(sys.argv[2]))
