# Native Rust Matrix client — plan

Updated 2026-11-10. Status: M0 in progress; no milestone exit claimed.

## Goal

A simple, native desktop Matrix client. macOS is the primary UX and acceptance
platform; Linux and Windows build from the same source and get their own release
gates. Required for 1.0: E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and
direct/group calls through Element Call.

## Decisions

| Area | Decision |
| --- | --- |
| Frontend | One GPUI + GPUI Kit frontend for all three OSes. The earlier Iced and SwiftUI/UniFFI frontends were deleted (2026-11-10); they remain in git history only. |
| Protocol | matrix-sdk / matrix-sdk-ui, pinned. Use SDK sync, timelines, stores and crypto; never reimplement protocol state. |
| Sync | Native simplified sliding sync is a **minimum server requirement**. No legacy `/sync` fallback; document this as a compatibility limit. |
| Server entry | Users type a server name (`matrix.org`); `.well-known` discovery resolves it. Only HTTPS homeservers are accepted, including after delegation. |
| Storage | SDK SQLite stores, encrypted with a passphrase held in the OS credential store. No plaintext fallback. Logout and failed sign-in delete the local store. |
| Instances | One running instance per OS user, enforced by a lock file in the app data directory. |
| Calls | Element Call in a separate, capability-restricted web runtime (Wry candidate). Chat UI stays native. Legacy `m.call.*` is out of scope for 1.0. |
| Toolchain | Rust minimum = the version actually built and tested (1.99). Nix shell is optional. |

## 1.0 scope

In:
- One account; password, OAuth/OIDC and legacy SSO; refresh, restore, logout.
- Rooms and DMs: history, pagination, send queue, replies, edits, reactions,
  redactions, typing, read receipts; images/files with encrypted transfer.
- E2EE: cross-signing, device verification, key backup and recovery, visible
  undecryptable-message state. Never silently downgrade encryption.
- Spaces: navigation, nested hierarchy, join/leave. Threads: read, reply, unread.
- Unicode emoji picker and reactions.
- Direct and group calls via Element Call: ringing, mute, device choice, screen share.
- Notifications, unread counts, keyboard shortcuts, system light/dark.

Deferred past 1.0 (cut from the earlier plan):
- Stickers and sticker packs; custom image emoji.
- Alternative timeline styles (IRC, minimal, Element-like) and curated palettes
  (Gruvbox, Solarized, …). 1.0 ships one layout with System/Light/Dark.
- Inline audio/video playback (save/open only in 1.0).
- Space management beyond join/leave (create, edit, permissions UI).
- Multi-account, widgets, bots, moderation tooling, legacy calls.

## Milestones

Estimates assume one full-time developer. Re-estimate after M1.

**M0 — foundation (in progress, ~2 weeks).** Password login, OS-secret
persistence, same-device restore across process restarts, logout cleanup,
single-instance lock, live room list and timeline, plain-text sending, verified
CI on macOS/Linux/Windows. Exit: live account checks pass on macOS; CI green on
all three OSes. Compilation alone does not pass M0.

**M1 — authentication (2–3 weeks).** OAuth/OIDC (discovery, registration or
configured client ID, PKCE, callback validation) and legacy SSO (IdP selection,
login token) in the system browser. Token refresh, revoked/expired session
recovery. Exit: all flows pass against isolated servers, including cancellation,
invalid callback and relaunch. Rotated credentials persist atomically.

**M2 — encrypted messaging (5–7 weeks).** Message actions, attachments, emoji
and reactions, verification, backup and recovery. Exit: interoperates with
Element in encrypted rooms; reconnect without duplicate sends; fresh-install
recovery; missing-key and revoked-device behavior; no secrets or bodies in logs.

**M3 — spaces and threads (3–4 weeks).** Exit: nested/private/restricted spaces,
cycles and multi-parent rooms behave like Element; threads paginate and keep
unread/receipt state across clients without duplicate rendering.

**M4 — calls (5–7 weeks).** Start with a macOS call-runtime spike, then the
complete lifecycle and Linux/Windows. Exit: encrypted direct and group calls with
Element on each OS, including denied permissions, device removal, network change
and screen sharing (Wayland and X11). Transport TLS is not media E2EE.

**M5 — release (3–5 weeks).** Notifications, accessibility pass, migrations,
packaging, signing, updates. Exit: clean-machine install, upgrade with session
retained, uninstall, notification activation. Signed artifacts need release
credentials and explicit authorization.

## Known technical debt (fix during M0/M1)

- `core` returns English UI strings (`&'static str` statuses and errors). Return
  typed states/errors and map them to text in the frontend.
- `gpui/backend.rs` mirrors core types field-by-field and blocks on its own Tokio
  runtime from GPUI workers. Use core types directly.
- The room list is rebuilt from `client.rooms()` on every change. Move to the
  sync service's `RoomListService` (incremental, server-sorted) before large accounts.
- Message text is escaped and passed through Kit's Markdown renderer to get
  selectable text. Replace with a plain selectable-text element when available.
- The credential entry name `matrix-client.development` is hard-coded; give
  release builds their own entry name.

## Progress

Done (macOS arm64, offline and isolated-server verification only):
- Cargo workspace (`core`, `platform`, `gpui`), Nix shell, pinned dependencies.
- Server-name discovery via `.well-known`, HTTPS enforcement after delegation.
- Password sign-in, OS credential store, encrypted SDK store, restore, logout
  with store deletion, failed sign-in cleanup, single-instance lock.
- GPUI workspace laid out after the approved mockup (2026-11-10): space rail
  with unread badges, sidebar with Rooms / Direct messages sections and account
  row, room header (avatar, topic, encryption lock), bubble timeline with day
  separators and undecryptable/deleted states, rounded composer, details panel
  (Cmd-I) and profile panel (Escape closes). Two-step sign-in (server, then
  browser/password). Controls for unbuilt features are omitted, not faked.
- Home (rooms outside all spaces), room search, recency-sorted rooms, per-room
  drafts, pagination, plain-text send queue, read-only profiles.
- Offline previews, incl. a 10,000-message virtualization smoke fixture.
- Isolated Synapse fixture: SDK-to-SDK encrypted send/receive and same-device
  store reopen pass (`tests/integration/`).

Not yet verified:
- Live GPUI login → sync → send → relaunch → logout with Keychain.
- CI on any host; Linux and Windows builds and runs.
- Element interoperability; IME, VoiceOver, keyboard focus; performance numbers.

Evidence log: `docs/compatibility/STATUS.md`. Architecture: `docs/architecture/GPUI.md`.

## Workspace

```text
crates/core/        Matrix services, auth, sync, timelines (owns all Matrix state)
crates/platform/    OS credential store (later: notifications, deep links)
crates/gpui/        GPUI + GPUI Kit frontend
macos/              Development .app bundle helper
tests/integration/  Isolated Synapse fixture
docs/               Architecture and compatibility evidence
```

Add `call-host` (Element Call runtime) when M4 starts, not before.

## References

- https://github.com/matrix-org/matrix-rust-sdk
- https://github.com/longbridge/gpui-kit
- https://github.com/element-hq/element-call
- https://github.com/tauri-apps/wry
