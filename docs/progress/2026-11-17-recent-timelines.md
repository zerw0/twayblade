# Keep recent conversations loaded (#173)

## Built

- Core keeps the live timelines of the last 8 opened rooms per account
  (`RecentTimelines`, most recent first). Going back to one of them
  subscribes to the timeline already in memory, so its messages, and any
  history already loaded with Load Earlier, show at once; it is not rebuilt
  and does not fetch its first page again. The ninth room drops the oldest.
- Threads and jump-to-message timelines are not cached; they open as before.
- The cache lives in the `Account`, so signing out or switching accounts
  drops it.
- Integration test fix: the pagination test counted every row, which now
  includes room state rows; it counts the history messages instead.

## Verified

- Compile: strict workspace Clippy. Offline tests: unit test for the
  recency order and eviction; app and core suites pass.
- Local Synapse: `services` (all six tests) and `room-open` (6 tests) pass.
  The pagination test now reopens the room after loading all 90 messages and
  checks that the first ready snapshot already holds every row with the same
  IDs.

## Not verified

- Live account, and how it feels on a physical Mac when switching between
  busy rooms; memory use with eight large timelines open.
