# Native Rust Matrix client

Date: 2026-11-10. Status: M0 in progress.

## Goal

A simple native desktop Matrix client for macOS, Linux, and Windows, with E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and direct/group calling through Element Call.

macOS is the primary product platform, per the user's 2026-11-10 direction. Design and validate interactions on macOS first while maintaining a portable core and Linux/Windows frontend and Linux/Windows release gates.

## Architecture decisions

| Area | Initial choice | Gate |
| --- | --- | --- |
| UI | GPUI + GPUI Kit, one Rust frontend on macOS/Linux/Windows | Input, accessibility, large timelines, native execution and call-window acceptance on each OS |
| Protocol | matrix-sdk and matrix-sdk-ui | Pin compatible releases; verify server capabilities |
| Async | Tokio | Never block UI; cancel account tasks on logout |
| State | SDK SQLite state/crypto stores | Store encryption and migration tests |
| Secrets | OS credential stores | Keychain, Windows credentials, Linux Secret Service; no plaintext fallback |
| Calls | Pinned Element Call assets and SDK widget bridge | Actual encrypted media on all desktop platforms |
| Call runtime | Evaluate Wry in a separate call window | WebKit/WebKitGTK/WebView2 compatibility; assess Chromium helper if needed |
| Tooling | Project Nix shell, Cargo workspace | Native OS build/test matrix |

Matrix application logic and the new shared GPUI frontend are Rust. The earlier SwiftUI/Iced frontends remain reference implementations during migration. Element Call includes web assets and a browser runtime. Fully web-free calling would be a separate implementation. Evaluate matrix-rust-rtc during M0 as a future alternative, without replacing the requested Element Call integration silently.

## First release scope

- One active account, persistent login, refresh and logout.
- OAuth/OIDC, legacy Matrix SSO with identity-provider selection, password login where advertised.
- DMs and rooms: history, pagination, send queue, attachments, edits, reactions, replies, redactions, typing, receipts.
- Member/account profile pages with real Matrix identities, display names and room context; build avatar/edit/verification actions as separate working increments.
- E2EE: cross-signing, device verification, encrypted key backup, recovery and visible missing-key handling.
- Spaces: nested/private spaces, membership, discovery, join/leave, permission-aware management.
- Attachments: images, files, audio and video; file picker, drag/drop and clipboard image paste; encrypted upload/download in encrypted rooms, previews, save/open and visible transfer progress/failures.
- Stickers: render and send Matrix sticker events; picker with a documented pack source, favorites and graceful unsupported-format handling. Establish encrypted-room behavior and Element interoperability before enabling sticker sending there.
- Emoji: native composer picker, Unicode emoji insertion, searchable shortcodes, skin tones, recent/favorite emoji and message reactions. Custom image emoji are a separate compatibility extension, not implied by Unicode support.
- Threads: history, reply, pagination, unread state, receipts and notifications; no duplicate root/reply rendering.
- Audio/video direct and group calls using modern MatrixRTC and Element Call, incoming ringing, device selection, mute, screen sharing and reconnect.
- Desktop notifications, unread counts, shortcuts, light/dark mode and installable packages.
- Appearance preferences: selectable Element-like, IRC, minimal and chat-bubble styles; selectable color schemes independent of layout.

Deferred: multi-account, arbitrary widgets, bots, advanced moderation/server administration. Legacy `m.call.*` calling is a separate compatibility feature; modern direct calls do not prove it works.

## Interface

Use a macOS-first visual language: compact type and controls, restrained blue actions, system light/dark appearance, standard window decorations, comfortable desktop density and progressive disclosure. The sign-in window starts at 640 × 700 logical pixels with a 360-pixel content column, staged server/account entry, human-readable errors and no protocol-flow dump.

The shared GPUI conversation UI follows Mac sidebar/split-view conventions. GPUI Kit supplies custom-rendered controls, semantic themes and interaction behavior; menus, shortcuts, window lifecycle and accessibility require platform acceptance. Actual AppKit widgets or Liquid Glass need explicit platform integration. See `docs/architecture/MACOS_UI.md`.

Use the user's Cinny screenshot as the information-layout reference: space rail, compact rooms, room header, grouped timeline, rounded composer and optional members/details inspector. Adapt with macOS typography, rounded selections and semantic system colors. Use GPUI Kit semantic theme tokens and system light/dark appearance. Native macOS system accent/material matching is a separate platform task. Verify live appearance/accent changes and accessibility preferences separately on each frontend.

