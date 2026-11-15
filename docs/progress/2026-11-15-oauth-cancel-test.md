# OAuth cancel test matches the callback receiver (#84)

`oauth_cancel_and_wrong_state_discard_checkpoint` expected sign-in to fail
with `AuthError::Callback` after a wrong-state callback. The loopback
receiver deliberately ignores such a request, so a stray request cannot abort
sign-in, and the test waited out its 300 s receive timeout.

The test now checks the actual behaviour:
- the wrong-state request gets `400 Bad Request`;
- sign-in is still waiting half a second later;
- cancelling then ends it with `AuthError::Cancelled`;
- the checkpoint (keychain slot and store) is discarded and the listener is
  closed.

The plain-cancel case is unchanged.

## Verification

- Local MAS + Synapse (`run.sh oauth`): the test passes in 1.3 s, and
  `oauth_keychain_lifecycle_across_processes` still passes.
- No product code changed.
