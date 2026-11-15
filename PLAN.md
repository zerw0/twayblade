# Twayblade — plan

Updated 2026-11-14. Status: M0 in progress; no milestone exit claimed.

## Goal

A simple, native desktop Matrix client. macOS is the primary UX and acceptance
platform; Linux and Windows build from the same source and get their own release
gates. Required for 1.0: E2EE, spaces, threads, OAuth/OIDC, legacy SSO, and
direct/group MatrixRTC calls with a native call interface, interoperable with
Element Call.

## Decisions

| Area | Decision |
| --- | --- |
| Frontend | One GPUI + GPUI Kit frontend for all three OSes. The earlier Iced and SwiftUI/UniFFI frontends were deleted (2026-11-10); they remain in git history only. |
| Protocol | matrix-sdk / matrix-sdk-ui, pinned. Use SDK sync, timelines, stores and crypto; never reimplement protocol state. |
| Sync | Native simplified sliding sync is a **minimum server requirement**. No legacy `/sync` fallback; document this as a compatibility limit. |
| Server entry | Users type a server name (`matrix.org`); `.well-known` discovery resolves it. Only HTTPS homeservers are accepted, including after delegation. |
| Storage | SDK SQLite stores, encrypted with a passphrase held in the OS credential store. No plaintext fallback. Logout and failed sign-in delete the local store. |
| Instances | One running instance per OS user, enforced by a lock file in the app data directory. |
| Calls | Native (decision 2026-11-11, replacing the Element Call web runtime): MatrixRTC signalling in `core` through the Matrix SDK (call membership, call-key exchange over encrypted to-device messages), media through the LiveKit Rust SDK with end-to-end frame encryption, and a GPUI call interface. Interoperates with Element Call. Legacy `m.call.*` is out of scope for 1.0. |
| Appearance | Default: system light/dark and system accent, neutral surfaces, contrasting foregrounds. Users can override scheme, accent, fonts, scale and chat style in Settings (M4). |
| Settings | A native-style Settings window (⌘,) with sections. Preferences are stored locally as plain JSON, never with credentials. Notification rules are Matrix push rules on the server, so they sync with other clients. |
| Toolchain | Rust minimum = the version actually built and tested (1.99). Nix shell is optional. |

## 1.0 scope

In:
- One account; password, OAuth/OIDC and legacy SSO; refresh, restore, logout.
- Rooms and DMs: history, pagination, send queue, replies, edits, reactions,
  redactions, typing, read receipts; images/files with encrypted transfer;
  a full-size image viewer.
- E2EE: cross-signing, device verification, key backup and recovery, visible
  undecryptable-message state. Never silently downgrade encryption.
- Spaces: navigation, nested hierarchy, join/leave. Threads: read, reply, unread.
- Unicode emoji picker and reactions.
- Direct and group MatrixRTC calls with a native interface, interoperable with
  Element Call: ringing, mute, device choice, screen share.
- Notifications, unread counts, keyboard shortcuts.
- Settings: notifications, UI scaling, fonts, colour schemes, chat styles
  (bubbles, IRC, minimal), security and account (see M4).

Deferred past 1.0 (cut from the earlier plan):
- Stickers and sticker packs; custom image emoji.
- An Element-like fourth chat style and per-room style overrides.
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

**M2 — encrypted messaging (5–7 weeks).** Verification, backup and recovery
first; then attachments and the emoji picker (message actions and reactions
are done). Exit: interoperates with
Element in encrypted rooms; reconnect without duplicate sends; fresh-install
recovery; missing-key and revoked-device behavior; no secrets or bodies in logs.

**M3 — spaces, threads and image viewer (4–5 weeks).** Exit: nested/private/restricted spaces,
cycles and multi-parent rooms behave like Element; threads paginate and keep
unread/receipt state across clients without duplicate rendering.

*Image viewer.* Clicking an image thumbnail (or pressing Space on a focused
image, like Quick Look) opens it full size in an overlay over the
conversation:
- Fit to window by default; zoom with pinch, ⌘+/⌘−/⌘0 and double-click, and
  pan when zoomed. Animated GIF/WebP play.
- ← / → step through the room's other images in timeline order; the
  counter and sender/time are shown in a quiet top bar.
- Save…, Copy Image, Open in Default App and Show in Timeline; Escape or a
  click outside closes it and returns focus to the message.