Space rail → room/DM list → conversation → optional thread/details panel. Collapse navigation on narrow windows. Default to conversations, reveal details on demand, and retain a return-to-call control during navigation. Provide guided verification/recovery and actionable errors. Keep technical diagnostics out of normal product flows.

### Selectable chat styles and color schemes

Provide four timeline styles through Appearance preferences:

| Style | Presentation |
| --- | --- |
| Element-like | Left-aligned room timeline, avatar gutter, grouped sender headers, timestamps and message actions; inspired by Element Desktop's density. |
| IRC | Compact rows with a narrow timestamp/name column; minimal avatars and decoration; readable alignment for busy rooms. |
| Minimal | Quiet left-aligned text with restrained metadata, separators and spacing; show sender changes clearly. |
| Chat bubbles | Incoming/outgoing rounded bubbles, grouped sender metadata and alignment similar to the current macOS frontend. |

Keep all styles compatible with threads, replies, edits, reactions, attachments,
stickers, delivery failures and decryption placeholders. Group by Matrix user ID,
not display name; expose exact message times without repeating a header on every
message. Style changes must preserve the selected room, drafts, reading position
and active thread. Start with an account-wide local preference; per-room overrides
are deferred until the global preference works. Provide density, text-size and
timestamp-visibility preferences without changing protocol behavior.

Color schemes are independent of chat style: System (default, follows macOS
appearance/accent), Light, Dark, and curated palettes including high contrast,
Gruvbox and Solarized. Each curated palette needs explicit light/dark variants
or a clear supported appearance. Use shared semantic color tokens for surfaces,
text, accent, selection, mentions, unread, errors and message bubbles. Map these
tokens into the shared GPUI theme; keep platform-specific window/menu conventions.
Do not force a palette to imitate Liquid Glass on Linux/Windows. Store preferences
locally without credentials, preview changes immediately, and offer Reset to System.

Acceptance: switch every style and palette on a populated conversation without
losing drafts/scroll position or hiding message actions. Check sender identity,
long names, wrapping, selection/copy, emoji, media, threads, keyboard focus,
VoiceOver/screen-reader labels and large histories. Verify readable contrast,
high-contrast/reduced-transparency settings and live System appearance/accent
changes on each target OS. Screenshots use explicit offline samples.

Delivery: implement the shared preferences/token model during M2, ship the four
working timeline renderers and curated palettes before M5 exit, and include
cross-frontend parity and persistence in release acceptance. Current bubble UI
is the first renderer; a style picker and additional schemes are not implemented.

## Milestones and acceptance

Estimates are engineering weeks for one experienced full-time developer, assuming test infrastructure and access to target OSes. Total initial envelope: 18–27 weeks; revise after M0.

### M0 — feasibility (2–3 weeks)

1. Scaffold Cargo workspace, project Nix shell, native window and native OS CI.
2. Build homeserver discovery and authentication probes. Test OAuth and legacy SSO in system browser.
3. Exchange encrypted messages between two test users; restart without changing device identity and recover history.
4. Exercise large synthetic timeline, pagination anchoring, selection/copy, multilingual IME, keyboard and screen reader.
5. Embed Element Call and establish a three-participant encrypted call spanning macOS, Windows and Linux. Test microphone, camera, screen sharing, permissions and reconnect.
6. Record dependency/license inventory, minimum OS/runtime/server requirements, sync capability behavior and UI/runtime decision.

Exit: verified messaging/session restoration and encrypted cross-platform call; documented frontend/bridge acceptance and call runtime decision. Compilation alone does not pass M0. If server sync requirements exclude older servers, publish that limitation or implement/test a supported fallback before claiming compatibility.

### M1 — foundation and login (2–3 weeks)

Implement service lifecycle, OS secrets, encrypted store setup, login UI, browser callbacks, refresh, restore and logout. OAuth: discovery, public-client registration or configured client ID, authorization code/PKCE and callback validation. Legacy SSO: discover `m.login.sso`, select IdP, browser redirect, exchange one-use login token. Password: only when advertised.

Exit: all three flows pass isolated server tests, including cancellation, invalid callback, expired session, relaunch and logout cleanup. Persist rotated credentials atomically; never create a new device on ordinary restart.

