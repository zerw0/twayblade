# Twayblade — plan

Updated 2026-11-12. Status: M0 in progress; no milestone exit claimed.

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

**M6 — release (3–5 weeks).** Accessibility pass, migrations,
packaging, signing, updates. Exit: clean-machine install, upgrade with session
retained, uninstall, notification activation. Signed artifacts need release
credentials and explicit authorization.

## Known technical debt

- Message text is escaped and passed through Kit's Markdown renderer to get
  selectable text. Replace with a plain selectable-text element when available.
- Release credential entry is separate from debug; release migration and
  Developer ID signing are M6 work. Development builds can now sign with a
  stable local identity (`macos/create-dev-certificate.sh`), which stops the
  repeated Keychain prompts.

## Next steps (in order)

Decision 2026-11-10: build the full application first; CI is not a gate for now.
Linux and Windows CI passed on `36f850a`; macOS CI was switched to `macos-15`
(the `macos-14` image is retired) and its result has not been checked.

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

Done (macOS arm64, offline and isolated-server verification only):
- Cargo workspace (`core`, `platform`, `app`), Nix shell, pinned dependencies.
- Server-name discovery via `.well-known`, HTTPS enforcement after delegation.
- Password sign-in, OS credential store, encrypted SDK store, restore, logout
  with store deletion, failed sign-in cleanup, single-instance lock.
- GPUI workspace laid out after the approved mockup (2026-11-10): space rail
  with unread badges, sidebar with Rooms / Direct messages sections and account
  row, room header (avatar, topic, encryption lock), bubble timeline with day
  separators and undecryptable/deleted states, rounded composer, details panel
  (Cmd-I) and profile panel (Escape closes). Two-step sign-in (server, then
  implemented password and OAuth options). Controls for unbuilt features
  are omitted, not faked.
- Home (rooms outside all spaces), room search, recency-sorted rooms, per-room
  drafts, pagination, plain-text send queue, read-only profiles.
- Offline previews, incl. a 10,000-message virtualization smoke fixture.
- Isolated Synapse fixture: SDK-to-SDK encrypted send/receive and same-device
  store reopen pass (`tests/integration/`).

Functional workspace (2026-11-10, Codex; reviewed):
- Local M0 path: operator-confirmed GPUI sign-in, sync, send and Keychain
  restore; automated cross-process Keychain lifecycle incl. logout cleanup.
- Timeline: replies, reactions (toggle), thread reply counts, emote/notice,
  image/file/audio/video placeholders, failed-send retry/cancel.
- Room state: members with Admin/Moderator roles, typing (in and out),
  "New messages" separator, read receipts for the visible latest event.
- Scroll-to-top pagination with stable reading anchor.
- Debt cleared: typed core errors/statuses, core records used directly,
  RoomListService-driven room list; separate release credential entry.
- 26 unit/UI tests; six isolated-server checks (see STATUS.md).

OAuth/OIDC — Task 1 (2026-11-10):
- SDK metadata/discovery, dynamic registration, PKCE and one-shot loopback
  callback; system-browser action, Cancel and typed inline failures.
- Password-compatible atomic Keychain record holds OAuth client ID/session;
  synchronous SDK refresh callbacks persist rotations and serialize with logout.
- fmt/strict Clippy/31 unit/UI tests pass; light/dark sample captures reviewed.
  MAS 1.27.0 + Synapse 1.164.2: login, cancel, wrong state, expired-token refresh,
  three-process relaunch and logout pass. Physical browser/real-server check pending.

Legacy SSO — Task 2 (2026-11-10):
- SDK provider discovery/name buttons and bounded provider icons; shared
  one-shot state-checked callback, cancellation and SDK loginToken exchange.
- fmt/strict Clippy/34 unit/UI tests pass; light/dark SSO sample captures reviewed.
- Synapse 1.164.2 with local MAS 1.27.0 OIDC: full redirect, token exchange,
  Keychain restore and logout pass. Physical browser/real IdP check pending.

Session recovery — Task 3 (2026-11-10):
- SDK revocation events stop sync and show signed-out UI. Soft reauthentication
  retains store/device (password, OAuth and SSO); hard logout needs local-data
  deletion confirmation. Credential generations reject stale refresh writes.
- fmt/strict Clippy/36 unit/UI tests pass; light/dark soft/hard samples reviewed.
- Synapse 1.164.2: second-client device revocation, clean sync stop, retained
  credentials before confirmation, failed/successful same-device soft recovery
  and cleanup pass. Physical revoked-session/browser reauth checks pending.

Verification and recovery — Task 4 (2026-11-10):
- SDK cross-signing/backup setup, one-time recovery-key copy/confirmation,
  key/passphrase restore, live history re-decryption, SAS and incoming prompts.
- Settings → Security lists verified devices, revokes other password devices,
  uses OAuth browser management when advertised, and confirms recovery-key reset.
- fmt/strict Clippy/42 unit/UI tests pass; light/dark verification/recovery
  captures reviewed. Isolated Synapse: SAS/cancel, wrong key, fresh-store old
  history restore, device revocation and recovery reset pass. Element/physical
  UI and browser-management acceptance remain pending.

Attachments — Task 5 (2026-11-10):
- Native file picker, drop/paste, filename/size/thumbnail confirmation; SDK encrypted
  send queue, upload progress/cancel and restore; server upload limit enforced.
- Bounded image decode/render cache, file Save/Open with chosen destination,
  sanitised names and atomic downloads; opening requires an explicit action.
- fmt/strict Clippy/45 unit/UI tests pass; light/dark attachment captures reviewed.
  Synapse media round trips, thumbnails, oversize, cancel and queued restart pass.
  Physical dialogs/drop/paste and interruption during an active HTTP upload pending.

Emoji picker — Task 6 (2026-11-10):
- Shared searchable composer/reaction picker, keyboard selection, six skin tones,
  bounded persistent recents and `:shortcode` completion; inserts at Unicode cursor.
- Unicode 15.x baseline avoids newer missing system-font glyphs. Sample actions
  stay offline; room/account/cursor changes reject stale insertion.
- fmt/strict Clippy/49 unit/UI tests pass; light/dark emoji captures reviewed.
  Synapse `timeline_metadata_and_send_queue_actions` reaction regression passes;
  physical keyboard/IME/screen-reader and platform font checks remain pending.

Review fixes (2026-11-10, after Codex tasks 1–6):
- Browser callback keeps listening through preconnect sockets, favicon and
  forged-state requests instead of failing sign-in on the first connection.
- macOS bundle sets `LSFileQuarantineEnabled`, so opened attachments get
  Gatekeeper's downloaded-file warning. Windows Mark-of-the-Web is still missing.
- Recovery-key confirmation no longer requires copying to the clipboard.
- Open question: OAuth registration sends the homeserver URL as `client_uri`;
  it should be the client's own homepage once one exists.

System accent (2026-11-10):
- Primary controls, outgoing bubbles and selection tints derive from the OS
  accent, with contrasting foregrounds. macOS uses AppKit; Windows uses
  UISettings; Linux uses the desktop portal (fallback when unavailable).
- Native macOS read, actual theme-update and contrast tests pass (50 tests total).
  Live preference changes and Windows/Linux execution remain platform checks.

Layout fixes — Task 0 (2026-11-10):
- Wrapped the unencrypted-room warning; removed Kit’s default 32px row padding
  so message groups retain 6px spacing. Composer/account icons use 28px targets;
  composer icons align centrally and typing/status text uses the 13px base size.
- fmt/strict locked Clippy/55 unit/UI tests pass; three layout regressions,
  including a measured 38px-before/6px-after gap; reviewed light/dark captures.
- Five existing service checks pass on isolated Synapse 1.164.2; physical
  trackpad, multiline composer and real-account layout acceptance remain pending.

Threads — Task 1 (2026-11-10):
- SDK thread timelines, send queue, pagination, unread summaries and scoped receipts;
  replies hidden in the main timeline. Panel reuses grouping, retains per-thread
  drafts and rejects stale updates; Escape closes it. Room badges keep SDK totals.
- fmt/strict locked Clippy/57 unit/UI tests pass; light/dark thread samples reviewed.
  Isolated Synapse 1.164.2 thread acceptance and all five service regressions pass;
  initial pagination runs independently to prevent a suspended-stream lock stall.
- Operator checks remain: Element unread/receipt interoperability, real-account
  thread navigation, encrypted thread media and physical keyboard/trackpad use.

Spaces — M3a Task 2 (2026-11-10):
- SDK hierarchy with unjoined/suggested/restricted children and pagination;
  space join, explicit room selections on leave, room/DM/space invites,
  encrypted-by-default room creation and Matrix-ID DMs. Parent controls use SDK powers.
- fmt/strict locked Clippy/60 unit/UI tests pass; directory/invite light/dark
  samples reviewed. Fresh-account Synapse 1.164.2 acceptance includes real
  multi-page hierarchy, inaccessible children, restricted joins and selective leave.
- Home/cycle/multi-parent tests and five service/thread regressions pass.
  Operator checks remain: Element joins/leaves/invites, real-account permissions,
  native keyboard navigation and large hierarchies across servers.

Image viewer — M3 Task 3 (2026-11-10):
- Full-resolution SDK media overlay with keyboard/pinch/double-click zoom, pan,
  animation, image navigation, clipboard and explicit Save/Open destinations.
  Closing reveals and focuses the message; earlier images request SDK history.
- fmt/strict locked Clippy/66 unit/UI tests pass; keyboard/focus/save/gesture
  tests and reviewed light/dark samples. Isolated Synapse 1.164.2 verifies plain
  and encrypted images, GIF/WebP, oversized/corrupt inputs and uncached decryption.
- Decode limits: 8192px, 64MiB encoded; stills may use the full 8192² budget
  and are scaled to 4096px for display (Save/Open keep the original);
  animations 96MiB decoded and 240 frames. Small images fit up to 2×.
  Physical animation, native dialogs/default apps, trackpad,
  Element media interoperability and Linux/Windows execution remain pending.

