# Room list recovery after network loss and token refresh (#67)

Branch `fix/room-list-recovery`. Regression tests only; no product code
changed, because both reproducible cases already recover.

## Background

The freeze reported live on 2026-11-15 was the UI loop ending on its first
failed update; #75 (merged) keeps that loop alive and adds a watchdog. What
remained open was whether the core room list itself recovers after sleep, a
network change and a token refresh.

## Tests added

- `room_list_recovers_after_a_long_network_loss` (`run.sh room-open`): the
  fault proxy drops every request (new `all` route) until the list reports
  Offline, a second device creates a room meanwhile, and the outage lasts
  another 45 s so the SDK backs off. After the network returns, the same
  room-list stream reports Connected and contains the new room. The stream
  never ends.
- The OAuth lifecycle test (`run.sh oauth`, MAS with 5 s access tokens) now
  runs the room list across two expired tokens: each time a request
  refreshes the token, the list picks up a room created afterwards, and the
  session stays Active.

Observed: a running sliding-sync long poll can hold one request open past
the token lifetime, so the refresh happens on the next request, not on a
timer. That is fine for the list; it is not a failure mode.

## Verification

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy.
- Local Synapse: `run.sh room-open` 5/5 (77.6 s), including the new test.
- Local MAS + Synapse: `oauth_keychain_lifecycle_across_processes` passes
  with the refresh check.
- Separate, existing failure: `oauth_cancel_and_wrong_state_discard_checkpoint`
  times out on its wrong-state case. The loopback receiver deliberately
  ignores a callback with the wrong state (a stray request must not abort
  sign-in) and the test still expects `Callback`. Tracked separately; not
  related to the room list.

## Not verified (needs a physical Mac)

Real sleep and wake, Wi-Fi changes and a long-running build on the live
account. Checklist:

1. Launch with `TWAYBLADE_LOG=1` and leave a busy room open.
2. Sleep the Mac for at least 10 minutes, wake it, and wait 30 s.
3. Send a message to yourself from Element on the phone; it should appear in
   the room list and timeline without clicking anything.
4. Switch Wi-Fi off for 2 minutes, then on; repeat step 3.
5. Send the log folder entries containing `sync_`, `room_stream_`,
   `session_state_` and `ui_snapshot_pulled` (event names and counts only;
   the log holds no message text or tokens).