### M2 — encrypted messaging (4–6 weeks)

Implement room list, virtualized timelines, composer, local echoes/send queue, pagination, media, verification and recovery. Use SDK services; avoid a second independent event store.

Exit: Element interoperability, reconnect without duplicate sends, attachment encryption, same-device restart, verification, fresh-install backup recovery, missing-key and revoked-device behavior. No secrets or message bodies in routine logs. Protect SDK stores with supported encryption and OS-held secrets; document metadata/cache limits.

#### M2a — attachments and media

Use the SDK media/attachment APIs and authenticated media downloads. Add macOS-first file picking, drag/drop and clipboard image paste with filename, size and thumbnail confirmation. Support image/file/audio/video events, image previews, explicit open/save actions, progress, cancellation and retry. Respect homeserver limits and avoid loading large files fully into the UI. Treat received filenames and content as untrusted; never execute or automatically open downloads. Define cache limits and local decrypted-file cleanup.

Acceptance: send and receive each supported type with Element in plain and encrypted rooms; verify encrypted uploads and thumbnails are not exposed as plaintext media, confirm integrity/decryption, and test interrupted upload/download, denied file access, oversized files, unknown MIME types and restart behavior. Inline audio/video playback may follow basic receive/save support, but must have explicit release scope and codec/platform coverage before it is claimed.

#### M2b — emoji and reactions

Add a keyboard-accessible emoji picker next to the composer, Unicode insertion at the cursor, shortcode search, skin-tone selection and local recents/favorites. Integrate with the macOS character picker where feasible. Emoji reactions have add/remove/toggle and counts with sender details. Keep custom image emoji outside this baseline until a documented interoperability target is selected.

Acceptance: multi-codepoint emoji, flags, variation selectors and skin tones survive composition, sending, rendering and copy/paste; reactions synchronize with Element, toggle without duplicates and handle redaction. Check keyboard focus, VoiceOver and font fallback on target OSes.

#### M2c — stickers

Render received Matrix sticker events with bounded image decoding and accessible fallback text. Define an interoperable pack source before building pack management; provide a picker with previews, favorites and attribution where required. Do not embed a general widget/integration manager solely to provide stickers. Confirm SDK/spec handling in encrypted rooms and fail clearly if unsupported; never send an unencrypted sticker into an encrypted room as a fallback.

Acceptance: send/receive with Element, encrypted-room behavior, missing/deleted media, oversized images, unsupported animation and offline cached stickers. Sticker pack installation/removal and any nonstandard pack formats need an explicit compatibility record.

### M3 — spaces and threads (3–4 weeks)

Implement nested spaces, membership and room discovery/management; threads, pagination, unread/receipt semantics, notification routing and jump-to-thread.

Exit: private/restricted spaces enforce permissions; hierarchy handles cycles; threads update across clients, paginate and preserve unread/receipt behavior.

#### M3a — Matrix spaces

Add a space rail/sidebar section with Home for rooms outside spaces and per-space filtering. Home must exclude the union of all known space descendants, and wait for initial membership before showing rooms. Support joined spaces, nested hierarchy, room discovery, suggested rooms, unread aggregation and permission-aware create/edit/invite/join/leave/add/remove-room actions. Preserve rooms belonging to multiple spaces and distinguish separate rooms with the same name. Leaving a space must not implicitly leave all its rooms; explain and implement any separately requested room departures. Handle hierarchy cycles, inaccessible children, restricted joins and homeserver pagination.

Acceptance: compare hierarchy/membership updates with Element; test nested/private/restricted spaces, multi-parent rooms, duplicate display names, inaccessible children, cycles and paginated hierarchy. A room list that merely excludes spaces does not meet space support.

### M4 — complete calling UX (4–6 weeks)

Expand M0 proof into direct/group calls, incoming/outgoing lifecycle, ringing/decline/cancel/missed states, participant/device controls, screen sharing, permissions, return-to-call and reconnect. Test MatrixRTC transport discovery and missing-backend errors. Restrict widget capabilities and never expose account credentials to web assets.

Exit: encrypted direct and group calls interoperate with Element on every OS. Exercise denied permissions, device removal, network transition, hangup cleanup, screen sharing under Linux Wayland/X11, and multi-participant key changes. Do not equate TLS/SFU transport encryption with media E2EE.