M3 review fixes (2026-11-10):
- Rooms created in a space get a restricted join rule for the parent space
  (Element's "visible to space members"); the spaces server test asserts it.
- Thread panel: compact root header, "Load earlier replies" at the top,
  muted status line, composer matches the main field.

Settings — M4 Task 1 (2026-11-10):
- Versioned frontend JSON preferences with atomic replacement, defaults and corrupt/unknown-file backup; one separate Settings window with six sections, Security controls and SDK account name/avatar edits. Unimplemented controls remain hidden.
- Locked fmt/strict Clippy/69 unit/UI tests pass; Settings light/dark samples reviewed; section navigation retains the main conversation and draft.
- `encrypted_delivery_profile_and_same_device_store_restore` passes on isolated Synapse 1.164.2 (`twayblade.test`), including core display-name/avatar update/removal. Operator checks: native menus/shortcut, file picker, account sign-out, security focus/VoiceOver. Appearance, Chat and Notifications controls follow in Tasks 2–5.

Colour schemes and accent — M4 Task 2 (2026-11-10):
- System/Light/Dark/High contrast and Solarized/Gruvbox/Nord variants; persisted system/custom accent with six swatches and Kit colour picker, applied live through semantic tokens.
- Locked fmt/strict Clippy/82 unit/UI tests pass, including one contrast test per palette variant (4.5:1; High contrast 7:1) and live-control room/draft/timeline/thread preservation. Scheme light/dark samples captured; physical system transitions and picker/VoiceOver checks pending.
- Frontend-only change; no new isolated-server check required. Task 1's Synapse account check remains separate evidence.

Scaling and fonts — M4 Task 3 (2026-11-10):
- 85–150% in 5% steps and View shortcuts; shared logical-pixel helper plus Kit type scale covers the workspace and panels. Responsive column caps retain conversation space. Installed-family selectors set interface/message/code fonts; message size is independent; missing families fall back safely.
- Locked fmt/strict Clippy/83 tests pass; actual sidebar/composer scaling and font fallback tested; `captures/scale-{85,150}-{light,dark}.png` sample layouts reviewed. Frontend-only; no additional isolated-server check.
- Physical checks pending: native shortcuts versus image zoom, font rendering/IME, live scale and reading-anchor behavior with long histories, Linux/Windows.

Chat styles — M4 Task 4 (2026-11-10):
- Shared Bubbles/IRC/Minimal renderer in the timeline and threads, persisted density/avatars/timestamps; SDK edit indicator and all existing reply/reaction/media/error controls retained. Style changes remeasure retained scrollers.
- Locked fmt/strict Clippy/85 tests pass; control-driven style test preserves visible IDs, room, draft, reading anchor and open thread, and reaches existing message actions; edited/deleted states included. Light/dark style and attachment samples captured; thread samples included.
- `timeline_metadata_and_send_queue_actions` passes on isolated Synapse 1.164.2 (`twayblade.test`). Operator checks: keyboard/VoiceOver, long-history switching, real encrypted attachments and thread interactions. M4 notifications remain unimplemented.

M4 review fixes (2026-11-10):
- Choosing System after a forced Light/Dark scheme re-reads the OS appearance
  (it previously stayed on the forced mode); regression test added.
- Settings window restyled like the main window: sidebar under the traffic
  lights with icon rows, section title in the toolbar, grouped form rows with
  segmented controls, steppers, scheme thumbnails, accent dots and a switch.
- General and Notifications are hidden from the sidebar until they have real
  controls; Settings opens on Appearance.

Chat styles, avatars and performance (2026-11-10, after operator screenshots):
- Minimal is Slack-style (avatar gutter, name and time per group); IRC shows
  coloured 13px senders, a time column only with "Always", one-line replies.
- Real avatars for senders, members, DMs, rooms, spaces and your account:
  SDK thumbnails decoded and cropped in core, cached per `mxc://`, initials
  as fallback. Reply previews retry failed fetches; unloadable ones read
  "Replying to an earlier message" instead of a fake "Reply" sender.
- Scrolling: only message groups whose content changed are re-measured (not
  every row on every typing/receipt update); per-row full-history scans
  replaced by a per-snapshot cache; thumbnails cached by buffer instead of
  copied and hashed every frame. Dependencies are optimised in dev builds and
  `macos/build.sh --release` builds the optimised app.

Thread unread caching — Notifications Task 1 (2026-11-10):
- Per-selection root/reply-count cache; four concurrent SDK queries, immediate cached/zero snapshots and follow-up counts. Own public/private receipts and explicit mark-read invalidate; stale results are rejected and selection drop cancels work.
- Locked fmt/strict all-feature Clippy and 91 workspace tests pass, including a counting stub, invalidation, focused-root and bound/eviction regressions. The isolated `threads_send_paginate_receipts_and_closed_unread` passes on Synapse 1.164.2 (`twayblade.test`).
- Offline thread controls pass; reviewed `captures/notifications-thread-{light,dark}.png` sample captures. Physical checks remain: real-account thread counts versus Element and long-history scrolling.

Notification rules — Notifications Task 2 (2026-11-10):
- SDK server defaults for four room classes (All/Mentions only), keywords and Default/All/Mentions/Mute overrides; subscribed cross-client updates, grouped Settings, details controls, room context menu and muted badges. Edits use fresh SDK views, bounded idempotent retries and sync confirmation. Changed room summaries refresh SDK notification counts, preserving higher server totals without adding thread counts.
- Locked fmt/strict all-feature Clippy and 92 workspace tests pass; control-driven Settings/keyword/details/context-menu test passes. Reviewed release `captures/notification-{rules,room}-{light,dark}.png` sample captures.
- `notification_rules_round_trip_and_second_client_changes` passes on isolated Synapse 1.164.2 (`twayblade.test`), including muted unread exclusion. Physical checks: rules shown in Element, real sidebar/details/menu and keyboard/VoiceOver. Native delivery follows in Task 3.

Notification delivery — Notifications Task 3 (2026-11-11):
- Bounded SDK sync intake and NotificationClient decryption; own/muted/active-room suppression, guarded room/thread clicks and persisted local enable/sound/preview/badge controls. macOS UserNotifications (bundle detection/permission), persistent Linux D-Bus and Windows toast adapters; OS Focus remains authoritative.
- Locked fmt/strict all-feature Clippy and 97 workspace tests pass, including unbundled behavior, skip/privacy rules and device/click controls. `encrypted_notification_delivery_and_private_preview` passes on isolated Synapse 1.164.2 (`twayblade.test`); reviewed release `captures/notification-{delivery,private,device}-{light,dark}.png` samples.
- Operator checks: permission grant/deny/settings link and real delivery/click-to-room/thread, sound, preview, Focus and badge from the release .app. Linux/Windows builds/runtime, desktop badge support, Windows installed app identity and cold-launch activation remain unverified; current callbacks are scoped to the running account/process.

Linux accent observation — Notifications Task 4 (2026-11-11):
- One process worker owns the portal connection/runtime, subscribes to accent changes and caches RGB. Unsupported signals fall back to queries through the same proxy; a disconnected bus retries with backoff. ashpd's settings feature is explicit.
- Locked fmt/strict all-feature Clippy and 97 workspace tests pass; actual portal and Linux notification source typechecked in a macOS harness (not a Linux build). Reviewed release `captures/notification-accent-{light,dark}.png` samples; no new Matrix-server check applies.
- Operator checks: Linux portal accent transitions, missing-setting fallback, connection count and bus/portal restart. Linux/Windows builds and physical execution remain pending.

Notifications review and Claude Design redesign (2026-11-11):
- Review of Notifications Tasks 1–4 found no defects: own messages filtered in core, push rules applied by the SDK NotificationClient, bounded response queue. Open items unchanged (cold-launch activation; Linux/Windows runtime).
- Window chrome: unified 40pt title bar (sidebar toggle, back/forward history with ⌘[ / ⌘], centred ⌘K search, connection dot, notifications, settings, own avatar). The connection shows text only when not connected.
- Rail with Home first, selection bar and dashed join tile; sidebar with collapsible Favourites / Rooms / Direct messages sections, favourites via room tags (`m.favourite`), hash/lock icons, mention and unread badges, and a Compact / Regular / Comfortable density setting in Appearance. Create room, new DM and join space open as small dialogs.
- Conversation header: room icon, name, favourite star, member and pinned counts, topic, encryption and Info toggle (⌥⌘I). Timeline: ruled day dividers, red "New messages", foreground sender names, thread link with icon, a hover action bar, "Read" with an icon; IRC always shows the time column. Composer: a bordered box with a "Write to # room" placeholder and an attach/emoji/send toolbar; the typing line sits below it.
- Info panel with About (topic, Favourite/Mute/Copy link tiles, encryption, pinned and file counts, notification level, room ID), Members and Files (attachments from the loaded timeline) tabs.
- Member actions in profiles: Message (opens or creates a DM), Verify (SDK user verification, flow in Settings › Security), Ignore (account-wide, confirm), and Mute in room, Remove and Ban gated by the room's power levels (confirm for destructive ones).
- Emoji picker rebuilt as a standard picker: search, line-icon category bar, one virtualised grid with Frequently Used first, a name/shortcode footer, and a skin-tone popover for emoji that have tones (the chosen tone becomes the default).
- Impeccable quieter pass: sentence-case section labels, no kicker labels, semibold (not bold) headings, lighter selection tint, line icons instead of emoji glyphs in chrome.
- Locked fmt, strict all-feature Clippy and the workspace tests pass (68 GPUI tests, including new member-action and tone-popover tests). Offline preview captures reviewed. Not verified against a live server: favourites round-trip, member moderation, user verification with Element, DM creation from a profile.

M4b core — Task 1, room media gallery (2026-11-11):
- `Account::room_media` uses SDK `/messages` back-pagination and in-memory SDK decryption, with an event-type filter for unencrypted rooms and unfiltered encrypted envelopes; failed filtered requests retry without filters. `contains_url` is deliberately omitted because encrypted files forwarded into plain rooms use nested `file.url` and would be missed. Page limits are 1–100, with at most four requests per page; empty sparse pages retain a continuation. Cursors bind account instance, room and kind; oversized server chunks are rejected before projection. Opaque sources are not serializable or Debug; media fetches disable the SDK disk cache.
- UI contract: `Snapshot.media.{room,kind,items,loading,exhausted,error}`; `MatrixApp::load_media(kind)`, `load_more_media()` and `open_media_image(event_id)` (Window/Context arguments as usual). Items and viewer navigation retain newest-first pagination order. The backend deduplicates IDs, keeps one in-flight request, cancels selection/account work and checks account instance, selection and request before publishing. Metadata accumulates only on explicit page requests; exhausted means the server supplied no continuation. No new views or controls; UI integration entry points are explicitly marked as pending consumers.
- Compilation: strict locked workspace/all-target/all-feature Clippy passed. Unit/offline checks: locked all-feature workspace tests passed, including page bounds, encrypted-source projection and cancelled/stale gallery requests; rustfmt check passed after formatting.
- Live integration: isolated Synapse 1.164.2 viewer/gallery check passed for plain/encrypted images, two-item pages through exhaustion, stable IDs/order, SDK in-memory decryption, absent decrypted media cache, animation and corrupt/oversize failures. No Element interoperability or physical platform acceptance claimed.
- Remaining: gallery views, physical viewer/gallery navigation, Element attachments, Linux/Windows builds/runtime.

M4b core — Task 2, message search (2026-11-11):
- Core `SearchQuery`, `SearchResult`, `SearchPage` and opaque account/query-bound `SearchToken`; server `/search` explicitly restricted to confirmed unencrypted joined rooms, including global searches. Encrypted rooms scan only already-decrypted SDK cache events, with `limited: true`; no application index or plaintext persistence. Local scans cover at most 2,000 current cache events per encrypted room, retain at most 1,000 matches and reject scopes over 500 rooms. Local matches are returned before server pages; there is no single global ranking across the two sources.
- UI contract: `Snapshot.search.{query,results,loading,exhausted,limited,error}`, UTF-8 `match_ranges` within bounded plain `snippet`; `MatrixApp::search_messages(query)`, `search_more()` and `open_event(room,event_id)`. `Snapshot.focused_event` becomes populated once the SDK event-focused timeline contains the target; the UI owns scrolling to it. Room/sender/has-file filters are supported; the server's has-file results are filtered after retrieval, so empty pages may have continuations.
- New queries cancel the previous 250 ms debounce/request and check account instance plus request ID on completion; account teardown aborts search work. Query/result/cursor types have no Debug or serialization and errors contain only fixed codes. Event context runs in the selection task and is cancelled/rejected on selection/account changes. Views and controls remain the UI contributor's work.
- Compilation: locked strict all-target/all-feature Clippy and rustfmt check passed. Unit/offline tests: all-feature workspace suite passed (including debounce replacement and Unicode snippet boundaries). A scheduling-sensitive gallery test discovered in this run was corrected in Task 1's local commit to await asynchronous abort completion.
- Live integration: isolated Synapse 1.164.2 search test passed: 55 unencrypted results over server pages, cached encrypted results with limitation flag, global mixed-source search and SDK context opening an older event in both kinds of room. No query/body output is emitted by application search code or fixture errors.
- Remaining: search views and physical scrolling/focus, Element interoperability and Linux/Windows execution. Cached-history coverage is intentionally limited; server ranking/tokenization may differ from local substring matching.

M4b core — Task 3, presence data (2026-11-11):
- Core `presence`, `presence_updates` and `set_presence` use SDK/ruma endpoints. Bare offline responses are ambiguous (including Synapse with presence disabled), so they produce unknown/absent data; only positive server evidence enables presence. This is conservative capability detection, not an advertised Matrix capability or a fabricated offline state. Failed observations clear stale data.
- UI contract: `Snapshot.presence_enabled`, `own_presence: Option<Presence>` and `presence: HashMap<UserId,Presence>`; `Member.presence`, `RoomSummary.partner` (only an unambiguous single DM target) and `RoomSummary.presence`. `Presence` contains Online/Unavailable/Offline, optional absolute `last_active_ms`, and optional `status_msg`. `MatrixApp::set_presence(state,status)` and `observe_presence(visible_user_ids)` are the UI action contracts. Status messages are excluded from Debug output.
- The UI must pass its visible DM/member set (at most 64; server enforces sharing permissions). There is no whole-account member scan. Native sliding sync has no presence extension; one cancellable observer polls own/visible presence every 30 seconds with four concurrent user requests and a 20-second iteration timeout. Room/account changes abort it and reject completions by instance/request ID; new selections initially observe only self until the UI supplies visible IDs.
- Compilation: locked strict workspace/all-target/all-feature Clippy and rustfmt check passed. Unit/offline tests: workspace all-feature suite passed (106 tests; opt-in integration skipped), including ambiguous offline handling and status Debug privacy.
- Live integration: isolated Synapse 1.164.2 runs with presence enabled and disabled both passed. Checks cover own online, visible shared-room partner away/status, offline setting, empty visible set and the 64-user bound. Normal isolated fixture configuration was restored while stopped.
- Remaining: visible-set UI wiring, real-server behavior outside Synapse, physical status controls and Linux/Windows execution. An enabled server returning only bare offline remains unknown until positive evidence appears; no status is synthesized.

M4b core — Task 4, Reduce Motion and typing data (2026-11-11):
- Platform `reduce_motion()` and `reduce_motion_changes()` share a single process-wide latest-value watch channel. macOS reads `NSWorkspace.accessibilityDisplayShouldReduceMotion` and registers for its accessibility-display notification. Linux keeps a portal connection and observes `org.freedesktop.appearance/reduced-motion`, falling back to GNOME/GTK animation settings where exposed; reconnects on bus loss. Windows reads `SPI_GETCLIENTAREAANIMATION` and polls every two seconds. Unsupported reads default to false.
- UI contract: `Snapshot.reduce_motion` remains current while signed in or out; there is no animation implementation. `ConversationSnapshot.typing` and `Snapshot.typing` are `Vec<TypingUser {user_id,name,avatar}>`, sorted by user ID, deduplicated, excluding self, capped at three; `typing_others` counts the remainder. `model::typing_text` retains the accessible frontend string, including the additional count. Member-profile updates refresh names/avatars.
- Native observer registration and the Win32 output-pointer call require small documented unsafe blocks. Only the platform crate changes from inherited forbid to deny, with narrowly scoped module allowances; core/frontend keep forbid. No raw pointers escape the native adapters.
- Compilation: locked strict workspace/all-target/all-feature Clippy and rustfmt check passed on macOS. The actual Linux observer source also typechecked in a separate macOS harness; this is not a Linux build. Unit/offline tests: locked all-feature workspace tests passed, including deterministic/capped typing data, shared subscription channel and actual macOS workspace preference reads.
- Live integration: all five isolated Synapse service regressions passed, including real typing start/stop, members, receipts, E2EE and pagination. Native motion preferences do not require a Matrix server.
- Remaining: physical OS preference changes and delivery of accessibility notifications, Linux portal execution and Windows compilation/runtime; static/reduced-motion UI rendering belongs to the UI contributor.

M4b core — Task 5, cold-launch notification activation (2026-11-11):
- macOS installs one UserNotifications delegate before GPUI application launch. Notifications carry versioned, bounded metadata (stable homeserver/user/device identity, room/thread IDs and timestamp) in OS userInfo; no token, message body, query or callback URL is added to the activation metadata. The native adapter no longer relies on the delivery library's process-scoped response delegate. Framework callback/dictionary unsafe operations are isolated and documented in the macOS adapter.
- Cold responses use a latest-value queue retained until an account is connected and its room list contains the joined target. Validation checks stable identity, parsed IDs, joined SDK room state, metadata age (24 hours) and current account epoch; pending room discovery times out after 60 seconds once connected. Old acknowledgments cannot clear a newer click. Accepted targets bind to the new account instance and use the existing `Snapshot.notification_click` / room-and-thread action path.
- Normal process shutdown preserves delivered notifications so they can launch the app. Explicit logout/account replacement/revocation still dismisses the running account's tracked notifications; logout clears deferred activation and all delivered macOS notifications, including those retained from earlier processes. No new UI controls or layouts. `main.rs` has only the early lifecycle installation call.
- Compilation: locked strict workspace/all-target/all-feature Clippy and rustfmt check passed. Unit/offline tests: locked all-feature workspace suite passed; metadata expiry/size/unknown fields, late subscription, coalescing and acknowledgment ordering are covered. Unbundled tests do not request notification permission.
- Live integration: isolated Synapse 1.164.2 same-device encrypted-store restore passed, confirming changed instance ID but stable notification identity, accepted restored-room target and rejected wrong-account/invalid-thread targets. Encrypted notification/private-preview regression also passed.
- Physical acceptance remains unverified: signed-bundle native notification delivery, quit/terminated-app click launching the registered bundle, restore timing and room/thread opening, OS Focus and sound. Linux/Windows retain running-process callbacks only; cold activation is not implemented on those OSes. Cross-process OS launch is not established by unit/service tests.

M4b core — Task 6, HTML composition and native rich-text data (2026-11-11):
- `rich_text::compose` uses Ruma's Markdown support for bold/italic, inline/fenced code, links, lists and quotes (also tables/strikethrough). `Conversation::send`, including thread timelines, now sends `m.text` with `org.matrix.custom.html` and a sanitized `formatted_body`. The plain `body` is the exact composer input, retaining Markdown syntax as Ruma does. Plain input is escaped for its HTML representation. Formatting errors are typed, with frontend-owned wording.
- Incoming HTML uses Ruma's strict Matrix allowlist, removes rich-reply fallback and dangerous subtrees, rejects active URL schemes and remote inline-image sources. Parser diagnostics are suppressed only within synchronous parsing/sanitizing so malformed content cannot enter application logs. Input caps: 64 KiB composer text, 256 KiB HTML, depth 32, at most 4,096 projected nodes; invalid/oversized incoming formatting leaves the existing plain body available.
- UI contract: every `MessageRow` / frontend `Message` has `rich: Option<Arc<RichText>>`, with owned `RichNode::Text`, `Span` and `Block` trees; typed strong/emphasis/code/links/spoilers and paragraphs/headings/quotes/lists/tables/code blocks. No raw markup enters snapshots and no separate content index is persisted. Formatting-only changes participate in message-group fingerprints. Rendering, toolbar and formatting controls remain the UI contributor's work.
- Compilation: pinned Nix rustfmt and locked strict workspace/all-target/all-feature Clippy passed; Cargo.lock includes Ruma's Markdown dependencies. Unit/offline tests: all-feature workspace suite passed (115 tests), including Markdown/HTML/plain round trip, Element-style markup, script/iframe/javascript-link rejection, size bounds and formatting-only fingerprint invalidation.
- Live integration: all five isolated Synapse 1.164.2 service checks passed, including encrypted Markdown composition/decryption with exact plain fallback and rich projection, same-device restore, and Element-style formatted-event projection through SDK timelines. The HTML fixture is generated test content, not a running Element client.
- Remaining: native rich-text rendering, toolbar, physical keyboard/accessibility behavior, actual Element interoperability and Linux/Windows builds/runtime. No M4b milestone exit is claimed; UI and physical acceptance remain separate.

M4b core — Task 1 gallery review corrections (2026-11-11):
- Removed the unsafe `contains_url` optimization: a real encrypted source forwarded into a plain room is now included and decrypted by the gallery. Event-type filtering and bounded client-side filtering remain. Cursors also bind the account instance; response chunk limits are enforced even if a server ignores the requested limit.
- Gallery and timeline images share the existing single decoder permit, with cancellation before scheduling blocking decode work. Gallery actions start in UI invocation order and carry the expected account/room; a pending room switch cannot open or load the old room's gallery. The artificial 2,000-item exhaustion cap was removed; no background auto-pagination was added.
- UI fields/actions remain as listed in Task 1. Compilation: locked strict workspace/all-target/all-feature Clippy and rustfmt check passed. Unit/offline suite: 118 workspace tests passed, including account/selection/request admission, asynchronous selection cancellation, oversized server responses and cancellation while waiting for the shared decoder.
- Live integration: isolated Synapse viewer/gallery regression passed again, including encrypted media forwarded to a plain room and in-memory decryption. Physical platform and Element checks remain unverified, as above.

M4b UI on the core contracts (2026-11-11, Claude):
- Review of `feature/chat-polish-core` (7 commits): no blocking defects. Sanitizer uses Ruma's strict allowlist with scheme/mxc checks and budgets; search keeps no index and only scans already-decrypted cache for encrypted rooms; no bodies, queries or tokens logged. Follow-ups: plain messages always carry an HTML `formatted_body` (Element sends plain when Markdown changes nothing); cached encrypted search results carry only the sender ID.
- Rich text: sanitized `RichText` is converted to escaped Markdown for Kit's selectable text view (bold, italic, strike, code, code blocks, links, lists, quotes, headings, tables). Message text can never inject its own formatting; spoilers stay hidden ("[spoiler]") until a reveal control exists. Links show their real destination in a tooltip and open only http(s)/mailto.
- Motion: one module (`motion.rs`) with a single ease-out curve; side panels slide/fade in, dialogs and the emoji picker rise, room switches crossfade, the typing indicator pulses (30 fps cap). Only opacity and relative offsets animate. Reduce Motion (from the platform watch) shows final states; UI tests settle the same way.
- Typing: up to three typers' avatars, animated dots and the accessible "… is typing" text.
- Gallery: Info › Files shows the room gallery (Images grid / Files list) with skeleton loading, error with retry, empty states and Load Older; images open in the viewer in gallery order. Thumbnails reuse timeline previews when the event is loaded, otherwise a filename tile (gallery thumbnail fetching is pending). Offline previews keep the loaded-timeline list.
- Search: Return in the title search (or the "Search messages for …" sidebar row) opens a Search panel with All Rooms / This Room, Has Files, highlighted snippets, an encrypted-history notice, Load More and empty/error states. Opening a result selects the room, scrolls to the event and tints it.
- Presence: your avatar opens an account menu (Your Profile, Online / Away / Invisible, Clear Status Message, Account Settings…); status dots on your avatar, DM rows and members, only when the server shares presence. The visible set (DM partners, plus members when the Members tab shows; ≤64) is handed to the observer only when it changes.
- Locked fmt, strict all-feature Clippy and 121 workspace tests pass (incl. rich-text Markdown escaping, motion easing, search panel flow). Offline preview captures reviewed. Not verified live: gallery with real media, search against a server, presence changes, cold-launch activation, Reduce Motion toggling on a physical Mac.

Fixes from the first live run (2026-11-11):
- This device's verification uses `is_cross_signed_by_owner()` instead of `is_verified()` (true for the SDK's own, locally trusted device), so an unverified session no longer reads "verified"; other sessions show "Unknown until this device is verified" until it is.
- Conversation search matches all conversations (Home, every space, DMs), not only the shown space; choosing a result clears the query.
- Title bar: traffic lights centred on the 40pt bar (main and Settings), no filled sidebar toggle, and no connection dot while connected.
- Not yet re-verified on the live account: verification state after cross-signing with Element.

