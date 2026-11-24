#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Writes the crates of Cargo.lock as sources of the Flatpak build.

The Flatpak of Desk is built without network: flatpak-builder downloads
each crate named in Cargo.lock, checks it against the checksum the lock
file gives, and unpacks it where Cargo finds it as a vendored source
(review RV, batch 6, B-060: the build downloaded its crates itself).

Usage:
    ci/flatpak-cargo-sources.py          writes build-aux/flatpak/cargo-sources.json
    ci/flatpak-cargo-sources.py --check  fails when that file is not up to date

The manifest sets CARGO_HOME to the folder `cargo` of the module, where
the sources land.
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
OUTPUT = ROOT / "build-aux" / "flatpak" / "cargo-sources.json"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"

CONFIG = """[source.vendored-sources]
directory = "cargo/vendor"

[source.crates-io]
replace-with = "vendored-sources"
"""


def sources(lock: dict) -> list:
    """The sources of every crate of the registry in the lock file."""
    out = []
    for package in lock.get("package", []):
        source = package.get("source")
        if source is None:
            # A crate of the workspace.
            continue
        if source != REGISTRY:
            sys.exit(f"error: {package['name']} comes from {source}; "
                     "only crates.io is vendored")
        name, version = package["name"], package["version"]
        checksum = package.get("checksum")
        if not checksum:
            sys.exit(f"error: {name} {version} has no checksum in Cargo.lock")
        folder = f"cargo/vendor/{name}-{version}"
        out.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": folder,
        })
        out.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": folder,
            "dest-filename": ".cargo-checksum.json",
        })
    out.append({
        "type": "inline",
        "contents": CONFIG,
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    return out


def main() -> None:
    with LOCK.open("rb") as file:
        text = json.dumps(sources(tomllib.load(file)), indent=4) + "\n"
    if sys.argv[1:] == ["--check"]:
        if not OUTPUT.exists() or OUTPUT.read_text() != text:
            sys.exit(f"error: {OUTPUT.relative_to(ROOT)} does not match Cargo.lock; "
                     "run ci/flatpak-cargo-sources.py")
        print(f"{OUTPUT.relative_to(ROOT)} matches Cargo.lock")
        return
    if sys.argv[1:]:
        sys.exit(__doc__)
    OUTPUT.write_text(text)
    print(f"wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