### M5 — desktop release (3–5 weeks)

Complete notifications, accessibility, performance, crash-safe migrations, packaging, signing and update mechanism. Define supported architectures/OS versions after M0. Plan macOS app/DMG, Windows installer and Linux packages based on runtime results.

Exit: clean-machine install, launch, browser callback, upgrade with session retention, uninstall, notification activation and real media checks. Signed/notarized artifacts and updates require release credentials/authorization; unsigned CI builds do not establish release acceptance.

## Workspace boundaries

```text
macos/               GPUI bundle helper and retained SwiftUI reference
crates/bridge/       Retained UniFFI bridge for the SwiftUI reference
crates/gpui/          Primary shared GPUI + GPUI Kit frontend
crates/desktop/       Retained Iced reference frontend
crates/core/          Matrix services and account lifecycle
crates/platform/     OS credentials, notifications, links, permissions
crates/call-host/    Element Call runtime and restricted bridge
tests/integration/   Isolated service tests when implemented
tests/interoperability/  Element/platform acceptance records
docs/architecture/   Decisions and threat boundaries
docs/compatibility/  Proven server/platform/runtime matrix
```

Create platform/call-host crates when they contain working implementation, not placeholder APIs. Core owns client, sync, stores and account tasks; desktop issues commands and consumes updates. Use bounded channels/streams and explicit task cancellation. Call host receives only scoped capabilities over authenticated local IPC.

## Validation environment

Create isolated reproducible services: password/legacy-SSO Synapse, OAuth/MAS configuration, MatrixRTC authorization service and LiveKit with required network connectivity. Use Element clients and separate test users for interoperability. Never use production credentials in fixtures.

CI: native macOS/Linux/Windows builds, fmt, Clippy and meaningful tests; isolated protocol integration job. Physical acceptance: media, permissions, accessibility, notifications and installers on each supported platform. Record results separately.

## Initial backlog

- [ ] M0-01: workspace, Nix shell, native window, reproducible build and CI.
- [ ] M0-02: homeserver login discovery (legacy types and OAuth metadata).
- [ ] M0-03: OS secrets, browser auth and same-device session restoration.
- [ ] M0-04: encrypted two-user messaging interoperability.
- [ ] M0-05: Element Call runtime and cross-platform media spike.
- [ ] M0-06: UI input/accessibility/performance spike and architecture decisions.
- [ ] M2a: attachment/media picker, encrypted transfers, previews and failure/retry checks.
- [ ] M2b: Unicode emoji picker/shortcodes/skin tones and interoperable emoji reactions.
- [ ] M2c: Matrix sticker rendering/sending and a documented compatible pack source.
- [ ] Appearance: Element-like, IRC, minimal and bubble styles; independent System/light/dark/curated color schemes, persisted preferences and parity checks.
- [ ] M3a: Matrix space navigation, hierarchy, membership and permission-aware management.

## Progress

GPUI migration implemented on `feature/gpui-migration`. M0 is still in progress; no complete
milestone exit is claimed. This section supersedes earlier frontend status notes.

### Implemented

- [x] Shared Rust core and OS credential integration, pinned Cargo/Nix inputs,
  HTTPS homeserver validation and password discovery/sign-in/restore/logout paths.
- [x] Primary GPUI + GPUI Kit frontend for the three desktop targets, replacing
  the split UI strategy in source. Kit 0.9.1 and matching dependencies are locked.
- [x] GPUI sign-in, room search, unread counts, joined-space rail, Home filtering,
  SDK timelines, plain-text send queue and explicit history pagination are wired.
- [x] Grouped sender headers using user identity and time/date boundaries, stable
  SDK message IDs, selectable message text and virtualized sender-group scrolling.
- [x] Room-owned composer state preserves draft/cursor/undo state across navigation.
  Fixed delayed input events writing to the wrong room; account changes clear it.
- [x] Readonly sender/account profiles use SDK global profile data and available
  cached room context, with copy ID, retry and stale-request/account guards.
- [x] Semantic GPUI theme, vector icon family, system fonts, platform decorations,
  native menu/shortcut wiring and standard macOS development bundle helper.
- [x] Standard macOS/Linux/Windows DEVELOPMENT.md, with a separate optional Nix
  section. Earlier Iced/SwiftUI frontends retained as references.
