# Compatibility evidence

Updated 2026-11-10. Records what has actually been run, per platform. Compile
checks, offline UI tests, isolated-server tests and live/physical acceptance are
listed separately. Earlier Iced/SwiftUI evidence was removed with those frontends.

| Check | macOS arm64 | Linux | Windows |
| --- | --- | --- | --- |
| fmt / Clippy (`-D warnings`) / unit tests | Pass locally (Rust 1.99) | Not run | Not run |
| CI (`.github/workflows/check.yml`) | Never observed passing | Never observed passing | Never observed passing |
| GPUI build and launch | Pass; ad-hoc `.app` launches | Not run | Not run |
| Offline previews and Kit interaction tests | Pass; dark-mode captures of the new workspace, Home, profile and sign-in inspected. Light mode not captured | Not run | Not run |
| Isolated Synapse: encrypted send/receive, same-device store reopen | Pass | Not run | Not run |
| Live password sign-in, room list, history | Seen in the deleted Iced UI only; GPUI pending | Not run | Not run |
| Live GPUI send, relaunch restore (Keychain), logout | Pending | Not run | Not run |
| `.well-known` discovery against real servers | Pending | Not run | Not run |
| OAuth / legacy SSO | Not implemented | — | — |
| Element interoperability (E2EE) | Pending | Pending | Pending |
| IME, VoiceOver/screen reader, keyboard focus | Pending | Pending | Pending |
| Element Call | Not implemented | — | — |

## Notes

- The isolated test (`tests/integration/`) is SDK-to-SDK. It keeps the session
  in test memory, so it proves neither Keychain persistence nor `auth::restore()`.
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