Rename to Twayblade (2026-11-11):
- Crates `twayblade-core`, `twayblade-platform`, `twayblade` (app and binary); `Twayblade.app`, bundle ID `com.twayblade.desktop(.development)`, keychain service `twayblade(.development)`, data/config folder `Twayblade`, device name "Twayblade desktop", notification IDs and test fixtures (`twayblade.test`). GitHub repository `zerw0/twayblade`.
- One-time migration on first live launch: the pre-rename `MatrixClient` data/config folders are moved (skipped if the new ones exist or the old app is running), and the keychain session is copied from the old service and then deleted. Cold-launch activation from notifications posted before the rename is not carried over.
- `.github/workflows/build.yml` builds release binaries (macOS arm64 app zip, Linux x86_64 tarball, Windows x86_64 zip) on pushes to main and manual runs, and publishes a GitHub release with checksums for `v*` tags. Builds are unsigned (macOS ad hoc). The workflow has not run yet.
- Locked fmt, strict Clippy and the workspace tests pass after the rename; the migration has not yet been exercised on the live account.

M4b core 2 — Task 1, public room discovery (2026-11-11):
- `Account::public_rooms(DirectoryQuery)` uses SDK POST /publicRooms with server/filter/network and 1–100 result pages; remote POST rejection permits GET fallback for Matrix rooms, with bounded local filtering on each GET page. Cursors bind account instance, server, filter, network and transport. Repeated cursors terminate pagination. Typed errors cover invalid input, not found, forbidden, unreachable, invite/knock requirements and unsupported operations. Discovery requests time out after 20 seconds; preview/network/join/knock after 30 seconds.
- Validated `RoomTarget::parse(input, via)` accepts aliases, IDs, matrix.to and matrix: room/event links, rejects user/external targets and caps routing servers at 16. Preview uses SDK room summary/state fallback, then at most four directory pages; world-readable history is a read-only page capped at 20 text events and 4,096 characters per body. Browsing/preview never joins. Join and knock are explicit; knock does not select a room. Summary guest access is conservatively false when unavailable.
- UI contract: `Snapshot.directory.{server,filter,network,networks,networks_error,rooms,loading,exhausted,error}`; each room exposes `room_id,alias,name,topic,avatar,members,world_readable,guest_can_join,join_rule,is_space,joined`. `Snapshot.room_preview.{preview,loading,error}`; preview contains `room,messages,history_error`. `MatrixApp::{browse_directory(server,filter),browse_directory_network(server,filter,network),directory_more(),preview_room(target),join_room(target),knock_room(target,reason)}`. Browse debounces 250 ms, caps accumulated rooms at 1,000 and cancels superseded work. Room/account changes cancel requests; account instance and request IDs reject stale completions. Membership operations reserve an account/selection/request ticket synchronously before dispatch, recheck admission/completion and are cancellable. The application action generation also rejects completions delayed in the UI queue after newer actions or room selections. Space joins hand off to the existing hierarchy action, marked placeholder wiring. No views changed.
- Third-party protocols are available only from the authenticated homeserver's client API; remote network lists are empty. Unsupported local protocol endpoints return empty lists. No speculative remote protocol endpoint is used.
- Compilation: locked macOS fmt and strict workspace all-target/all-feature Clippy passed. Unit tests: locked workspace all-feature suite passed (123 tests; 22 opt-in service tests ignored). Final queue-admission/completion regression also passed in the 128-test final workspace suite. Linux/Windows cross-target standard libraries are not configured locally; their builds and runtime remain unverified.
- Live integration: isolated Synapse 1.164.2 directory test passed pagination/filter, account cursor rejection, bounded pages, remote-server error, alias/matrix.to join, world-readable preview without membership and knock. The fixture explicitly permits directory publication (new Synapse defaults deny it). Final rerun passed after cancellation hardening. GET fallback against an actual POST-rejecting federated server, third-party bridges, MSC3266-only servers, Element comparison and physical GUI/platform checks remain unverified. No milestone exit claimed.

