# Compatibility evidence

Updated 2026-11-10. macOS is the primary acceptance platform.

| Check | macOS arm64 | Linux | Windows |
| --- | --- | --- | --- |
| Workspace check/build | Passed locally | Not run | Not run |
| Rustfmt | Passed | Shared source | Shared source |
| Clippy with warnings denied | Passed, all targets | Not run | Not run |
| Unit tests | 16 passed | Not run | Not run |
| Native window | Launched; own-window capture inspected | Not run | Not run |
| Anonymous discovery | User screenshot: voltized.de | Not run | Not run |
| Password sign-in | User screenshot: signed-in screen | Not run | Not run |
| Same-device relaunch / logout cleanup | Pending physical acceptance | Not run | Not run |
| Room synchronization | User screenshot confirms live room list | Not run | Not run |
| OAuth / legacy SSO | Not implemented | Not implemented | Not implemented |
| Timeline/history/send queue | History confirmed by user screenshot; live sending pending | Not run | Not run |
| E2EE interoperability | Pending | Pending | Pending |
| Element Call | Not implemented | Not implemented | Not implemented |

User screenshots verify discovery and password sign-in UI on voltized.de, not the
entire session persistence or encryption lifecycle. OAuth metadata was unconfirmed.

## Verification performed

- Project Nix/direnv shell: workspace check/build/run, formatting, Clippy with
  warnings denied, and workspace tests.
- Initial eight tests: HTTPS validation, password-event Debug redaction, logout UI cleanup,
  stale room/timeline/queue results after logout, and per-room draft preservation
  and pending-send tracking across navigation; identity/time-based message grouping.
- Native credential-free preview captured only its own window with Iced. It did
  not read Keychain or connect to an account. Inspected the corrected 2240 x 1520
  capture: proportional bold headings, compact sidebar/search and no empty-state clipping.
- Populated offline preview launched and captured: fixed sidebar initials/names,
  long room-name wrapping, unread badge and message/composer layout checked.
  All data in this preview is explicitly labeled synthetic and sending is disabled.
- Nix syntax/CI YAML parsing passed. CI is a draft and has not run. Local Rust is
  1.99; declared minimum 1.96 remains independently unverified.

## Current limitations

- SDK unified sync requires native sliding sync; no legacy fallback is implemented.
  Cached rooms may appear offline. Connected status does not establish full sync.
- Selected-room subscriptions and SDK timelines now supply history and live
  updates. Initial and explicit history pagination load 30 events; plain text goes
  through the SDK send queue, with local echo and sending/sent/failure labels.
  Missing-key and redaction placeholders are shown. SDK encryption behavior is
  retained, but live message/E2EE interoperability has not been verified.
- Spaces are excluded from the conversation list itself and now appear in a separate navigation rail. Attachments,
  thread UI, message actions, send-failure retry/cancel, selectable text,
  virtualization and pagination scroll anchoring remain pending.
- Controls are custom-rendered Iced, with macOS San Francisco and portable fallback.
  Native menus, VoiceOver, IME, physical keyboard, appearance transitions and
  window lifecycle remain acceptance tasks. No AppKit/vibrancy claim.
- The OS credential store holds one serialized Matrix session and database
  passphrase, never the user's password. SDK store encryption does not imply
  encryption of all filesystem metadata. No plaintext credential fallback.
- Pre-login storage precedes device creation. Failed final persistence triggers
  best-effort remote logout. Interrupted sign-in and concurrent instances remain
  release blockers.
- Logout removes the secret record after server revocation; encrypted store files
  remain. Local deletion retries are supported within the current process after
  revocation. Crash/revoked-session recovery is pending.
- Token refresh, verification, recovery, key backup and restored-session server
  validation are pending. No message/call encryption acceptance is claimed.

## Earlier environment restriction

The initial sandbox denied Nix daemon and crates.io access. A cached direnv shell
later allowed local checks, and approved Nix execution enabled dependency fetches
and native previews. Those initial blockers no longer describe the current build.

## Conversation design follow-up

User screenshot confirms live message history on macOS. Sender display names,
Matrix-ID hover labels, local timestamps, consecutive-message grouping and softer
selection are implemented. Follow-latest and Jump to Latest are wired; live
scrolling behavior and pagination anchoring still need physical acceptance.
The new populated offline preview launched and was visually inspected. The Cinny
reference, planned space rail/member inspector/rounded composer and future native
semantic/accent-color adapter are documented in the design plan. Current color
values are approximations with system light/dark tracking.

## Spaces navigation increment

SDK SpaceService drives a joined-space rail, Home, selected-space
headings and room filtering. SDK-provided descendants are expanded with a visited
set, so cycles terminate and shared rooms do not duplicate unread counts. Filters
use IDs, not display names. Removed-space selection falls back to Home; hidden-room
drafts are retained in memory. Stale account updates and logout cleanup are guarded.

Five added tests cover cyclic/shared descendants, selection/draft preservation,
unknown selection, unread aggregation and stale updates. Total: 16 passing tests.
The offline parent-space preview was launched and inspected in a captured native
window: nested markers, unread badge and filtered conversations display correctly.

Live joined-space sync remains unverified. The SDK's navigation filters expose
two levels, and may depend on cached state completeness; this is not a complete
server hierarchy/discovery implementation. Create/edit/join/leave/invites,
permission-aware management and deeper navigation remain pending. No membership
writes were performed during implementation or preview verification.

## Home filtering and visual redesign

Home now excludes rooms in the union of SDK-provided space descendants. It stays
in a loading state until initial space filters arrive, and clears a selection that
becomes hidden while retaining its draft. Three regression tests cover exclusion,
initial loading and membership changes. The total is now 16 tests.

San Francisco is read from the installed macOS font at startup and is not bundled
or redistributed. Vector icons are locally authored SVG assets rendered by Iced;
these are not SF Symbols. Compact rows, account-menu disclosure, rounded content
surface and composer replace the earlier bulky layout. Actual semantic system
colors/vibrancy remain unimplemented, and controls remain custom-rendered Iced.

Redesigned Home and selected-space previews were launched and their own native
window captures inspected. Home displays the standalone sample room while hiding
all three space-owned samples. The selected-space view retains its own rooms.
A San Francisco bold-face fallback discovered during QA was corrected by using
semibold emphasis. Formatting and Clippy pass; all 16 tests pass. Live membership
completeness, native accent/vibrancy and accessibility remain separate checks.
