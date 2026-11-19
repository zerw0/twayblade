# Voice channels on room version 12 servers (#176)

## Fixed
- Creating a voice channel failed with `400 M_UNKNOWN: Creator user … must not
  appear in content.users`. The voice channel's power-level override listed
  the creator at 100; room version 12 (now Synapse's default) gives creators
  unlimited power and rejects them in `users`. The entry is removed: older
  room versions already give the creator 100 through the private-chat preset.
  The real app was affected on any homeserver defaulting to room version 12,
  not only the test.

## Verified
- Local Synapse: `tests/integration/run.sh spaces` passes end to end
  (it stopped at "create voice channel" before).
- Compile, clippy, `twayblade-core` tests.

## Not verified
- A voice channel created on a production homeserver and joined from Element.
