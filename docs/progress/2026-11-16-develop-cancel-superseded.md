# Develop cancels superseded check runs

## Built

- On develop, a newer push now cancels the older `Rust checks` run instead
  of queueing behind it. Before, each merge into develop waited for the
  previous merge's full run on all three OSes (seen on 2026-11-16: the run
  for the 0.1.1-rc.1 version commit sat pending behind the run for #135).
- main is unchanged: its runs are never cancelled, so a failure on any
  commit stays visible.
- The newer develop run tests the whole branch, so nothing goes untested;
  only the separate result for the intermediate commit is lost.
- A release pull request cut from a develop commit whose run was cancelled
  fails its wait (#134); cut it again from develop's tip.

## Verified

- `actionlint` accepts the workflow.

## Not verified

- Two quick merges into develop with this change.
