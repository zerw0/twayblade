# Release tags match the app version (#56)

## Built

- The app reports its Cargo workspace version to the updater. Every build up
  to v0.1.0-rc.3 was version `0.1.0`, which semver orders after
  `0.1.0-rc.N`, so installed release candidates never offered a newer rc.
- `scripts/check-release-version.py` fails unless a tag's version equals the
  `[workspace.package]` version. A new `Release version` job in `build.yml`
  runs it before the builds, so a mismatched tag builds nothing and creates
  no draft.
- `macos/build.sh` sets `CFBundleShortVersionString` from `Cargo.toml` (its
  numeric part), so the bundle no longer depends on a hand-edited plist.
- The rule is written down in `docs/architecture/UPDATES.md`.
- The version itself is not bumped here: the release pull request sets it
  (for the updater test, `0.1.1-rc.1`).

## Verified

- `python3 scripts/test_check_release_version.py`: matching version passes,
  a differing tag fails. Added to the release script tests in `check.yml`.
- `actionlint` accepts the workflow (one pre-existing shellcheck note).
- The PlistBuddy step sets `0.1.1` for `0.1.1-rc.1` on a copy of the plist.

## Not verified

- A tag build on GitHub (no tag pushed).
- The updater offering a newer release end to end: needs the `0.1.1-rc.1`
  tag, which needs your permission.
