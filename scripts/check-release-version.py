#!/usr/bin/env python3
"""Fail unless a release tag's version equals the workspace version in Cargo.toml.

The app compares its own Cargo version with GitHub releases (#56), so a build
tagged v0.1.1-rc.1 must also be version 0.1.1-rc.1 in Cargo.toml.
"""

import sys
import tomllib
from pathlib import Path


def workspace_version(root: Path) -> str:
    with open(root / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check-release-version.py <version>", file=sys.stderr)
        return 2
    tag = sys.argv[1]
    cargo = workspace_version(Path(__file__).resolve().parent.parent)
    if tag != cargo:
        print(
            f"Tag version {tag} does not match Cargo.toml version {cargo}. "
            "Set [workspace.package] version to the release version before tagging.",
            file=sys.stderr,
        )
        return 1
    print(f"Tag matches Cargo.toml version {cargo}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
