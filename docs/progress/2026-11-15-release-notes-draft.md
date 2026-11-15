# Release notes draft generator

Issue #55 (release flow), first piece: `scripts/release-notes.py` drafts the
changelog for a release from the pull requests merged since the previous release.

```sh
scripts/release-notes.py --version 0.1.0-pre.1 --since v0.0.9      # a tag
scripts/release-notes.py --version 0.1.0-pre.1 --since 2026-11-01  # or a date
```

- Groups PRs as New, Improved and Fixed by label (`bug`) and the first word of the
  title; skips documentation-only and internal changes.
- Lists, once each, the gaps PR descriptions state under "Not verified", "Limits"
  or "Not included", so a release does not read as more tested than it is.
- Output is a draft to review and reword for users. It also lists gaps that later
  PRs may have closed, so each one needs checking before publishing.
- Reads only merged-PR metadata from GitHub (`gh`, or `nix shell nixpkgs#gh -c gh`,
  or a JSON file with `--input`). It does not publish, tag or sign anything.

Verified: seven unit tests (`python3 scripts/test_release_notes.py`) and a real run
against this repository's merged PRs. Not built: attaching the notes to a GitHub
pre-release, promotion to stable, checksums and signing (need release credentials
and your authorization), and a CI job running the tests.
