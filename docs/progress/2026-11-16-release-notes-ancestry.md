# Release notes list only what is in the tag (#145)

## Fixed

- The drafted notes picked pull requests by merge date, so the v0.1.1-rc.1
  draft listed #138 (merged into develop after the release branch was cut)
  and the release and sync pull requests.
- With a tag for `--since` (as `build.yml` passes), `scripts/release-notes.py`
  now lists a pull request only when its merge commit is reachable from the
  new tag and not from the previous one, and skips `release/*` and `sync/*`
  branches. A date for `--since` still selects by merge date.

## Verified

- `python3 scripts/test_release_notes.py` passes, including a case with a
  PR merged after the cut and flow PRs.
- Run against the real history: `--since v0.1.0-rc.3 --until v0.1.1-rc.1`
  lists #132, #135, #137 and #140 only.
