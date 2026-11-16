# Release pull requests reuse develop's checks

## Built

- A `release/*` branch cut at develop's tip has the same commit as develop's
  push run, so its pull request showed every check twice (8 instead of 4)
  and ran format, lint and tests again on all three OSes.
- The `changes` job now looks up develop's push run of `Rust checks` for the
  pull request's head commit. If there is one, it waits for it (`gh run watch
  --exit-status`, at most 120 minutes): success marks the commit as tested,
  failure fails this run. The `check (...)` jobs then skip format, lint, tests
  and the release script tests, and still report success for the branch
  protection. The app build that only pull requests into main run still runs.
- Without such a run (a release branch with its own commits) nothing changes.

## Verified

- `actionlint` accepts the workflow.
- The run lookup returns develop's run 43906766685 for `f99bc84` (the
  #132/#133 commit) and nothing for an unknown commit.

## Not verified

- A release pull request running with this change: the next one is the first.
