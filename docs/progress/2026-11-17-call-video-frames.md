# Calls: video that never appeared (#200)

## Fixed
- A video tile is rebuilt when its track's frame slot changes. A track
  consumed again under the same id (a resubscription after a reconnect) gets a
  new `LatestFrame` and retires the old one; the tile kept the retired handle
  and stayed empty for the rest of the call. `LatestFrame::same` compares slots.
- `LatestFrame` drops a frame only when it is slightly older (out of order).
  One more than 1 s older means the sender's clock restarted (camera off and
  on, reconnect) and is shown; before, the video froze until the new clock
  passed the old one.

## Verified
- Compile, clippy; `twayblade-media` unit test
  `a_restarted_sender_clock_does_not_freeze_the_video`; `twayblade` tests.

## Not verified
- A live call on macOS with Element Call reproducing either case.
- The reported case of the local Mac camera not showing is not explained by
  these; it needs `TWAYBLADE_LOG=1` output if it recurs.
- Element Call's shared screen shown cropped on macOS: under investigation,
  waiting for a screenshot.
