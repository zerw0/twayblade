#!/usr/bin/env python3
"""Fill the AUR PKGBUILD template (twayblade-bin) for a release.

    python3 scripts/aur-pkgbuild.py --version 0.1.1-rc.1 --sums dist/SHA256SUMS > PKGBUILD

Reads the Linux tarball's SHA-256 from the release's SHA256SUMS. Arch does not
allow "-" in pkgver, so tag 0.1.1-rc.1 becomes pkgver 0.1.1rc.1 (sorting
before 0.1.1); the tag itself stays in the download URL. Generate .SRCINFO
with `makepkg --printsrcinfo > .SRCINFO` on Arch. Pushing to the AUR is a
separate, explicit step.
"""
import argparse
import re
import sys
from pathlib import Path

TEMPLATE = Path(__file__).resolve().parent.parent / "packaging" / "aur" / "PKGBUILD"
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$")
TARBALL = "twayblade-linux-x86_64.tar.gz"


def pkgver(version):
    return version.replace("-", "")


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
    digest = sha_for(sums, TARBALL)
    if digest is None:
        raise ValueError(f"{TARBALL} is not in SHA256SUMS")
    out = template.replace("pkgver=PKGVER", f"pkgver={pkgver(version)}", 1)
    out = out.replace("_tag=TAG", f"_tag={version}", 1)
    out = out.replace("sha256sums=('SHA256')", f"sha256sums=('{digest}')", 1)
    for placeholder in ("pkgver=PKGVER", "_tag=TAG", "sha256sums=('SHA256')"):
        if placeholder in out:
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
        sys.exit(f"aur-pkgbuild: {error}")


if __name__ == "__main__":
    main()
