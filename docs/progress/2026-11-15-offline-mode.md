# Saved conversations and a "no internet connection" notice

Requested 2026-11-15: when the connection is lost, keep the conversations readable
from what is saved and say there is no internet connection.

- **Saved messages.** The SDK keeps timelines in its local store, but opening a room
  first asks the server for its encryption state; offline that failed and the room
  retried instead of showing saved messages. If that request fails with a network
  error and the room's state is already stored locally, the room now opens from the
  store (`room_open_from_cache`). An unknown state is still retried, never assumed
  unencrypted.
- **Notice.** A slim strip under the room header, "No internet connection. Showing
  saved messages.", shown from the first failed sync until it reconnects
  (`Snapshot.offline`, `next_offline`). `--preview-offline` captures it.
- **Sending offline.** The composer stays enabled; messages queue in the SDK send queue
  and the queue is re-enabled when sync returns to running, so they send on
  reconnect.
- **Diagnostics.** `room_open_from_cache`; `sync_state_offline|running` already exist.

Verified: compilation, strict Clippy, 145 offline tests (banner on/off, the offline
state machine), a capture. Not verified: any of this on a real connection loss
(Wi-Fi off) with a live account, that saved messages really show for every room,
that queued messages send on reconnect, Linux and Windows. "Offline" here means the
homeserver cannot be reached, which is also what a server outage looks like; there
is no separate OS network monitor, so the notice appears when sync fails rather than
the instant the network drops.

Stacked on #65 (sync lifecycle events and room stream restart).