- Full-resolution media is fetched through the SDK (decrypted in memory for
  encrypted rooms), with a progress indicator, bounded decode size and the
  same untrusted-filename rules as attachments. Nothing decrypted is
  written to disk unless the user saves it.
Exit: works for plain and encrypted images from Element, large (>8k px) and
corrupt files fail with a clear message, and keyboard-only use reaches every
action.

**M4 — settings and appearance (3–4 weeks).** One Settings window (⌘,,
menu Twayblade › Settings…) with a sidebar of sections. Every change
previews live, and each section has Reset to Defaults.

- *Notifications.* Default level for DMs and group rooms, encrypted and unencrypted (all
  messages or mentions and keywords; mute is a per-room override), keyword list, and per-room overrides
  from the room's context menu and details panel, all via the SDK's
  notification settings (server push rules). Native OS notifications
  (macOS UserNotifications, Linux freedesktop, Windows toasts) with sound,
  message-preview toggle (off hides sender and body on the lock screen),
  Dock/taskbar badge and click-to-open-room. Encrypted messages are decrypted
  for notifications through the SDK notification client. Respect OS Focus/DND.
- *Appearance: scaling.* Interface scale 85–150% in steps, also via ⌘+, ⌘−
  and ⌘0. It scales the whole layout, not just text.
- *Appearance: fonts.* Interface font (system by default), message font
  family and size (independent of scale), monospace font for code.
- *Appearance: colour schemes.* System (default), Light, Dark, High contrast,
  and curated palettes, each with light and dark variants: Solarized,
  Gruvbox, Nord. Accent: system or a custom colour. Palettes map onto the
  existing semantic tokens in `theme.rs`; every palette must pass 4.5:1 text
  contrast, checked by a unit test per palette.
- *Appearance: chat styles.* Bubbles (current), IRC (one line per message:
  time, sender, text, no avatars) and Minimal (left-aligned, no bubbles,
  sender headers only on change). Plus density (compact/regular), avatars
  on/off and timestamps (hover, always). All styles keep replies,
  reactions, threads, edits, attachments, failed sends and undecryptable
  states, and group by Matrix user ID.
- *Security.* The existing verification, recovery and devices screens move here.
- *Account.* Display name and avatar editing, sign out.

Exit: switching style, scheme, font or scale on a populated room keeps the
selected room, drafts, scroll position and the open thread; no message action
is hidden in any style. Preferences survive relaunch. Notification levels set
here are reflected in Element for the same account.

**M4b — chat polish (2–3 weeks).** From the redesign review (2026-11-11):
- *Search.* ⌘K finds conversations today. Add message search: server-side
  `/search` for unencrypted rooms, and a local index of decrypted events held
  in the encrypted SDK store for encrypted rooms (never a plaintext index on
  disk). Results open the message in context; filters for room, sender and
  "has file".
- *Room media and files.* The Info panel's Files tab lists attachments from the
  loaded timeline only. Replace it with a paginated room gallery (images grid,
  files list) fed by SDK back-pagination, with the image viewer stepping
  through the gallery.
- *Presence.* Change your own status (Online, Away, Invisible/Offline) and a
  status message from the profile menu, and show presence dots on avatars, only
  when the server has presence enabled. Never fake a status.
- *Motion.* Fast, smooth transitions on GPUI's animation API: panels (Info,
  thread, sidebar) slide and fade, popovers and dialogs fade and rise a few
  pixels, room switches crossfade, hover/press are colour and surface steps.
  150–250 ms, one exponential ease-out curve, exits faster than entrances;
  never animate layout-driving size; respect the OS Reduce Motion setting.
- *Typing indicator.* Replace the text line with an animated three-dot
  indicator next to the typer's avatar (static dots and text under Reduce
  Motion), keeping the accessible "Alex is typing" label.
- *Formatting.* HTML message composition (bold, italic, code, links, lists,
  quotes) with a formatting toolbar, sent as `formatted_body` with a plain
  fallback.
- *Room discovery.* Today only a space's own hierarchy can be browsed. Add the
  public room directory: browse and search your homeserver's directory
  (`/publicRooms` with server-side filter and pagination), switch to another
  server's directory (and its third-party networks where offered), and join
  by `#alias:server` or a matrix.to / `matrix:` link. Each result shows name,
  avatar, topic, member count, join rule and whether it is a space; a preview
  (summary or world-readable history) before joining where the server allows
  it, and knock for knock-only rooms. Never auto-join; joining a space opens
  its hierarchy.
