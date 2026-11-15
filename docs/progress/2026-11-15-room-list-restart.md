# Chats not loading after a state change

Chats not loading after a state change (2026-11-15, `fix/chats-not-loading`):
reported only as "after a state change"; no cause has been reproduced. Found by
reading the code: the backend ran the room-list stream once per login, and the
stream ends for good on several conditions (closed channels, a login state
change), after which nothing restarted it and the list stayed on its last
status. The backend now restarts it with backoff while the login is current and
active, and the diagnostics log records sync state changes, restart attempts,
why the stream ended, and the room and timeline counts the UI holds (see
DEVELOPMENT.md). Verified by compilation, strict Clippy and unit tests of the
log allowlist only. Not verified: that this is the reported failure (it has not
been reproduced), or behaviour across a real sleep, network change or token
refresh. Next step: reproduce with `TWAYBLADE_LOG=1` and read the new events.
