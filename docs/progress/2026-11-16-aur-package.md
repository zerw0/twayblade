# AUR package twayblade-bin, ready to publish

## Built

- `packaging/aur/PKGBUILD` is now a template that `scripts/aur-pkgbuild.py`
  fills from a release's `SHA256SUMS`, like the Homebrew cask script:
  - Arch versions can't contain `-`, so tag `0.1.1-rc.1` becomes
    `pkgver=0.1.1rc.1` (sorting before `0.1.1`); the tag stays in the
    download URL, saved under a per-version file name.
  - Adds `hicolor-icon-theme` (namcap) and `options=('!debug')` (a prebuilt
    binary has no debug data). Installs the 512 px icon only (no SVG).
- `scripts/test_aur_pkgbuild.py` (pre-release and stable versions, refusals)
  runs with the other release script tests in `check.yml`.
- Publishing steps are in `packaging/README.md`.

## Verified

- Script tests pass.
- In an Arch Linux container (x86_64): `makepkg --printsrcinfo`, `makepkg`
  from the real v0.1.1-rc.1 tarball (checksum verified), `namcap` without
  errors, `pacman -U` installs it, and `ldd /usr/bin/twayblade` finds every
  library.

## Not verified

- Running the app on a physical Arch machine; the package is not pushed to
  the AUR yet (needs the owner's AUR account).
