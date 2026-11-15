#!/usr/bin/env python3
"""Fill the Homebrew cask template for a release (#52).

    python3 scripts/homebrew-cask.py --version 0.2.0 --sums dist/SHA256SUMS > twayblade.rb

Reads the DMG's SHA-256 from the release's SHA256SUMS and writes the cask for
our own tap. Publishing the tap (a separate repository) needs explicit
authorization; this script only prints the file.
"""
import argparse
import re
import sys
from pathlib import Path

TEMPLATE = Path(__file__).resolve().parent.parent / "packaging" / "homebrew" / "twayblade.rb"
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$")


def dmg_name(version):
    return f"Twayblade-{version}-arm64.dmg"


def sha_for(sums, name):
    for line in sums.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2 and parts[1].lstrip("*").strip() == name:
            digest = parts[0].lower()
            if re.fullmatch(r"[0-9a-f]{64}", digest):
                return digest
    return None


def render(template, version, sums):
    if not VERSION.match(version):
        raise ValueError(f"not a release version: {version!r}")
    digest = sha_for(sums, dmg_name(version))
    if digest is None:
        raise ValueError(f"{dmg_name(version)} is not in SHA256SUMS")
    out = template.replace('version "VERSION"', f'version "{version}"', 1)
    out = out.replace('sha256 "SHA256"', f'sha256 "{digest}"', 1)
    if 'version "VERSION"' in out or 'sha256 "SHA256"' in out:
        raise ValueError("template placeholders were not replaced")
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True)
    parser.add_argument("--sums", required=True, type=Path)
    args = parser.parse_args()
    try:
        sys.stdout.write(render(TEMPLATE.read_text(), args.version, args.sums.read_text()))
    except ValueError as error:
        sys.exit(f"homebrew-cask: {error}")


if __name__ == "__main__":
    main()