- [x] Isolated, pinned, loopback-only TLS Synapse fixture with generated accounts
  and ignored local state; normal application credentials are never used by it.
- [x] Repository contribution rules require a dedicated branch per feature and
  worktrees for concurrent work; local AI folders/installed skills are ignored.
- [x] User-requested UI/UX Pro Max skill installed repo-locally at the recorded
  revision and applied to relevant design/accessibility review; GPUI Kit supplies
  stack-specific implementation guidance.

### Verified

- macOS arm64: primary GPUI build/run and ad-hoc packaged app launch pass. Own
  rendered-content captures inspected for grouped chat, Home, profiles, sign-in
  and a 10,000-message virtualized fixture. Preview data is explicitly synthetic.
- 24 Rust workspace tests pass with all features, including two actual GPUI Kit
  control-interaction tests for draft/navigation and profile opening/closing.
  Formatting and all-feature Clippy with warnings denied pass locally.
- Isolated SDK/application-service test passed real profile fetching, unified
  sync, encrypted send queue/wire events, recipient decryption, original-device
  encrypted-store reopening and earlier-history decryption. Successful runs revoke
  only their generated test sessions.
- Theme token contrast was measured for the main outgoing-bubble and secondary
  text pairs. Full accessibility/input/appearance acceptance remains separate.

### Remaining acceptance and next work

1. Verify the normal GPUI login → live room/timeline sync → sending → restart /
   logout path with dedicated accounts and OS secret storage. The isolated test
   retains the SDK session in test memory; it does not prove Keychain persistence,
   a separate-process restart, crash recovery or normal auth restoration.
2. Add OAuth/OIDC and legacy browser SSO, refresh/revoked-session recovery and
   concurrent-instance protection. Existing production credentials must be retained.
3. Complete device verification/cross-signing, backup/recovery and Element
   interoperability. SDK-to-SDK encrypted delivery is only the first check.
4. Complete native Linux/Windows builds/runs and CI, physical IME/keyboard /
   accessibility, appearance transitions, minimum OS/Rust versions, measured
   timeline performance and live pagination anchoring.
5. Implement the Element Call/MatrixRTC runtime/media spike; message encryption
   does not establish media E2EE. Legacy calling is a separate compatibility target.
6. Continue attachments, emoji/reactions, stickers, full space management, threads,
   selectable chat styles/color schemes, notifications and release packaging.
   Those workstreams above remain planned, not completed.

Earlier user screenshots prove live password login, room loading and history in
the Iced reference only. See `docs/compatibility/STATUS.md` for historical evidence
and `docs/architecture/GPUI.md` for the current architecture and limits.

## Primary references

- https://github.com/longbridge/gpui-kit
- https://gpui.rs/

- https://github.com/iced-rs/iced
- https://github.com/matrix-org/matrix-rust-sdk
- https://docs.rs/matrix-sdk-ui/latest/matrix_sdk_ui/
- https://docs.rs/matrix-sdk/latest/matrix_sdk/authentication/oauth/index.html
- https://docs.rs/matrix-sdk/latest/matrix_sdk/authentication/matrix/struct.MatrixAuth.html
- https://github.com/element-hq/element-call
- https://github.com/tauri-apps/wry
- https://github.com/element-hq/matrix-rust-rtc

## Earlier Apple-design refinement and Home semantics (Iced reference)

The user supplied the apple-design skill as design guidance:
https://github.com/emilkowalski/skills/blob/main/skills/apple-design/SKILL.md
Applied its guidance on typography hierarchy, restrained surfaces, familiar
controls and immediate feedback to the native Iced implementation. Its web
examples do not authorize replacing the Rust interface with a web UI.

Home now means rooms outside every known space, including nested descendants.
Unknown/loading membership does not briefly show all rooms. Membership updates
clear a newly hidden selection while keeping its draft. Test coverage includes
all three cases. Server-state completeness remains a live acceptance check.

The visual pass replaces oversized avatar rows and account footer controls with
compact icon rows, a bottom account menu, San Francisco loaded from the installed
macOS font, vector icons, a smaller toolbar, grouped message avatars and a rounded
inset conversation/composer. Colors remain an adaptive approximation; native
semantic colors, vibrancy and accessibility-preference integration are pending.
