# The UI stopped applying snapshots after a window state change

Reported as "chats not loading" (#67). Reproduced live on a real Mac with the
diagnostics build (#65), 2026-11-15 12:49-12:53.

**What the log showed.** The last snapshot the UI applied was at 12:49:46. Four
seconds later the window deactivated and reactivated several times. From then on
every room selection reached the backend and core delivered each timeline
(`first_snapshot success=true`), but the UI applied nothing: no `ui_room_state`
events, so the clicked room never matched the UI's snapshot and the header showed a
spinner over a blank timeline. Sync itself stayed healthy (it went offline and
back to running within a millisecond at 12:52:10), and clicks, selections and
window events kept working.

**Cause.** The window receives everything through one loop that applied each
published snapshot with `update_in(...)` and did `if is_err() { break }`. A
single failed update (a window-state race) ended it silently and permanently,
leaving the UI frozen on its last snapshot while the backend carried on. That
fits "only after a state change" and is unrelated to the network.

**Fix.** The loop now ends only when the view or the backend is gone. If an
update fails while the view is alive it retries the latest snapshot every 50 ms
(about ten seconds at most, then stops and says so). It logs
`ui_snapshot_apply_retry` and `ui_updates_loop_end_*` so any future stop is
visible.

**Safety net (added after "Try again" did nothing on the frozen window).** Once
the loop is dead no button can recover the window, because every retry reaches the
backend and nothing reaches the screen (log at 13:16:56: `ui_room_retry`, selection
finished, `first_snapshot success=true`, no snapshot applied). A watchdog now checks
once a second whether the backend has published a revision the window has not
applied, waits half a second for the normal path, and otherwise applies the latest
snapshot directly (`ui_snapshot_pulled`). So a missed update or a dead loop heals
itself within about 1.5 s whatever caused it.

**Verified:** compilation, strict Clippy and the existing tests; the diagnosis
from the live log. **Not verified:** that this fixes it. The transient failure has not
been reproduced on purpose, so the loop's retry path has no automated test. Next
step: run this build for a while, switching windows and sleeping the Mac, and
check the log for `ui_snapshot_apply_retry` (it means the race happened and was
survived) and for continued `ui_room_state` events.