M4b core 2 — Task 2, gallery thumbnails (2026-11-11):
- `Account::gallery_thumbnail(item,cancel)` prefers the event thumbnail source, otherwise requests a 168×168 SDK server thumbnail. Encrypted sources are decrypted only in memory by the SDK; its disk media cache is disabled. The first image frame is decoded with the same single semaphore as timeline/viewer work, capped at 8,192 pixels per dimension, 64 MiB decoder allocation, 16 MiB encoded input and 168 pixels output. Known oversized encrypted originals are rejected before downloading; SDK responses are buffered before the encoded-input check. Fetch timeout is 15 seconds. Failures leave entries absent.
- UI contract: `Snapshot.media_thumbnails: HashMap<event_id,Arc<Vec<u8>>>` contains PNGs, with a 256-entry recency cache (under 32 MiB of PNG payload). Existing `MatrixApp::load_media` / `load_more_media` actions fetch thumbnails automatically, prioritizing each new page; at most 256 missing items per request, sequential fetching, no hidden pagination. No new UI action or view changes. Room/account changes clear the map/order, signal decoder cancellation, abort work and reject stale account/selection/request completions.
- Compilation: pinned macOS fmt and strict locked all-target/all-feature workspace Clippy passed. Unit tests: locked all-feature workspace suite passed (125 tests; opt-in tests ignored); bounds, cache eviction and cancellation while waiting for the shared decoder are covered. Linux/Windows target standard libraries are unavailable in the pinned shell; cross-platform compilation/runtime remain unverified.
- Live integration: isolated Synapse 1.164.2 viewer/gallery run passed, including plain/encrypted preferred and original-source thumbnails, GIF/WebP first-frame thumbnails and SDK media-store absence after fetching. Physical gallery display, actual Element media and platform acceptance remain unverified.

