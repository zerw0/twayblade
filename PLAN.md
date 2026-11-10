# Native Rust Matrix client

Date: 2026-11-10. Status: M0 in progress.

## Goal

A simple native desktop Matrix client for macOS, Linux, and Windows, with E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and direct/group calling through Element Call.

macOS is the primary product platform, per the user's 2026-11-10 direction. Design and validate interactions on macOS first while maintaining portable core/UI and Linux/Windows release gates.

## Architecture decisions

| Area | Initial choice | Gate |
| --- | --- | --- |
| UI | Iced, native Rust rendering | Text selection, IME, accessibility, timeline performance |
| Protocol | matrix-sdk and matrix-sdk-ui | Pin compatible releases; verify server capabilities |
| Async | Tokio | Never block UI; cancel account tasks on logout |
| State | SDK SQLite state/crypto stores | Store encryption and migration tests |
| Secrets | OS credential stores | Keychain, Windows credentials, Linux Secret Service; no plaintext fallback |
| Calls | Pinned Element Call assets and SDK widget bridge | Actual encrypted media on all desktop platforms |
| Call runtime | Evaluate Wry in a separate call window | WebKit/WebKitGTK/WebView2 compatibility; assess Chromium helper if needed |
| Tooling | Project Nix shell, Cargo workspace | Native OS build/test matrix |

Chat and application logic are native Rust. Element Call includes web assets and a browser runtime. Fully web-free calling would be a separate implementation. Evaluate matrix-rust-rtc during M0 as a future alternative, without replacing the requested Element Call integration silently.

## First release scope

- One active account, persistent login, refresh and logout.
- OAuth/OIDC, legacy Matrix SSO with identity-provider selection, password login where advertised.
- DMs and rooms: history, pagination, send queue, attachments, edits, reactions, replies, redactions, typing, receipts.
- E2EE: cross-signing, device verification, encrypted key backup, recovery and visible missing-key handling.
- Spaces: nested/private spaces, membership, discovery, join/leave, permission-aware management.
- Attachments: images, files, audio and video; file picker, drag/drop and clipboard image paste; encrypted upload/download in encrypted rooms, previews, save/open and visible transfer progress/failures.
- Stickers: render and send Matrix sticker events; picker with a documented pack source, favorites and graceful unsupported-format handling. Establish encrypted-room behavior and Element interoperability before enabling sticker sending there.
- Emoji: native composer picker, Unicode emoji insertion, searchable shortcodes, skin tones, recent/favorite emoji and message reactions. Custom image emoji are a separate compatibility extension, not implied by Unicode support.
- Threads: history, reply, pagination, unread state, receipts and notifications; no duplicate root/reply rendering.
- Audio/video direct and group calls using modern MatrixRTC and Element Call, incoming ringing, device selection, mute, screen sharing and reconnect.
- Desktop notifications, unread counts, shortcuts, light/dark mode and installable packages.

Deferred: multi-account, arbitrary widgets, bots, advanced moderation/server administration. Legacy `m.call.*` calling is a separate compatibility feature; modern direct calls do not prove it works.

## Interface

Use a macOS-first visual language: compact type and controls, restrained blue actions, system light/dark appearance, standard window decorations, comfortable desktop density and progressive disclosure. The sign-in window starts at 640 × 700 logical pixels with a 360-pixel content column, staged server/account entry, human-readable errors and no protocol-flow dump.

The conversation UI should follow Mac sidebar/split-view conventions. Native menu bar integration, Command shortcuts, preferences, window lifecycle, accessibility and any vibrancy must be implemented/tested separately. Styling Iced does not create AppKit controls. See `docs/architecture/MACOS_UI.md`.

Use the user's Cinny screenshot as the information-layout reference: space rail, compact rooms, room header, grouped timeline, rounded composer and optional members/details inspector. Adapt with macOS typography, rounded selections and semantic system colors. Current system light/dark tracking and approximate palette do not establish native accent/semantic-color integration; implement that through the platform layer and verify preference changes.

Space rail → room/DM list → conversation → optional thread/details panel. Collapse navigation on narrow windows. Default to conversations, reveal details on demand, and retain a return-to-call control during navigation. Provide guided verification/recovery and actionable errors. Keep technical diagnostics out of normal product flows.

## Milestones and acceptance

Estimates are engineering weeks for one experienced full-time developer, assuming test infrastructure and access to target OSes. Total initial envelope: 18–27 weeks; revise after M0.

### M0 — feasibility (2–3 weeks)