- *Message editing.* Edit your own text messages (including formatted ones and
  thread replies): Edit in the message menu and ↑ in an empty composer edit
  the latest one; the composer shows "Editing" with Escape to cancel; Return
  saves. Sent as an SDK `m.replace` edit (keeping encryption and the plain
  fallback); the timeline shows the latest version with "Edited", and a failed
  edit keeps the draft for retry. No edit history view in 1.0.
- *Stacked attachments.* Add several files at once (picker, drag and drop,
  paste) to the composer, remove any before sending, and send them with the
  text using Return or Send.
- *Image size in the timeline.* Settings › Appearance offers Default (images
  as compact previews, capped in size) or Large (images fill the message
  column up to a maximum height), shown as two live previews with radio
  buttons. Applies to the timeline and threads in every chat style; the
  viewer is unchanged. Thumbnails are requested at the size the setting needs.
- *Keyboard shortcuts.* A Keyboard section in Settings lists every shortcut by
  area (Composer, Navigation, Calls when M5 lands), drawn with key caps, and
  the same list is reachable from Help › Keyboard Shortcuts (⌘/). Composer
  shortcuts to implement with it: Return sends, ⇧Return adds a line; ⌘B bold,
  ⌘I italic, ⌘⇧> quote, ⌘⇧L link, ⌘E code (wrap the selection in the
  Markdown the composer already sends); ⌘Z / ⌘⇧Z undo and redo; ⌘↑ / ⌘↓ (⌘Home
  / ⌘End) to the composer's start and end; Escape cancels a reply or edit; ↑
  and ↓ in an empty composer step through your own messages to edit; ⌃⌥↑ and
  ⌃⌥↓ step through what you sent before (composer history, kept per room in
  memory only, never on disk). Use platform modifiers (Ctrl on Linux and
  Windows). The list is generated from the actual key bindings so it cannot
  drift; no custom rebinding in 1.0. Sticker shortcuts are out of scope
  (stickers are deferred past 1.0).

**M5 — calls (8–11 weeks).** Native MatrixRTC calls. Start with an
encrypted audio call between Twayblade and Element Call on macOS, then video,
devices, screen sharing, ringing and Linux/Windows. The Element Call web
runtime spike (`call-host`) has been removed after native encrypted audio interop passed. Exit: encrypted direct and group calls with
Element on each OS, including denied permissions, device removal, network change
and screen sharing (Wayland and X11). Transport TLS is not media E2EE.

Call view (requested 2026-11-13; implemented 2026-11-15, `feature/call-view-modes`):
- *Layout.* The call stage grows like Discord's: a compact strip for voice or a
  single camera; an expanded stage (about 56% of the window height) when
  someone shares a screen or two or more cameras are on. A screen is large
  with people in a strip beneath; cameras are fitted to the stage in a 16:9 grid.
- *Focus on a person or a shared screen.* Each tile and the shared screen have
  a focus button (also from the keyboard); everyone else moves to a strip, or
  is hidden for a focused screen. Clicking again or Escape returns. A focus you
  chose is never overridden.
- *Full screen.* The full-screen button (or ⌃⌘F) makes the call fill the window
  in the macOS full-screen space, with no sidebar or timeline. The controls
  float at the bottom and fade out after 3 s without pointer or key activity
  (they stay while hovered or a menu is open; instant under Reduce Motion).
  Escape or ⌃⌘F leaves. Full screen from the window's own green button keeps
  the chat layout.
- *Follow the speaker.* A toggle in the call controls (shown with two or more
  other people), remembered in preferences. The view moves to someone after
  they have spoken for 1 s and holds 4 s before it can move again; silence keeps
  the pick, and a person leaving clears it. It only reorders the strip while a
  screen is shown.
- *Mute or turn people down for yourself.* A "more" button on other people's
  tiles opens Mute for me, a 0–200% slider and Reset to 100%. It uses the
  existing local playback contract, never changes what others hear, and shows
  as a badge on the tile. Reset at the end of the call. Muting someone for
  everyone needs moderator permissions that MatrixRTC/LiveKit don't
  standardise yet; out of scope.