M4b core 2 — Task 3, plain composition (2026-11-11):
- `rich_text::compose` now returns plain `m.text` when Ruma's Markdown parser returns no formatting. Serialized plain events omit both `format` and `formatted_body`; formatted input retains sanitized HTML and the exact composer fallback. This supersedes the earlier all-HTML composition behavior.
- UI contract is unchanged: existing `Conversation::send` / MatrixApp send actions; `Message.rich` remains absent for plain input. No new snapshot fields/actions or view changes.
- Compilation: pinned macOS fmt and strict locked all-target/all-feature workspace Clippy passed. Unit tests: locked all-feature workspace suite passed (125 tests; opt-in tests ignored); plain Unicode, ampersands/newlines, serialized omission and formatted round-trip checks. Linux/Windows builds/runtime remain unverified (target standard libraries unavailable locally).
- Live integration: all five isolated Synapse 1.164.2 service tests passed, including plain and formatted messages through encrypted SDK timelines. Actual Element-client interoperability and physical platform checks remain unverified.

M4b core 2 — Task 4, cached-search sender names (2026-11-11):
- Encrypted cached search resolves each distinct room/sender through `Room::get_member_no_sync`, using the SDK member store without member-network fetches. Per-room name lookup caches are bounded by the existing 1,000-result cap. Missing, empty or unreadable display names keep the full Matrix sender ID; no plaintext index or additional persistence.
- UI contract: existing `Snapshot.search.results[].sender_name` now contains the stored display name where available; `sender` remains the stable ID. `MatrixApp::search_messages` / `search_more` are unchanged; no new fields/actions or view changes.
- Compilation: pinned macOS fmt and strict locked all-target/all-feature workspace Clippy passed. Unit tests: locked all-feature workspace suite passed (125 tests; opt-in tests ignored). Linux/Windows compilation/runtime remain unverified (target standard libraries unavailable locally).
- Live integration: isolated Synapse 1.164.2 search regression passed, including encrypted cached display names and fallback to sender IDs after removing the profile name. Actual Element-client interoperability and physical platform checks remain unverified.

M4b core 2 — Task 5, spoiler/status/verification contracts (2026-11-11):
- Sanitized `SpanKind::Spoiler {index,reason}` now carries a deterministic zero-based preorder index, unique within each message, including nested spans. UI reveal state should use `(message.event_id,index)`; repeated projection keeps indices and each message starts at zero. Existing `Snapshot.messages[].rich` / thread messages carry the data; no new snapshot root fields or reveal views/actions.
- `Account::set_presence(state,Some(status))` trims whitespace, caps Unicode scalar characters at 256 and rejects all control characters (including edge newlines/tabs) with typed `PresenceError::InvalidStatus`. `None` clears through the SDK. Existing `MatrixApp::set_presence` and `Snapshot.own_presence.status_msg` / `presence[].status_msg` are the UI contracts; frontend backend wording explains invalid status. No status content enters Debug/log output.
- Kept `SecuritySnapshot.verified` based exclusively on `is_cross_signed_by_owner()`. The real crypto regression asserts an existing cross-signing identity plus locally trusted own device still reports unverified, then becomes verified after recovery; another fresh device remains unverified before SAS and becomes verified afterward. `Snapshot.security.verified` and existing security actions are unchanged.
- Compilation: pinned macOS fmt and strict locked all-target/all-feature workspace Clippy passed. Unit tests: locked all-feature workspace suite passed (127 tests; opt-in tests ignored), including spoiler stability and Unicode/status boundaries. Linux/Windows compilation/runtime remain unverified (target standard libraries unavailable locally).
- Live integration: isolated Synapse 1.164.2 security/SAS/recovery and both presence-enabled/disabled runs passed, including trimmed status, invalid status and clearing. Actual Element interoperability, reveal controls, status-entry UI and physical platform checks remain unverified.

Permissive software license (2026-11-11):
- Added the MIT license at the user's request, with an SPDX `MIT` workspace license inherited by all three crates and a README link. Third-party dependency licenses remain their own. No snapshot fields, actions or views change.
- Validation: pinned-shell locked Cargo metadata confirms all three crates inherit MIT; repository diff check passed. This documentation/manifest-only change adds no runtime behavior or integration/platform acceptance claim.

Review of M4b core 2 (2026-11-11, `fix/core-2-review`):
- Switching rooms no longer cancels directory browsing, previews or a join in progress, and no longer clears the preview. Previously the list could be left empty with no loading or error state. A join that finishes after you switch rooms still doesn't navigate to the joined room.
- Directory networks are kept across filter changes and fetched again only when the server changes or the last fetch failed. Previously each search keystroke cleared the list and refetched it.
- Joining uses the space/room type already loaded in the preview or directory list and fetches a preview only when the type is unknown.
- Preview history contains only `m.text`/`m.notice`/`m.emote`, oldest first. Previously it was newest first and included file names from image and file events.
- Gallery thumbnails are fetched only in the Images tab. Without an event thumbnail, only `image/*` originals are scaled. Encrypted originals with an unknown size are rejected before download, which closes an unbounded buffered download.
- A status message of only whitespace clears the status instead of sending an empty string.
- Checks: fmt, strict Clippy and the workspace tests pass. The isolated Synapse directory, viewer/gallery and presence (enabled/disabled) runs pass. No physical or Element checks.

Stacked attachments and sync restart (2026-11-11):
- The composer keeps up to 10 attachments (256 MB in total, since drafts are held in memory until sent) as tiles above the message field: image thumbnails or file name and size, each with a Remove button, and an "Adding…" tile while a file is read. The picker allows several files; drag and drop and paste add every file. The "Send attachment?" confirmation is gone: Return or Send sends the attachments in order and then the text, if any. Sending waits until every tile is ready. If an attachment fails, the rest stay stacked and the error appears in the status line. Room or account changes clear the stack.
- The SDK sync service stops for good after an error or unexpected termination. While the session is active it is now restarted with backoff (1 s, doubling up to 60 s), showing "Offline · reconnecting…". Previously the app stayed disconnected until relaunched.
- Checks: fmt, strict Clippy and the workspace tests pass (offline UI test for the tray and its removal; restart backoff unit test). Isolated Synapse sync service tests pass. Sending stacked attachments to a live server, and the restart path against a real failing server, have not been exercised.

M5 — native calls, phase 1 technical layer (2026-11-11):
- The superseded macOS WebKit spike has been removed after native/Element Call encrypted audio passed. The app no longer includes `call-host`, widget capabilities/bridge, Element Call assets or a WebKit fixture. Claude owns GPUI call views; this work changes only technical contracts and existing action wiring.
- `core::calls` uses matrix-sdk 0.19.1 / Ruma for `org.matrix.msc3401.call.member` (MSC3401/4143; per-user/device state keys), SDK active-membership parsing/expiry, `.well-known` `org.matrix.msc4143.rtc_foci`, OpenID and encrypted to-device sending. The oldest membership selects one LiveKit focus. MSC4195 deployment compatibility uses lk-jwt-service `/sfu/get`; MSC4140 delayed state targets Ruma's Synapse-compatible revision `3ee73abe5f81252b00877cfb5db941ee9aa6c18d`, with 15-second delayed leave, five-second restart and two-minute expiry fallback. This phase does not implement MatrixRTC 2.0 sticky events, hashed identities or multi-SFU migration.
- Per-participant 16-byte CSPRNG media keys travel as `io.element.call.encryption_keys` only to exact current call-member devices through SDK Olm encryption. Incoming events require authenticated sender-device metadata and current room/call membership. Current `media_key`/`member_id` and deprecated fields are supported; timestamps/session claims are validated when present. Departures rotate the sender key/index. Media E2EE is mandatory; clear publications are rejected before subscription.
- New `twayblade-media` isolates LiveKit 0.10.1, libwebrtc 0.4.0 / webrtc-sys 0.3.53 and prebuilt `webrtc-a545de4` from Matrix state and GPUI. HKDF, salt `LKFrameEncryptionKey`, ratchet window 10, key ring 256 and AES-GCM-128 interoperate with Element Call. Bundled WebRTC BSD/dependency notices are tracked and packaged. Microphone/speaker use the native ADM, camera uses AVFoundation behind a small capture boundary, and `platform` owns typed permission errors/prompts. Device lists are refreshed and selection validates stable IDs; physical hot-plug recovery is unverified.
- UI contract: `Snapshot.call` keeps room/kind/phase/error and adds device-level participants (name, speaking, mute, video, quality), local media/device state, input/output/camera lists and video-track descriptors. Existing start/join/leave/mute/video actions remain, with `select_input`, `select_output`, `select_camera` and `call_video_handle`. Frames never enter snapshots: one latest-frame handle per track exposes immutable BGRA8 with timestamp/rotation, bounded conversion outside GPUI. Removed tracks permanently retire their handle and cancel its consumer. One account reservation and account/request tickets reject concurrent/stale calls; account/logout/room exit cancel them. No views changed.
- Isolated fixture: digest-pinned Element Call **0.28.2** (`c61fc9e4e04f75690c693d4556d01c09a2b9246f3fc8d35222d3aac36ba29906`), Matrix JS SDK `3f05dc4d104a3dfc753c5e3549a18c74fab4b076`, LiveKit **1.9.0**, lk-jwt-service **0.9.0**, Playwright **1.56.1** / Chromium build **1194**. Browser peer runs the standalone SPA in Compatibility mode with generated tone/pattern capture. Native trust uses the account root-certificate option; browser trust is limited to the fixture leaf SPKI. WebSocket trust bindings match the SFU origin and exact SDK bearer token to keep accounts isolated. No keychain changes. Selected service/trust/evidence/assertion pieces were reused from the unmerged signalling branch, without merging its runtime.
- Compilation/checks: pinned macOS Nix shell `cargo fmt --all -- --check` and strict locked workspace all-target/all-feature Clippy passed. Shell/Node/Python syntax and Compose call-profile validation passed.
- Unit/offline checks: locked workspace all-feature suite passed **141 tests**, with **24** opt-in service tests ignored. Coverage includes CSPRNG generation/departure rotation, authenticated key acceptance and modern/legacy schemas, membership state keys/expiry, oldest-focus selection/refusal to fork, stale account/request tickets, same-SFU account trust isolation and latest-frame bounding/retirement. Offline GPUI tests are distinct from call-view acceptance.
- Isolated interop: final `run.sh call` passed **both** cases (2/2, 141 seconds): native↔native and native↔Element Call 0.28.2. Audio first, then video: both Matrix membership/participant lists, bidirectional encrypted key receipt counts, exact two-identity LiveKit room, decrypted generated-tone signal blocks and new decoded video test-pattern frames on each side. The native case additionally remained joined through a 75-second sustained interval across membership expiry refresh and delayed-leave renewal. This regression caught and fixed an orphaned delayed leave on servers retaining it across state writes. These are real SDK/SFU/browser flows with synthetic capture, not physical media acceptance.
- Packaging: the final macOS native debug `.app` built and signed ad hoc; helper removal, microphone/camera usage declarations and bundled libwebrtc notices are checked. No application launch, call views or physical permission prompts were exercised. Linux/Windows packaging scripts now include the notices but their builds were not run. Commands and full contract: `docs/architecture/CALLS.md`.
- Remaining acceptance: actual microphone/speaker/camera and permission interactions on this Mac need a person. Public-server Element Web, Linux and Windows compilation/runtime are not attempted. Screen sharing, ringing/MSC4075 notifications, device removal, network changes, cross-service focus migration and GPUI view integration remain open. No M5 exit claimed.

