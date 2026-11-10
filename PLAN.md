# Native Rust Matrix client — plan

Updated 2026-11-11. Status: M0 in progress; no milestone exit claimed.

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
- Direct and group calls via Element Call: ringing, mute, device choice, screen share.
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
menu Matrix Client › Settings…) with a sidebar of sections. Every change
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

**M5 — calls (5–7 weeks).** Start with a macOS call-runtime spike, then the
complete lifecycle and Linux/Windows. Exit: encrypted direct and group calls with
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
   indicator, formatting (see Milestones).
6. **M5 calls.** Element Call runtime, direct and group calls.
7. **M6 release.** Accessibility pass, packaging, signing, updates.

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
- `encrypted_delivery_profile_and_same_device_store_restore` passes on isolated Synapse 1.164.2 (`matrixclient.test`), including core display-name/avatar update/removal. Operator checks: native menus/shortcut, file picker, account sign-out, security focus/VoiceOver. Appearance, Chat and Notifications controls follow in Tasks 2–5.

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
- `timeline_metadata_and_send_queue_actions` passes on isolated Synapse 1.164.2 (`matrixclient.test`). Operator checks: keyboard/VoiceOver, long-history switching, real encrypted attachments and thread interactions. M4 notifications remain unimplemented.

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
- Locked fmt/strict all-feature Clippy and 91 workspace tests pass, including a counting stub, invalidation, focused-root and bound/eviction regressions. The isolated `threads_send_paginate_receipts_and_closed_unread` passes on Synapse 1.164.2 (`matrixclient.test`).
- Offline thread controls pass; reviewed `captures/notifications-thread-{light,dark}.png` sample captures. Physical checks remain: real-account thread counts versus Element and long-history scrolling.

Notification rules — Notifications Task 2 (2026-11-10):
- SDK server defaults for four room classes (All/Mentions only), keywords and Default/All/Mentions/Mute overrides; subscribed cross-client updates, grouped Settings, details controls, room context menu and muted badges. Edits use fresh SDK views, bounded idempotent retries and sync confirmation. Changed room summaries refresh SDK notification counts, preserving higher server totals without adding thread counts.
- Locked fmt/strict all-feature Clippy and 92 workspace tests pass; control-driven Settings/keyword/details/context-menu test passes. Reviewed release `captures/notification-{rules,room}-{light,dark}.png` sample captures.
- `notification_rules_round_trip_and_second_client_changes` passes on isolated Synapse 1.164.2 (`matrixclient.test`), including muted unread exclusion. Physical checks: rules shown in Element, real sidebar/details/menu and keyboard/VoiceOver. Native delivery follows in Task 3.

Notification delivery — Notifications Task 3 (2026-11-11):
- Bounded SDK sync intake and NotificationClient decryption; own/muted/active-room suppression, guarded room/thread clicks and persisted local enable/sound/preview/badge controls. macOS UserNotifications (bundle detection/permission), persistent Linux D-Bus and Windows toast adapters; OS Focus remains authoritative.
- Locked fmt/strict all-feature Clippy and 97 workspace tests pass, including unbundled behavior, skip/privacy rules and device/click controls. `encrypted_notification_delivery_and_private_preview` passes on isolated Synapse 1.164.2 (`matrixclient.test`); reviewed release `captures/notification-{delivery,private,device}-{light,dark}.png` samples.
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
- `Account::room_media` uses SDK `/messages` back-pagination and in-memory SDK decryption, with a message/URL filter for unencrypted rooms and unfiltered encrypted envelopes; failed filtered requests retry without filters. Page limits are 1–100, with at most four requests per page; empty sparse pages retain a continuation. Cursors bind room and kind. Opaque sources are not serializable or Debug; media fetches disable the SDK disk cache.
- UI contract: `Snapshot.media.{room,kind,items,loading,exhausted,error}`; `MatrixApp::load_media(kind)`, `load_more_media()` and `open_media_image(event_id)` (Window/Context arguments as usual). Items and viewer navigation retain newest-first pagination order. The backend retains at most 2,000 items, deduplicates IDs, cancels selection/account work and checks account instance, selection and request before publishing. No new views or controls; UI integration entry points are explicitly marked as pending consumers.
- Compilation: strict locked workspace/all-target/all-feature Clippy passed. Unit/offline checks: locked all-feature workspace tests passed, including page bounds, encrypted-source projection and cancelled/stale gallery requests; rustfmt check passed after formatting.
- Live integration: isolated Synapse 1.164.2 viewer/gallery check passed for plain/encrypted images, two-item pages through exhaustion, stable IDs/order, SDK in-memory decryption, absent decrypted media cache, animation and corrupt/oversize failures. No Element interoperability or physical platform acceptance claimed.
- Remaining: gallery views, physical viewer/gallery navigation, Element attachments, Linux/Windows builds/runtime. The 2,000-item session cap currently reports exhausted; UI may restart the gallery to reset it.

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

Not yet verified:
- Physical GUI checks of logout, pagination, IME and VoiceOver.
- CI on any host; Linux and Windows builds and runs.
- Element interoperability; IME, VoiceOver, keyboard focus; performance numbers.

Evidence log: `docs/compatibility/STATUS.md`. Architecture: `docs/architecture/GPUI.md`.

## Workspace

```text
crates/core/        Matrix services, auth, sync, timelines (owns all Matrix state)
crates/platform/    OS credentials, notifications, appearance (later: deep links)
crates/gpui/        GPUI + GPUI Kit frontend
macos/              Development .app bundle helper
tests/integration/  Isolated Synapse fixture
docs/               Architecture and compatibility evidence
```

Add `call-host` (Element Call runtime) when M5 starts, not before.

## References

- https://github.com/matrix-org/matrix-rust-sdk
- https://github.com/longbridge/gpui-kit
- https://github.com/element-hq/element-call
- https://github.com/tauri-apps/wry