- Each mode is announced through the call status line (screen readers) and the
  tile labels; transitions follow Reduce Motion.
- Verified: compilation, strict Clippy, 142 offline app tests (including the
  speaker-follow timing and grid-fit rules) and preview captures
  (`--preview-call-video|-volume|-fullscreen|-screen`). Not verified: a live
  call (speech detection, audible volume and mute effects), Element
  interoperability, full-screen transitions and auto-hide on a physical Mac,
  VoiceOver, Linux and Windows.

**UI motion polish (with M5/M6, requested 2026-11-13).** Hover and press
states animate instead of snapping: rows, buttons, rail tiles, message
actions and call tiles fade their background and actions in over 150–200 ms
with an ease-out curve; nothing moves or resizes on hover. Reduce Motion
switches them back to instant changes. Verified in captures and on a
physical Mac at 60 and 120 Hz.

**M6 — release (3–5 weeks).** Accessibility pass, migrations,
packaging, signing, updates. Exit: clean-machine install, upgrade with session
retained, uninstall, notification activation. Signed artifacts need release
credentials and explicit authorization.

Packaging targets (requested 2026-11-13); each needs its own install, upgrade
and uninstall check on a clean machine, and publishing needs authorization:
- *macOS:* signed, notarised `.app` in a DMG, plus a Homebrew cask
  (`brew install --cask twayblade`) in our own tap first, homebrew/cask once
  eligible. The cask tracks GitHub releases and checksums.
- *Linux, all major distributions:* native `.deb` (Debian, Ubuntu and
  derivatives) and `.rpm` (Fedora, openSUSE) built in CI with correct
  dependencies (libsecret, PipeWire, glib, GPU/Vulkan stack); an AUR package
  for Arch; an AppImage as the universal package for everything else. Desktop
  entry, icons, AppStream metadata and the `matrix:` URI handler in every
  package. No Flatpak: Flathub doesn't accept AI-generated applications.
- *Nix:* the flake exports `packages.<system>.default` (the app, built
  reproducibly from `Cargo.lock`), `apps.default`, and an overlay; NixOS and
  nix-darwin users can install it from the flake, and nixpkgs submission comes
  after 1.0. The flake keeps its development shell.
- *Windows:* signed MSI or MSIX, plus winget.

Release flow (requested 2026-11-13): every packaged build is published as a
GitHub **pre-release** with its artifacts, checksums and a changelog generated
from the merged pull requests since the previous release (user-facing wording,
grouped as New, Improved and Fixed, with known gaps stated). A build becomes a
**stable** release only when the user says so at push time; then the
pre-release is promoted (no rebuild), the Homebrew cask, AUR and
winget manifests follow, and Linux/Windows packages are signed. Pre-releases
never update stable channels. Publishing needs the release credentials and
the user's authorization.

Updates (requested 2026-11-13): the app checks GitHub releases at launch and
once a day (no account data sent; off switch in Settings › General) and
follows the channel it was installed from: stable by default, pre-releases
only when the user opts in.
- *Direct downloads* (DMG, AppImage, Windows installer): an in-app updater
  downloads the new version in the background, verifies its signature and
  checksum against the release, and offers "Restart to Update"; it never
  restarts during a call and keeps the session (Keychain/Secret Service), the
  local store and settings. macOS uses a signed update feed (Sparkle-style,
  EdDSA-signed); a failed or unverifiable update leaves the current version
  running and says so.
- *Package-managed installs* (Homebrew, deb/rpm repositories, AUR, Nix,
  winget): no self-update, which would fight the package manager. The app
  shows a quiet update notice with what's new and the command to run
  (`brew upgrade --cask twayblade`, and so on).
- The notice and a "What's New" sheet show the release changelog. Verified
  by upgrading an installed build from one pre-release to the next on each
  OS, including a tampered download that must be rejected.

## Known technical debt

- Message text is escaped and passed through Kit's Markdown renderer to get
  selectable text. Replace with a plain selectable-text element when available.
- Release credential entry is separate from debug; release migration and
  Developer ID signing are M6 work. Development builds can now sign with a
  stable local identity (`macos/create-dev-certificate.sh`), which stops the
  repeated Keychain prompts.

## Next steps (in order)