Native calls review fixes (2026-11-11):
- Only current call members' encrypted tracks are received. Previously every LiveKit publication was subscribed and departed members' keys stayed installed, so someone who left the Matrix call but stayed in the SFU room kept playing. Departure now unsubscribes their tracks, clears their video slots and overwrites their installed key indices with random material.
- Keys are sent on membership changes and retried only after a failed delivery. Previously the 5-second keepalive resent them to every member.
- An unencrypted publication is left unsubscribed and its member marked `media_blocked`; the call continues. Previously one such publication ended the call for everyone.
- After a rotation the new key is distributed first and used after 5 s (matrix-js-sdk `useKeyDelay`, as in Element Call 0.28.2), avoiding a dropout on every departure.
- Frame-counting evidence sinks are behind the media crate's `evidence` feature (tests only); release builds attach no extra sink per remote track. Remote audio still plays through the platform audio device module; that path is not exercised without a person.
- Checks: fmt and strict Clippy (with and without `evidence`) and the workspace tests pass. `run.sh call` passed 2/2 (133 s): native↔native audio, video, sustained refresh and the new departure case (a member who left the call but stayed in the SFU room sends no further received audio or video frames and has no video slots), and native↔Element Call 0.28.2 audio and video. No physical devices.


M5 — calls phase 2a technical layer (2026-11-12; acceptance partial):
- Branch `feature/calls-ringing` starts at `main` / `433a3f8`. Technical code, tests and documentation only; no GPUI view changes or ringtone assets.
- New calls send `org.matrix.msc4075.rtc.notification`, matching Element Call 0.28.2 / JS SDK `3f05dc4d104a3dfc753c5e3549a18c74fab4b076`. DM `ring` / group `notification`, 90-second lifetime, audio/video intent, explicit recipient mentions and a reference to the acknowledged membership event. Modern slot ID is `m.call#ROOM`, which Element maps to the compatibility membership's empty `call_id`; open slot state requires room permission. Reads also accept `m.call.notify` and `org.matrix.msc4075.call.notify`. This does not implement legacy VoIP.
- Incoming activity now follows SDK sync events, room updates and local expiry deadlines, replacing the backend's one-second room-call polling. `Snapshot.incoming_calls` is bounded to eight newest-first entries, one per room, with caller user/device/name, room, audio/video kind, ring/notification mode, event ID, since/expiry, verified and ringing flags. Exact membership references bind caller devices; SDK decryption metadata establishes verification. Own-user rings from all devices, ignored users, expired events, other slots and nonjoined rooms are excluded. Caller departure, room exit and an own-user membership on any device remove the invitation. Intake is 32 events / 16 KiB each; dismissal tombstones are bounded to 256 until expiry.
- UI contracts: `accept_call(room, video)`, `decline_call(room)` and `dismiss_incoming(room)`; accepting validates the current unexpired invitation under the account guard and joins without notifying. Decline sends `org.matrix.msc4310.rtc.decline` referencing the notification, as the pinned SDK's `sendRtcDecline` does. Dismissal is local only. Outgoing `CallPhase::Ringing { since, expires }` ends on join, decline, leave or expiry; decline/expiry leave with typed end reasons. All timestamps are Unix milliseconds. Existing start/join/leave/media actions remain.
- Incoming ring OS notifications use the existing device enabled/preview/sound and room-mute preferences. Private previews hide caller/room names; bodies contain generic call wording only. Standard OS delivery delegates Focus/DND to the OS without a critical-alert bypass. Live macOS categories, freedesktop actions and Windows toast actions expose audio accept and decline; opening remains the default/cold action. Actions revalidate account, exact invitation and expiry. Notification removal/account cancellation cleans up outstanding deliveries. UI ringtone playback remains Claude's work.
- macOS hot-plug uses CoreAudio device/default-route property listeners and AVFoundation connect/disconnect notifications; other targets refresh at two seconds. Removed selected devices fall back only to an available system default. No replacement/default-discovery failure reports `DeviceUnavailable` and mutes/stops capture. Existing permission grants are checked without prompts every two seconds; revocation mutes/stops the affected track with a typed permission error. `CallState.events` holds at most 16 serial-numbered `DeviceFallback { kind, from, to }` / `PermissionRevoked { kind, error }` entries for toast deduplication.
- LiveKit signal/ICE recovery reports `Reconnecting`, retains the current sender index (including a pending delayed key switch), refreshes membership and redistributes keys after recovery. Transient Matrix keepalive failure also gets recovery rather than immediately dropping media. A 45-second budget permits several retries while staying below the 120-second membership fallback; timeout reports `ReconnectTimeout` and performs bounded cleanup. In-flight bounded signalling can finish before the deadline is observed. Reuse a retained delayed-leave ID, or replace it only after `NotFound`; cancellation clears membership and cancels the timer. A failed cancellation retains the account reservation through the old 15-second timer, preventing it from deleting a subsequent local session. Unreachable-server cleanup still depends on server delayed leave/expiry.
- Security: room signalling remains SDK-encrypted in encrypted rooms; no plaintext workaround for interop. Matrix access tokens never reach the SFU, and JWT/OpenID/key handling and the log allowlist are unchanged. Keys still go only to current member devices through SDK Olm.
- Compilation/offline verification: pinned macOS Nix-shell formatting, strict locked workspace/all-target/all-feature Clippy and production library/binary Clippy without media `evidence` passed. The production dependency graph confirms no evidence feature. Locked all-feature workspace tests passed **190**, with **30** opt-in tests ignored. `run.sh services` passed **6/6**, and `run.sh room-open` passed **4/4** isolated cases. Unit coverage includes lifetime/skew/expiry, own-user and ignored filtering, bounds/order/dismissal, accept/decline/expiry transitions, answer on another own device, caller departure and default-device fallback. Physical device behaviour is not established by these tests.
- Isolated interop verification: final-source `run.sh call` passed **3/3** in **171.67 seconds** (both existing media cases plus four qualified ringing scenarios). Native/native also passed a 20-second SFU pause: `Reconnecting` was observed, memberships stayed active, and decrypted audio/video resumed in both directions. An earlier combined run lost browser-side audio during repeated Element reconnections; the focused case and subsequent full run passed. This intermittent failure is recorded rather than treated as physical or reliability acceptance. The fixture grants ordinary members slot-write and room-notification permission explicitly; other state power levels remain unchanged.
- **Element acceptance gap:** the pinned standalone SPA emits `Room.timeline` before Megolm decryption, but its outgoing decline lifecycle listens only to that initial event. The untouched standalone decline case failed. The new ringing fixture is explicitly a **simulated host decrypted-event boundary**: it forwards the original decrypted decline through the room emitter to the unchanged Element lifecycle. It uses the real pinned SPA/SDK, event IDs, encrypted delivery and memberships, but is not live Element Web/widget or standalone encrypted-decline acceptance. Set `MATRIX_TEST_ELEMENT_STANDALONE=1` with the ringing test filter to reproduce the unforwarded path. No silent encryption downgrade or product web runtime was added.
- Remaining acceptance: Claude's call views/ringtone integration; physical microphone/output/camera removal, permission revocation, notification actions and Focus/DND on macOS; actual Wi-Fi/network changes and homeserver-wide outages; live Element Web/host interoperability and the standalone decline issue; Linux/Windows builds/runtime and default-device discovery where the media backend does not expose a default. Screen sharing and MatrixRTC sticky-event reception/replay remain outside this phase. No M5 exit or complete phase-2a interoperability acceptance is claimed.
- Review fixes (2026-11-12, Claude): the integration fixture grants members slot-write, but ordinary group rooms do not, so a refused slot state event or failed notification no longer fails the call; the call starts without ringing. Ring expiry and declines end only direct calls; a group call stops ringing and continues. Default-device selection at start is best effort (CoreAudio UIDs may not match WebRTC IDs). Permission polling treats only Denied/Restricted as a revocation, so platforms that report Unsupported are not muted every 2 s. Incoming-call notification copy: DMs show the caller and "Incoming (video) call"; groups show the room and "<caller> started a (video) call". Re-verified: Clippy, workspace tests, `run.sh call` 3/3.
- Call views (2026-11-12, Claude, `ui/calls.rs`): header voice/video buttons (disabled with a reason in unencrypted rooms or during another call) or a "Join call · N" button when others are in the room's call. A call stage above the timeline: participant tiles (you first; avatar or live video; speaking border; muted marker; unencrypted-media overlay; "+N" past eight), a dashed "Ringing…" tile for the callee in a direct call, dimmed tiles while reconnecting, status (Connecting / Calling X / Waiting for others / Reconnecting / duration and people), mute, camera, a devices menu (microphone, speaker, camera) and Leave (Cancel while ringing). Ended calls leave a summary with the reason and Rejoin (others still present) or Call Again. A strip with Return/Leave shows a call in another room. The newest unseen device/permission event shows as a dismissible notice. An incoming-call card (top right) offers Decline / Accept (Audio / Video for video calls) for rings and Dismiss / Join for group notifications, with "+N more". Video tiles are separate views that poll their latest-frame handle each display refresh and release GPU images when replaced or dropped. Previews: `--preview-call`, `-call-group`, `-call-ringing`, `-call-ended`, `-call-reconnecting`, `--preview-incoming`. Verified: Clippy, workspace tests and preview captures. Not yet done: ringtone, live video against a real call, keyboard shortcuts for mute/leave, accessibility review of the stage.

