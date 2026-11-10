# Compatibility evidence

Updated 2026-11-10. Records what has actually been run, per platform. Compile
checks, offline UI tests, isolated-server tests and live/physical acceptance are
listed separately. Earlier Iced/SwiftUI evidence was removed with those frontends.

| Check | macOS arm64 | Linux | Windows |
| --- | --- | --- | --- |
| fmt / Clippy (`-D warnings`) / unit tests | Pass locally (Rust 1.99; locked/all features; 26 tests) | Not run | Not run |
| CI (`.github/workflows/check.yml`) | Never observed passing | Never observed passing | Never observed passing |
| GPUI build and launch | Pass; ad-hoc `.app` launches | Not run | Not run |
| Offline previews and Kit interaction tests | Pass; light/dark workspace, Home, profile, sign-in, details and 10k captures; Kit interaction/layout tests | Not run | Not run |
| Isolated Synapse: encrypted send/receive, same-device store reopen | Pass | Not run | Not run |
| Live password sign-in, room list, history | Operator reports GPUI sync/messaging working; homeserver not supplied | Not run | Not run |
| Live GPUI send, relaunch restore (Keychain), logout | Operator reports send and Keychain restore; isolated cross-process lifecycle including logout/store deletion passes | Not run | Not run |
| `.well-known` discovery against real servers | Pending | Not run | Not run |
| OAuth / legacy SSO | Not implemented | — | — |
| Element interoperability (E2EE) | Pending | Pending | Pending |
| IME, VoiceOver/screen reader, keyboard focus | Pending | Pending | Pending |
| Element Call | Not implemented | — | — |

## Notes

- Isolated tests (`tests/integration/`) use generated accounts on Synapse
  v1.164.2, `matrixclient.test`, `https://127.0.0.1:18448`. The encrypted
  interoperability check is SDK-to-SDK. A separate macOS lifecycle test uses
  real Keychain storage and the normal auth implementation in separate processes.
  Neither establishes physical UI automation or Element interoperability.
- The 10,000-message preview is a virtualization smoke test, not a benchmark.
- Theme contrast: white on outgoing blue 5.59:1 (light) / 4.79:1 (dark);
  secondary text on sidebar 5.13:1 / 5.89:1. Pairwise only; not a full audit.
- GPUI controls are custom-rendered. No AppKit widgets, native accent colour,
  vibrancy or complete accessibility is claimed.

## Current limitations

- Native sliding sync is required; servers without it are unsupported.
- One account; one running instance per OS user.
- Logout deletes the local store after revoking the device. If remote revocation
  fails, the session and store are kept so logout can be retried.
- Token refresh, verification, recovery and key backup are not implemented.

## Isolated-server checks

Run with `bash tests/integration/run.sh services` (all platforms) and
`bash tests/integration/run.sh keychain` (macOS). All passed on macOS arm64
against Synapse v1.164.2 with generated accounts only:

| Test | Covers |
| --- | --- |
| `encrypted_delivery_profile_and_same_device_store_restore` | SDK-to-SDK E2EE, profile fetch, encrypted store reopen |
| `keychain_session_lifecycle_across_processes` | Normal auth code with a scoped Keychain slot: login, restore same device in a new process, failed-login cleanup, logout deletes credential and store |
| `timeline_metadata_and_send_queue_actions` | Replies, reaction toggle, message kinds, thread count, failed send retry/cancel (plaintext room) |
| `members_typing_and_read_receipts` | Member names/roles, typing start/stop, fully-read marker, receipt seen by another client |
| `pagination_preserves_message_identity` | 90 messages paginated to history start with stable unique IDs |
| `room_list_service_tracks_recency_and_details` | RoomListService reordering, name/topic refresh, leave/removal |

## Operator reports (not independently reproduced)

- GPUI sign-in, sync and sending work on the operator's account; homeserver not
  recorded. Relaunch restored the session after a Keychain approval prompt.
- Ad-hoc signed development builds change their code hash on every build, so
  macOS may ask for Keychain access again. Stable signing is release work.

## Remaining gates

- CI on macOS/Linux/Windows has never been observed passing (M0 exit).
- Physical checks: logout from the GUI, trackpad pagination, IME, VoiceOver.
- Element interoperability, verification, key backup/recovery, attachments,
  thread view, large-account performance.