Prioritize the remaining product and acceptance work below. Individual PRs
#25–#29 passed hosted macOS, Linux and Windows checks; the combined integration
branch requires its own validation. CI does not establish physical acceptance.

1. **M1 authentication** and **2. M2 encryption usability** are implemented
   (see Progress). Remaining: real-browser sign-in on a public server, and
   Element interoperability for verification and attachments.
3. **M3 spaces, threads and image viewer.** Implemented locally; remaining
   Element and physical acceptance covers thread view and replies, space
   join/leave, room directory, invites, room creation; full-size image
   viewer with zoom, pan and next/previous.
4. **M4 settings and appearance.** Local implementation includes Settings,
   appearance, chat styles, server notification rules and native delivery adapters.
   Notification OS permission/delivery/click/badge acceptance and Element comparison
   remain. Linux accent observation now retains its portal connection.
5. **M4b chat polish.** Search, room media gallery, presence, motion, typing
   indicator, formatting, room discovery, message editing, stacked
   attachments, timeline image size and keyboard shortcuts (see Milestones).
6. **M5 calls.** Native calls (decision 2026-11-11): MatrixRTC in `core`, LiveKit Rust SDK media, GPUI call interface. The merged Element Call web-runtime spike was removed after native encrypted audio interop passed.
7. **M6 release.** Accessibility pass, packaging, signing, updates.

## Progress

Implementation is ahead of milestone acceptance. No milestone exit is claimed.
Detailed dated evidence is retained in [the progress history](docs/compatibility/PROGRESS-HISTORY.md)
and [compatibility status](docs/compatibility/STATUS.md).

| Milestone | Implemented | Remaining acceptance or source gaps |
| --- | --- | --- |
| M0 foundation | Password login, OS secret storage, encrypted SQLite restore, logout cleanup, instance lock, room list, timeline, send queue; hosted macOS/Linux/Windows checks | Physical macOS account/restore/logout checks and each OS's runtime gates |
| M1 authentication | Browser OAuth/OIDC with PKCE and callback validation, legacy SSO, refresh and recovery; isolated-server coverage | Public-server browser flows, real IdP cancellation/reauthentication and relaunch |
| M2 encrypted messaging | SDK E2EE, verification, backup/recovery, reactions/edits/redactions, encrypted attachments | Physical verification/recovery flows and Element interoperability; missing/revoked-device acceptance |
| M3 spaces, threads, viewer | Space navigation/join/leave, threads, image viewer; isolated SDK and offline UI checks | Element comparison, keyboard/trackpad use, real rooms and encrypted media across clients |
| M4 settings | Appearance/chat styles, server push rules, notification adapters, account/security settings | OS permissions, delivery/click/badges, system appearance/accent transitions and Element comparison |
| M4b chat polish | Search, gallery, presence, typing, formatting, history and keyboard UI | Physical keyboard layouts/IME, focus, long-history performance and public-server behavior |
| M5 calls | Native MatrixRTC/LiveKit frame E2EE, ringing, devices, screen share, playback contracts, generated three-party interoperability | Live check of playback controls and speaker follow; X11/Windows source-picker views; Linux/Windows webcam capture is unimplemented; physical media/permissions on all OSes |
| M6 release | AccessKit labels, binary CI, Nix package, update-check service/contracts | VoiceOver/AT-SPI/UIA, updater views, signed/tampered upgrade handling, signing/notarization and installed-package acceptance |

Integration base (2026-11-14):
- `feature/integration-base` starts at current `origin/main` `077f021`, which
  already contains PR #25. It combines #29, #26, #27 and #28 in that order.
  The original feature branches and PRs are unchanged.
- Resolved shared call state/initializers, typed errors, ticket-preserving
  snapshots, teardown cleanup and test helpers by retaining both contracts.
  The call fixture registers three accounts and keeps the playback readback
  and key-rotation checks. The Nix shell keeps clang 21 and the added
  PulseAudio/PipeWire/X11 runtime dependencies.
- Hosted checks: read-only GitHub verification found all six checks successful
  at #25 `a426713`, #26 `46c80cd`, #27 `0987fdb`, #28 `025c725` and #29 `dbc0a2a`.
  These results apply to the individual PR heads, not this integration branch.
- Combined-source macOS compilation/offline checks: `cargo fmt --all -- --check`,
  strict locked all-target/all-feature Clippy and locked all-feature workspace
  tests passed: 222 passed, 32 opt-in checks ignored.