M4b — message editing technical contracts (2026-11-11):
- `Message.editable` covers our own sent text/notice/emote events (checked in core: SDK 0.19.1 `is_editable` omits notices, which `Timeline::edit` accepts; no SDK fork). Queued/failed local echoes, media, redacted and state events are excluded. Replacements remain SDK `Timeline::edit` / `EditedContent`, encrypted and queued by the SDK, with no hand-built replacement events.
- `Conversation::{edit_source, edit}` use event IDs. The source is the latest plain body: exact original Markdown for this client's composition; imported HTML formatting absent from that body is lost on a changed re-save. Identical sources return `EditError::Unchanged` without enqueueing; other errors are `NotEditable`, `TooLong`, `Failed`. Changed text uses the same sanitized Markdown composition as send, preserving notice/emote type. SDK updates supply latest plain/rich content and the edited marker, including other-device and thread edits.
- `Conversation::latest_editable(root)` and `previous_editable/next_editable(from)` navigate eligible loaded events in SDK order. Use the main conversation with `None`, or the thread conversation with its root; thread roots are excluded from thread navigation. No implicit pagination.
- Claude's composer/menu/keyboard contracts: `MatrixApp::{begin_edit(event_id), save_edit(text), cancel_edit()}`; `Snapshot.editing` holds `{event_id, thread_root, original}`, with `edit_error` (backend wording) and `edit_saving`. Failed and unchanged saves retain the edit state; views must retain their current draft separately. Selection/thread/account changes and cancellation invalidate request tickets and abort pending work. No views or layout changed.
- Verification: pinned macOS Nix shell formatting check and strict workspace/all-target Clippy passed. Workspace tests passed (106 tests; 25 service tests ignored). `run.sh services` passed all 6 isolated cases, including encrypted plain/Markdown/notice/emote edits, cross-device incoming/outgoing latest versions, thread rich-content edits, stable IDs, navigation, unchanged suppression and queued/media/redacted/foreign rejection. Backend unit checks cover retained retry state and stale tickets. Physical composer/keyboard, Element UI and Linux/Windows acceptance remain open; no milestone exit claimed.

M4b — larger timeline previews technical contracts (2026-11-11):
- Core `PreviewSize::{Default, Large}` exposes explicit long-edge physical pixels (512 / 1280); display scale and layout stay with the views. `Conversation::attachment_preview(id, size, cancel)` requests a scaled SDK homeserver thumbnail, falling back to the SDK-decrypted original. Previews are first-frame PNGs, decoded off-thread through the shared single decoder semaphore, with 64 MiB encoded/decoded-allocation and 8192-pixel dimension limits. The default-size compatibility adapter also uses that decoder. The full-size viewer is unchanged.
- One selected-room preview LRU covers main timeline and open thread, across sizes: at most **16,777,216 total pixels** (64 MiB RGBA equivalent) and 256 entries, including a metadata bound for tiny images. Eviction clears references in current message snapshots too. Preview bytes remain in memory; SDK persistent media caching is disabled. Fetch concurrency is four, viewport lists at most 128 IDs per surface. Account/selection/request cancellation rejects stale completions and cancels decode waiters.
- Claude contracts: `Backend::select_room_with_preview_size(room, size)` passes the size on room selection; `Backend::{preview_size, set_preview_size}` read/persist `Preferences.chat.preview_size` without changing other settings. `MatrixApp::set_timeline_preview_size` also updates its preference model. `Snapshot.preview_size` and `PreviewSize::pixels()` describe physical pixels.
- `MatrixApp::timeline_preview_viewport(thread_root, message_ids)` (backend also takes room/account) reports **stable `Message.id` values currently visible**; main uses `None`, thread uses its root, empty lists clear demand. Size switches refetch only that demand, reuse size-keyed cached entries, and replace `Attachment.preview_png` in place without rebuilding the SDK timeline or changing IDs/selection. Old whole-history eager thumbnail fetching is removed. Views must supply viewport reports before timeline/thread previews load; Claude owns that wiring and the Settings control. No views/layout changed.
- Verification: pinned macOS Nix shell formatting check, strict workspace/all-target Clippy and default-feature workspace tests passed (108 tests; 24 service tests ignored). Unit tests cover 512/1280 scaling/aspect ratio, cache pixel/entry bounds, LRU and replacement accounting, released snapshot references, preference round trips, cancellation and unchanged identities/selection. `run.sh viewer` passed: plain and encrypted 2048×1024 images switch Default → Large → Default with larger previews, correct caps/aspect ratio and stable IDs; existing animated viewer, uncached media, explicit save, corrupt and >8k checks also pass. Physical Mac Settings/scroll/HiDPI acceptance, viewport/Settings view integration and Linux/Windows builds/runtime remain open; no milestone exit claimed.

M4b — editing and image-size UI (2026-11-12):
- Editing: an Edit (pencil) action on your own editable messages in the timeline and threads; ↑ in an empty composer edits your newest message, ↑ at the start / ↓ at the end step between your messages, and ↓ past the newest returns to the draft. The composer shows an edit bar ("Editing message · Return to save, Escape to cancel", Saving…, or the error) with Cancel; Return saves, Escape or Cancel leaves. Whatever you were typing is set aside and restored after saving or cancelling, also when you switch rooms or close the thread. A failed save keeps the edited text for retry. The send button reads Save while editing. ↑/↓/Escape are taken from the text field's own actions only when they start, step or end an edit.
- Image size: Settings › Chat › "Images in conversations" (Default / Large). Images keep their shape inside 280×180 (Default) or 560×420 (Large) points. The setting is applied in memory and saved through the app's preferences writer; the backend no longer writes the preferences file itself (it raced other settings saves).
- Previews follow the screen: each frame records the image messages rendered in the main timeline and the open thread, and the set is reported per surface once it has been stable for 150 ms, so scrolling doesn't keep cancelling fetches. At most 128 per surface.
- Checks: fmt, strict Clippy and the workspace tests pass, including offline UI tests for the editing flow (↑ loads your newest message, ↓ returns to the draft, Edit plus Escape restores a half-written reply) and the size setting. Not verified live: editing against a server from the UI, Large previews with real images, and preview loading while scrolling a long history.