1. Scaffold Cargo workspace, project Nix shell, native window and native OS CI.
2. Build homeserver discovery and authentication probes. Test OAuth and legacy SSO in system browser.
3. Exchange encrypted messages between two test users; restart without changing device identity and recover history.
4. Exercise large synthetic timeline, pagination anchoring, selection/copy, multilingual IME, keyboard and screen reader.
5. Embed Element Call and establish a three-participant encrypted call spanning macOS, Windows and Linux. Test microphone, camera, screen sharing, permissions and reconnect.
6. Record dependency/license inventory, minimum OS/runtime/server requirements, sync capability behavior and UI/runtime decision.

Exit: verified messaging/session restoration and encrypted cross-platform call; documented decision on Iced and call runtime. Compilation alone does not pass M0. If server sync requirements exclude older servers, publish that limitation or implement/test a supported fallback before claiming compatibility.

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
crates/desktop/       Iced views and presentation
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
- [ ] M3a: Matrix space navigation, hierarchy, membership and permission-aware management.

## Progress

- Three-crate workspace, Cargo/Nix lockfiles and draft three-OS CI. macOS check/build, rustfmt, Clippy with warnings denied and 16 unit tests pass. Linux/Windows and CI are unverified.
- User screenshots confirm discovery and password sign-in on voltized.de. Same-device relaunch, logout cleanup, browser login, token refresh, interrupted-login and multi-instance handling remain pending.
- macOS interface now uses proportional bold/semibold San Francisco headings, compact sign-in, system appearance and a 1120 × 760 split-view workspace. Search, unread counts, selection, Tab navigation, Command-F and Command-Q are wired. Native menus remain pending.
- User screenshot confirms live room loading on voltized.de. Fixed avatar centering that overrode fixed width and pushed names to the right. Kept distinct rooms with identical names; room IDs are the identity.
- SDK timeline increment: selected-room sync subscriptions, observable message history, initial/explicit 30-event back-pagination, plain-text composer, local echo/delivery state and send queue. Per-room in-memory drafts survive navigation; account/selection generations reject stale results and pending sends remain tracked across navigation. Missing decryption keys and redactions have visible placeholders. User screenshot confirms live history rendering. Sending and isolated E2EE interoperability remain pending. Native sliding sync is required; no legacy-sync fallback is claimed.
- Credential-free native workspace preview launched and its own 2240 × 1520 capture was visually inspected. Corrected a bold-font fallback discovered in the first capture. Previews never access Keychain, authenticate or synchronize.
- Offline populated conversation preview launched and captured for layout QA; sample data is explicitly labeled, sending disabled and no credentials accessed.
- Next: verify history, live delivery, pagination and same-device relaunch using dedicated test accounts; perform isolated two-user encryption acceptance. Add verification/recovery, message actions, send-failure retry/cancel, timeline virtualization and scroll anchoring. Continue browser login/session hardening and call feasibility work.
- Conversation presentation now includes sender display names (Matrix ID on hover), local timestamps, identity-based consecutive-message grouping, subdued rounded selection and follow-latest/Jump to Latest behavior. Grouping regression test passes. Populated offline preview inspected; live scrolling and pagination anchoring still need acceptance. Cinny-inspired layout and future native semantic colors are recorded in `docs/architecture/MACOS_UI.md`.
- Space navigation implemented using SDK SpaceService filters: narrow rounded rail, Home, selected-space heading, joined-room filtering, nested-space indicators and unread aggregation. Parent filtering expands SDK-provided descendants with cycle/deduplication guards; shared rooms retain their IDs and drafts across navigation. Stale updates are rejected and removed-space selection falls back to Home. SDK navigation filters currently expose two levels; hierarchy discovery, deeper navigation, create/join/leave/invites and room management remain pending.
- Native offline spaces preview launched and inspected: parent space selected, unrelated rooms hidden, nested-space marker and unread counts visible. Live space loading still requires connected-account acceptance.
- M0 remains incomplete: encrypted messaging, calling and cross-platform physical acceptance are outstanding. See `docs/compatibility/STATUS.md`.
- User explicitly requested spaces, attachments, stickers and emoji as planned workstreams. They now have dedicated scope and acceptance sections above; no implementation completion is implied. The 18–27 week envelope predates the detailed sticker/media scope and must be re-estimated after M0 and pack/codec decisions.

## Primary references

- https://github.com/iced-rs/iced
- https://github.com/matrix-org/matrix-rust-sdk
- https://docs.rs/matrix-sdk-ui/latest/matrix_sdk_ui/
- https://docs.rs/matrix-sdk/latest/matrix_sdk/authentication/oauth/index.html
- https://docs.rs/matrix-sdk/latest/matrix_sdk/authentication/matrix/struct.MatrixAuth.html
- https://github.com/element-hq/element-call
- https://github.com/tauri-apps/wry
- https://github.com/element-hq/matrix-rust-rtc

## Apple-design refinement and Home semantics

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