- Combined-source isolated integration: the first call suite passed 3/4
  (192.52 s); three-party setup timed out before the third member joined.
  The focused three-party rerun passed (20.50 s) with unchanged assertions and
  deadline. Numeric-only timeout counters were added; the initial failure's
  cause was not established. The complete final rerun passed 4/4 (158.65 s),
  and isolated `run.sh services` passed 6/6, including same-device encrypted
  store restore. Fixture services stopped afterward. Final formatting and
  strict all-target/all-feature core Clippy passed after the diagnostic edit.
  Generated media is not physical microphone/speaker/camera or desktop-capture
  acceptance.
- Nix: fresh `nix flake show --no-eval-cache --all-systems --json` passed for
  Apple Silicon macOS and x86_64/aarch64 Linux on the combined source. The
  original #28 Apple Silicon package build remains historical evidence. The
  combined source's `nix build .#default` also built on Apple Silicon macOS
  (2026-11-14, 7 min, `twayblade-0.1.0`); it was not launched or physically accepted.
- GitHub project Board rendering and date-field configuration remain unverified;
  this session could not open the user's Chrome. No roadmap dates were invented.
- No remote publication or PR merge was performed by this integration work.
Invite people (2026-11-14, `feature/invite-members`): room info has an
Invite action that opens a dialog for a Matrix ID; core checks the invite power
level and maps a server 403 to a permission message. Verified by compilation,
strict Clippy and an offline UI test only. Not verified against a live homeserver
or Element, and there is no member picker or user-directory search yet.

Updater views (2026-11-14, `feature/updater-views`): Settings > General has the
update switch, channel, status, plain-text What's New, displayed upgrade command
and release-page button. Verified by compilation, strict Clippy and offline app
tests only; no capture, Element comparison or physical Mac review. A launch-time
notice outside Settings is not implemented.

First live call (2026-11-13, operator-reported): a direct call between Twayblade
on macOS and Element on macOS (another person, real accounts) worked in both
directions for audio, camera video and screen sharing, using physical
microphones, speakers, cameras and the macOS screen picker. No logs or captures
were taken. Media E2EE is mandatory in the client (call buttons are disabled in
unencrypted rooms; unencrypted publications are refused), but this call did not
independently inspect key exchange or encryption. Still open: group calls live;
who rang whom and whether the ringtone, incoming card and OS notification actions
behaved; device removal, permission denial/revocation and network changes; call
shortcuts pressed during a live call; Element Web and other Element versions;
Linux (Wayland/X11) and Windows. No M5 exit claimed.

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

## Workspace

```text
crates/core/        Matrix services, auth, sync, timelines (owns all Matrix state)
crates/platform/    OS credentials, notifications, appearance (later: deep links)
crates/app/         The app (crate `twayblade`): GPUI + GPUI Kit frontend
  src/ui/           Views: window chrome, timeline, conversation, panels,
                    settings, security, spaces, threads, viewer, emoji
  src/backend/      Bridges the UI to core services (snapshots, actions)
crates/media/       Native LiveKit/libwebrtc media and bounded video handles
macos/              Development .app bundle helper
tests/integration/  Isolated Synapse fixture
docs/               Architecture and compatibility evidence
```

## References

- https://github.com/matrix-org/matrix-rust-sdk
- https://github.com/longbridge/gpui-kit
- https://github.com/element-hq/element-call
- https://github.com/livekit/rust-sdks

### Scheme-specific accents (2026-11-14)

- OS accent and saved accent overrides now apply only to System. Solarized uses its blue, Gruvbox its light/dark blue, Nord its frost blue, and Catppuccin its Latte/Mocha mauve. Light/Dark/High Contrast use fixed blue; Custom uses the Catppuccin base accent because its editable palette has no accent field. Saved overrides are retained for switching back to System.
- Existing foreground, link contrast and high-contrast adjustments remain applied to the chosen accent. No view layout or Settings controls changed.
- Added regression coverage for override isolation across all schemes and variants, plus foreground/link contrast. `git diff --check` passed. Rust formatting, Clippy, tests and compilation were not run: the required pinned Nix shell is unavailable on this machine. Live appearance and physical platform acceptance remain unverified; the running bundle has not been rebuilt.