M4 / M4b — keyboard shortcuts technical contracts (2026-11-12):
- `crates/app/src/shortcuts.rs` is the single binding registry (`SHORTCUTS`): `Shortcut { area, label, keys, action, context }`, `Area::{Composer, Navigation, Window, Viewer}`, `KeyChord`, `ShortcutAction`. `bindings()` now supplies all application/viewer bindings to `main.rs`; the old viewer binding function is removed. `list() -> Vec<ShortcutEntry>` returns plain `{ area, label, keys: Vec<String>, display_only, context }` data for both shortcut views. `Platform::{MacOS, Linux, Windows}`, `KeyChord::{resolve, parts}`, `bindings_for` and `list_for` resolve without host cfg dependence. No Calls area until M5's shortcut work.
- Plain ⌘I / Ctrl+I no longer toggles Room Details; ⌥⌘I / Ctrl+Alt+I remains. `FormatBold` (⌘B), `FormatItalic` (⌘I), `FormatCode` (⌘E), `FormatQuote` (⌘⇧>), `FormatLink` (⌘⇧L), `HistoryPrevious` (Ctrl+Alt+↑) and `HistoryNext` (Ctrl+Alt+↓) are registered in **`Composer`** context (Cmd becomes Ctrl elsewhere). `ShowKeyboardShortcuts` (⌘/ / Ctrl+/) is a global action contract, awaiting Claude's Help menu/sheet handlers. Existing navigation uses `Twayblade`, viewer uses `ImageViewer`, Settings/Quit remain global.
- Existing field/composer behaviour is explicitly **display-only**, with no added bindings: Return, Shift+Return, Escape, edit ↑/↓, Undo, Redo, composer start/end, Select All, Copy/Cut/Paste. Verified against locally resolved Kit/base 0.9.1 source: macOS redo is ⌘⇧Z, Linux/Windows redo is **Ctrl+Y** (not Ctrl+Shift+Z); start/end are ⌘↑/⌘↓ versus Ctrl+Home/End.
- `composer_format::{Format, selection, transform}` are pure byte-range contracts: `transform(format, &str, Range<usize>) -> (String, Range<usize>)`. Bold/italic/code wrap and toggle contents or selected markers; multiline code uses fences long enough for embedded backticks and expands to complete lines. Quote toggles prefixes on whole selected lines. Link inserts `[text](url)` and selects `url`; with no selection the caret goes in the empty label. Empty bold/italic/code selections place the caret inside the markers. Malformed ranges clamp/expand to whole extended graphemes (ZWJ/skin-tone emoji and combining marks); Unicode segmentation is pinned. Send/edit continue through core `rich_text::compose` (Ruma Markdown, sanitized HTML, exact plain Markdown fallback).
- **Selection API finding:** Kit's `TextareaState` re-exports base 0.9.1 `InputBaseState<TextareaMode>` with public `value()`, `selected_range()` and `set_selected_range()` in **UTF-8 bytes** and public undoable `replace_all()`. `composer_format::apply(format, &mut TextareaState, window, cx)` uses these directly, then selects the transform's returned range. No fork, private-field access, UTF-16 conversion, or `set_value` undo reset. Offline textarea testing verifies actual emoji selection, Undo and Redo.
- Backend owns `backend::history::ComposerHistory::{push(room, text), previous(room, current_draft), next(room), reset_cursor, reset_cursors, clear}`. It retains at most 50 sent texts per room / 50 rooms in LRU order, with 64 KiB maximum per text/saved draft, memory only; no serialization, body logging, or Debug implementation. Main-room and thread sends record only after SDK send acceptance (queue acceptance, not a claim of server delivery), never edits or incoming events. Send and room changes reset cursors, advancing past newest restores the saved draft, and `State::stop` clears all entries on sign-out/account replacement/session teardown. Epoch checks reject late send completions after teardown.
- Claude's history integration API: `Backend::history_previous(account_instance, room, current_draft)` / `history_next(account_instance, room) -> Result<Option<String>, ServiceError>`, validating the current account and selected room. Main/thread share their room's history. Replace only the focused composer's text when `Some`; `None` means no move. The existing ↑/↓ message-edit navigation remains separate.
- **Claude still owns:** add the `Composer` key context to both composer surfaces and connect formatting/history actions to these helpers, including their editing/draft policy; build Settings › Keyboard and Help › Keyboard Shortcuts from `shortcuts::list()`, key-cap rendering, global `ShowKeyboardShortcuts` handling and menu entry. New actions intentionally have no view handlers yet; no view layout or styling was changed. The only viewer change removes its superseded binding factory.
- Verification: pinned macOS Nix shell `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, and `cargo test --workspace --all-features --locked` all passed (179 passed: app 116, core 55, media 2, platform 6; 25 isolated-service tests ignored). Unit coverage includes registry/list parity and conflict/parse checks for all three platform resolutions; each formatting operation, empty/toggle/multiline/Unicode behaviour and the core Markdown path; history bounds/LRU, draft restoration, room isolation, session clearing and stale completions. Physical keyboard layouts, focus/IME, Claude's new action integration, live sends/history, and Linux/Windows builds/runtime remain open; no milestone exit claimed.

Room loading state (2026-11-12, Claude): switching rooms no longer shows "Loading conversation…" in the middle of the timeline. While a room loads, a small spinner (a static loader icon with reduced motion) sits after the room name and the timeline stays empty. If the backend reports a timed-out open (`Retrying`), a failure or an unavailable room, or the window has waited 15 s, the empty timeline shows an error layout (icon, title, explanation, Try Again). Try Again drops any selection still waiting in the window's queue and selects the room again, which also recovers from the intermittent stuck selection seen on 2026-11-12. `Snapshot.timeline_phase` carries the main timeline's `TimelineStatus` beside its text. Verified: Clippy, app tests (a new test drives the failed layout and clicks Try Again). Not captured visually against a real failure.

M4b — keyboard shortcuts UI (2026-11-12):
- Settings › Keyboard lists every shortcut from `shortcuts::list()` in four groups (Message box, Navigation, Window, Image viewer), one row per action with alternative keys joined by "or", drawn as key caps with modifiers in platform order (⌃⌥⇧⌘; Ctrl, Alt, Shift). Help › Keyboard Shortcuts and ⌘/ (Ctrl+/) open it, from the main window or Settings.
- Both composers carry the `Composer` key context: ⌘B, ⌘I, ⌘E, ⌘> (Ctrl+Shift+. elsewhere) and ⇧⌘L format the selection through `composer_format::apply`, which is undoable. ⌃⌥↑/⌃⌥↓ (Alt+↑/↓ on Linux and Windows) step through the room's sent messages, replacing the text undoably; ignored while editing a message.
- Review fixes to the technical branch: the quote binding `cmd-shift->` could never match (GPUI matches the typed `>` without shift), now `cmd->` / `ctrl-shift-.`; Ctrl+Alt+↑/↓ is taken by Kit's extra-cursor binding on Windows and reserved by Linux desktops, so history uses Alt+↑/↓ there; "Cancel reply / edit" became "Cancel edit" because Escape does not cancel a reply. Removed the placeholder `dead_code` allowances.
- Checks: fmt, strict Clippy and workspace tests, including offline UI tests that ⌘B and ⌘I reach the main and thread composers through the real bindings (and ⌘I no longer opens room details) and that the Keyboard Shortcuts action opens Settings at Keyboard. Not verified: sent-message history against a server, non-US keyboard layouts for ⌘>, Linux and Windows.

Room-open stop-gap (2026-11-12, Claude):
- Rooms could stay on "Loading conversation…" forever with sync still running. A process sample showed no deadlock: an await before the first timeline snapshot never completed. Each open attempt (timeline build plus subscribe) now has 15 s; on timeout the room shows `TimelineStatus::Retrying` ("Still loading. Check your connection; trying again…") and retries after 1 s, doubling to 30 s. While retrying, selecting the room starts a fresh load. Root cause not established; tracked as fix/room-open-hang.

Room-open investigation and recoverable SDK failures (2026-11-12, Codex):
- Based on merged `main` `8ea5e7c` (PR #13), branch `fix/room-open-hang`. No views changed; backend retry text now says "Messages are taking longer to load. Trying again…", since a store wait need not be a connection problem.
- **Reproduced versus inferred:** a loopback TLS fault proxy over the real isolated Synapse makes `TimelineBuilder::build()` remain pending when an unknown-encryption room's state fetch receives repeated HTTP 503s. The default-policy control is still pending at 3 seconds; the fixed opener promptly yields `Retrying` and reaches Ready after the fault is removed, without reselection. A continuously stalled response returns retry status when the 15-second open budget expires; a dropped response also recovers. This demonstrates a concrete hidden SDK network-retry path before the first snapshot. It does **not** prove that the original real-account Mac occurrence used this path: no trace from that occurrence identifies its exact await, and no permanent async SDK deadlock was reproduced.
- **SDK 0.19.1 source audit:** `RequestConfig::default()` has a **30-second per-attempt timeout**, no retry-count override; native HTTP does **not retry network failures** without an explicit retry limit. Transient 5xx/429 responses do retry, with a 15-minute backoff budget and server rate-limit delays that can exceed an attempt's timeout. This is a long hidden wait, not an infinite default network retry. In this version `max_retry_time` configures maximum backoff delay (`with_max_delay`), not an overall deadline; it is not used as the fix. Source paths: `matrix-sdk/src/config/request.rs`, `http_client/native.rs`, `error.rs`, `room/mod.rs`, `matrix-sdk-ui/src/timeline/builder.rs` (all locally resolved/pinned 0.19.1).
- **Fix at the request/error boundary:** common authenticated-client construction (password/restore/OAuth/SSO) now uses `request_policy::interactive()`: a single HTTP attempt with the SDK's default 30-second timeout (Claude's review, 2026-11-12: Codex's 10-second timeout was dropped because joins over federation, history pagination and search routinely exceed it; room opening is bounded by its own 15-second budget). This returns retry decisions to the existing opening loop. SDK operations with explicit configurations retain theirs (e.g. sliding-sync long polling, member fetching); other default SDK requests inherit this policy. Unknown encryption state is resolved through public SDK `latest_encryption_state()` before building, so the builder's `.ok().unwrap_or_default()` cannot suppress a fetch failure and assume an unencrypted timeline. No manually managed Matrix state, cache deletion, secret reuse, or SDK fork.
- The public SDK event-cache dependency is prepared under its own span before construction. Cross-process event-cache lease contention was reproduced with a second holder over the **same isolated encrypted SQLite store**: SDK acquisition fails in bounded time; the old opener treated that as terminal. Locking errors now yield Retry and recover after lease release. Other cache/storage corruption errors remain terminal; there is no blind purge. Transient HTTP/network failures retry, permanent HTTP failures remain Failed, and parsed Matrix rate-limit retry-after hints can extend the normal delay. Tests verify no request is made before that hint. A held crypto-store lease did not prevent a cached room opening; expired lease reacquisition worked. The crypto heartbeat alone therefore cannot establish a timeline lock blockage.
- **Stop-gap kept:** `OPEN_TIMEOUT = 15 s` still bounds each complete preparation/build/subscribe attempt; base retry delays remain 1, 2, 4, 8, 16, 30 seconds, capped at 30 (longer server retry-after is respected). It also protects awaits outside HTTP. Stream/selection cancellation drops the SDK futures and guards; isolated cancellation during the state request, switching to another room, and reopening the interrupted room all pass without resetting caches. No detached build tasks or unbounded queue added. Generation/account snapshot rejection is preserved. SDK futures are boxed at the preparation call boundaries: the first services run caught a stack overflow in the unoptimized encrypted-restore test after adding the nested stages; boxing fixed it and all six services tests pass at the normal stack size, without increasing `RUST_MIN_STACK`.
- **Persistent opt-in diagnostics:** `TWAYBLADE_LOG=1` enables our debug plus SDK/UI/base/store info tracing. A filter may be supplied instead; unset means no subscriber or log writes. Platform log directory, 4 × 2 MiB rotation (8 MiB total), single-writer OS lock and Unix 0700/0600 permissions; rotation closes files before renaming. Formatter allowlists numeric/boolean fields and static metadata, dropping all SDK free text, string/Debug/error/unknown fields even at trace. This prevents bodies, credentials, recovery material and callback URLs embedded in SDK warning/error payloads from being formatted. Tests include `body`, `formatted_body`, `token`, `password`, callback/response/message fields and a Debug value that panics if formatted. `DEVELOPMENT.md` documents filters, locations, limits and how to interpret pending spans.
- `room_open` spans include generation/attempt and elapsed/cancelled fields, with stages `event_cache_for_room`, `encryption_state_fetch`, `timeline_build`, `timeline_subscribe`, then `first_snapshot` or `room_open_retry` (timeout flag and retry milliseconds). SDK child `read`/`write`/`spin_lock`/SQLite spans expose pending lock acquisition and completion; raw request URLs, event data and room/user IDs are not recorded. The startup call in `main.rs` is the only frontend wiring.
- Isolated matrix additionally covers opening a newly created SDK room before **any** first sync response; opening while a proxy stalls sync; stopping/restarting the real SDK SyncService; rapid cancellation and reopening; contended event-cache leases and held/released crypto leases. A fresh generated fixture identity is used for each runner invocation, with loopback URL checks, trusted fixture TLS, isolated stores and payload-free proxy counters. `tests/integration/run.sh room-open` runs the four bounded cases sequentially. These are headless SDK tests; the cold-created-room case is not physical app restoration acceptance.
- Verification: pinned macOS Nix shell `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo test --workspace --all-features --locked` passed (186 passed: app 118, core 58, media 2, platform 8; 29 opt-in tests ignored). The final `run.sh room-open` passed 4/4 (23.65 s), `run.sh services` 6/6 (including encrypted same-device store restore at normal stack size), and `run.sh viewer` 1/1 (plain/encrypted media, 512/1280 previews, animation, corrupt and >8k limits). A negative control restoring only the SDK default request policy failed the new integration test at its 3-second prompt-retry assertion; the source policy was restored afterward. Formatter/privacy/rotation/one-writer, HTTP classification, retry-after/backoff and timeout-budget unit checks passed; shell/Python syntax checks passed. Remaining acceptance: reproduce/trace the original real-account occurrence with these diagnostics; physical macOS room opening under real-world network/large restored stores; Linux/Windows builds, file rotation and runtime. No milestone exit claimed.

Not yet verified:
- Physical GUI checks of logout, pagination, IME and VoiceOver.
- CI on any host; Linux and Windows builds and runs.
- Public-server Element interoperability; IME, VoiceOver, keyboard focus; performance numbers.

Evidence log: `docs/compatibility/STATUS.md`. Architecture: `docs/architecture/GPUI.md`.

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
