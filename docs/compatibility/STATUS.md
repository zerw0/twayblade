# Compatibility evidence

Updated 2026-11-11. Records what has actually been run, per platform. Compile
checks, offline UI tests, isolated-server tests and live/physical acceptance are
listed separately. Earlier Iced/SwiftUI evidence was removed with those frontends.

| Check | macOS arm64 | Linux | Windows |
| --- | --- | --- | --- |
| fmt / Clippy (`-D warnings`) / unit tests | Pass locally (Rust 1.99; locked/all features; 97 tests) | Not run | Not run |
| CI (`.github/workflows/check.yml`) | Never observed passing | Never observed passing | Never observed passing |
| GPUI build and launch | Pass; ad-hoc `.app` launches | Not run | Not run |
| Offline previews and Kit interaction tests | Pass; light/dark workspace, Home, profile, sign-in, details and 10k captures; Kit interaction/layout tests | Not run | Not run |
| Isolated Synapse: encrypted send/receive, same-device store reopen | Pass | Not run | Not run |
| Live password sign-in, room list, history | Operator reports GPUI sync/messaging working; homeserver not supplied | Not run | Not run |
| Live GPUI send, relaunch restore (Keychain), logout | Operator reports send and Keychain restore; isolated cross-process lifecycle including logout/store deletion passes | Not run | Not run |
| `.well-known` discovery against real servers | Pending | Not run | Not run |
| OAuth/OIDC (Task 1) | SDK discovery/registration/PKCE, callback state, cancellation, atomic Keychain session/refresh; isolated MAS lifecycle passes | Not run | Not run |
| OAuth offline UI (Task 1) | Kit control tests pass; sample waiting screen captured/reviewed light/dark in `captures/oauth-wait-{light,dark}.png` | Not run | Not run |
| OAuth physical system browser / real homeserver (Task 1) | Operator check pending: browser opens, completes callback and relaunch restores on matrix.org or another OAuth server | Not run | Not run |
| Legacy SSO (Task 2) | Provider names/icons, shared loopback/state/cancel, SDK loginToken exchange; full local OIDC flow/restore/logout passes | Not run | Not run |
| Legacy SSO offline UI (Task 2) | Provider controls and callback/icon-limit tests pass; reviewed `captures/sso-{light,dark}.png` samples | Not run | Not run |
| Legacy SSO physical browser / real IdP (Task 2) | Operator check pending: provider button, system browser, callback completion on a real SSO server | Not run | Not run |
| Session recovery (Task 3) | Typed soft/hard revocation, sync shutdown, same-device/store reauth, confirmed hard cleanup; isolated Synapse check passes | Not run | Not run |
| Session recovery offline UI (Task 3) | Confirmation/cancel and soft-retention controls pass; reviewed `captures/{signed-out,soft-logout}-{light,dark}.png` | Not run | Not run |
| Session recovery physical UI/browser (Task 3) | Operator check pending: real revocation prompt, password/browser reauth, hard cleanup confirmation | Not run | Not run |
| Verification/recovery (Task 4) | SDK setup, SAS, backup restore/live history decryption, wrong-key rejection, cancel, recovery reset and other-device sign-out pass on isolated Synapse | Not run | Not run |
| Verification/recovery offline UI (Task 4) | Copy/confirm lifetime, recovery exit and SAS controls pass; reviewed `captures/{verify,recovery}-{light,dark}.png` samples | Not run | Not run |
| Verification/recovery physical/Element (Task 4) | Operator check pending: Element emoji comparison, recovery input/copy, incoming prompt, server browser device management | Not run | Not run |
| Attachments (Task 5) | SDK encrypted/plain queue, progress/cancel, bounded thumbnails, chosen-location Save/Open and upload limits implemented | Not run | Not run |
| Attachment offline UI (Task 5) | Explicit save/open/cancel controls pass; reviewed `captures/attachments-{light,dark}.png`, including sample confirmation and thumbnails | Not run | Not run |
| Attachment physical checks (Task 5) | Pending: native picker, drag/drop, clipboard images and interruption during an active HTTP upload; queued-store restart is automated | Not run | Not run |
| Emoji picker (Task 6) | Search/shortcodes, cursor insertion, shared reactions, keyboard controls, skin tones and bounded persistent recents; Unicode 15.x baseline | Not run | Not run |
| Emoji offline UI (Task 6) | Unicode cursor/Return/shortcode/tone/reaction control tests pass; reviewed `captures/emoji-{light,dark}.png` samples | Not run | Not run |
| Emoji physical checks (Task 6) | Pending: IME, VoiceOver, native keyboard and per-platform emoji fonts; Synapse SDK reaction regression passes | Not run | Not run |
| System accent | Native AppKit read, actual light/dark theme updates and contrast tests pass; light/dark sample captures reviewed; live system-setting change pending | Desktop portal implementation; not run | UISettings implementation; not run |
| Layout fixes (Task 0) | Locked/all-feature fmt, strict Clippy and 55 unit/UI tests pass; measured group gap 38px before / 6px after, reply/content-update and warning-wrap regressions pass | Not run | Not run |
| Layout offline UI (Task 0) | 28px composer/account targets and composer alignment pass; reviewed `captures/layout-fixes-{light,dark}.png` sample warnings and spacing | Not run | Not run |
| Layout physical checks (Task 0) | Pending: real-account spacing, trackpad scrolling, multiline composer and toolbar alignment on macOS | Not run | Not run |
| Threads (M3 Task 1) | SDK thread focus/send/pagination, unread summaries, main reply filtering and scoped receipts implemented; room badges retain SDK server totals | Not run | Not run |
| Thread offline UI (M3 Task 1) | 57 workspace unit/UI tests pass; draft isolation, stale updates, panel switching and real Escape binding; reviewed `captures/thread-{light,dark}.png` samples | Not run | Not run |
| Thread physical/Element checks (M3 Task 1) | Pending: real-account unread/receipt comparison with Element, encrypted thread media, keyboard and trackpad use | Not run | Not run |
| Spaces (M3a Task 2) | SDK directory/pagination, restricted join handling, explicit selective leave, room/DM/space invites, encrypted room creation and Matrix-ID DMs implemented; optional parent choices follow SDK permissions | Not run | Not run |
| Space offline UI (M3a Task 2) | 60 workspace unit/UI tests pass; real context menu, directory access states, leave selection, encryption default and parent permissions; reviewed `captures/{directory,invites}-{light,dark}.png` samples | Not run | Not run |
| Space physical/Element checks (M3a Task 2) | Pending: real-account joins/leaves/invites against Element, keyboard navigation, changing power levels and large cross-server hierarchies | Not run | Not run |
| Image viewer (M3 Task 3) | SDK full-resolution fetch/decryption without media disk caching; bounded GIF/WebP decode, zoom/pan, history navigation, copy and explicit Save/Open; 8192px / 64MiB encoded / 96MiB decoded / 240 frames | Not run | Not run |
| Viewer offline UI (M3 Task 3) | 66 workspace unit/UI tests pass; actual Space/arrows/zoom/Tab/Save/Open/Show-in-Timeline/Escape bindings, focus trap/return, pinch/pan and stale-load cancellation; reviewed `captures/viewer-{light,dark}.png` samples | Not run | Not run |
| Viewer physical/Element checks (M3 Task 3) | Pending: actual trackpad, animation playback, native Save/overwrite/default app, clipboard into another app, VoiceOver and Element images; navigation covers loaded room/current-thread images and requests earlier main history | Not run | Not run |
| Settings foundation (M4 Task 1) | Versioned local JSON, atomic save and invalid-file backup; one window, six sections, Security migration and SDK name/avatar edits; locked fmt/strict Clippy/69 tests pass | Not run | Not run |
| Settings offline UI (M4 Task 1) | Section navigation and draft/window retention tests pass; reviewed `captures/settings-{light,dark}.png` samples | Not run | Not run |
| Settings physical checks (M4 Task 1) | Pending: native Settings menu/shortcut, keyboard/VoiceOver, avatar picker, sign-out and real-account Security controls | Not run | Not run |
| Colour schemes/accent (M4 Task 2) | Seven schemes, curated light/dark variants, persisted system/custom accent, swatches and picker; 12 contrast tests and retained-state control test pass; 82 workspace tests | Not run | Not run |
| Schemes offline UI (M4 Task 2) | `captures/scheme-{system,light,dark,high-contrast,solarized,gruvbox,nord}-{light,dark}.png` samples; 4.5:1 body/muted/accent/link, 7:1 High contrast | Not run | Not run |
| Schemes physical checks (M4 Task 2) | Pending: OS appearance/accent transitions, picker and VoiceOver; no additional server changes | Not run | Not run |
| Scaling/fonts (M4 Task 3) | Shared logical sizes, 85–150% steps, View actions, installed-family selectors, independent message size and missing-font fallback; locked fmt/strict Clippy/83 tests pass | Not run | Not run |
| Scaling offline UI (M4 Task 3) | Actual sidebar height scales; composer remains reachable; `captures/scale-{85,150}-{light,dark}.png` includes toolbar, bubbles and details | Not run | Not run |
| Scaling physical checks (M4 Task 3) | Pending: shortcuts/image-zoom routing, fonts/IME, live anchors on long histories; no server changes | Not run | Not run |
| Chat styles (M4 Task 4) | Bubbles/IRC/Minimal in timelines/threads; density, avatars, timestamps and SDK edit state; locked fmt/strict Clippy/85 tests pass | Not run | Not run |
| Styles offline UI (M4 Task 4) | Control-driven visible-ID/room/draft/anchor/thread/action retention; deleted/edited states; `captures/style-{bubbles,irc,minimal}-{light,dark}.png` plus attachment/thread samples | Not run | Not run |
| Styles physical checks (M4 Task 4) | Pending: keyboard/VoiceOver, long histories, encrypted media and thread interactions | Not run | Not run |
| Thread unread cache (Notifications Task 1) | Immediate cached/zero snapshots; four concurrent queries; own receipts/mark-read invalidation and stale-result guards; locked fmt/strict Clippy/91 workspace tests pass | Not run | Not run |
| Thread cache offline UI (Notifications Task 1) | Existing thread controls pass; reviewed `captures/notifications-thread-{light,dark}.png` sample captures | Not run | Not run |
| Thread cache physical checks (Notifications Task 1) | Pending: real-account counts versus Element and long-history scrolling | Not run | Not run |
| Notification rules (Notifications Task 2) | SDK defaults/keywords/room modes and cross-client subscription; real unread/mute exclusion, SDK count fallback and sync-safe retries; stage fmt/strict Clippy/92 tests pass (final suite: 97) | Not run | Not run |
| Rules offline UI (Notifications Task 2) | Settings/keywords/details/context-menu controls pass; reviewed release `captures/notification-{rules,room}-{light,dark}.png` samples | Not run | Not run |
| Rules physical checks (Notifications Task 2) | Pending: Element rules comparison, real room details/menu, keyboard/VoiceOver | Not run | Not run |
| Notification delivery (Notifications Task 3) | SDK NotificationClient decryption, bounded intake/responses, own/mute/focus suppression; locked fmt/strict Clippy/97 workspace tests pass | D-Bus source typechecked on macOS; Linux build/runtime not run | Toast adapter source; not compiled/run |
| Delivery offline UI (Notifications Task 3) | Device switches/persistence, private preview, stale-account guards and thread click pass; reviewed release `captures/notification-{delivery,private,device}-{light,dark}.png` samples | Not run | Not run |
| Delivery physical checks (Notifications Task 3) | Pending: release .app permission grant/deny, System Settings, real room/thread click, sound, previews, Focus and Dock badge | Pending: daemon/actions, desktop badge support and permissions | Pending: installed app identity, toast activation and taskbar badge |
| Notification activation after process exit (Notifications Task 3) | Pending release work; callbacks currently belong to the running account/process | Pending | Pending |
| Portal accent worker (Notifications Task 4) | Locked fmt/strict Clippy/97 tests pass; light/dark release workspace samples reviewed in `captures/notification-accent-{light,dark}.png` | Persistent portal/runtime, signal cache and same-connection fallback; source typechecked on macOS; Linux build/runtime not run | Unchanged; not run |
| Accent physical checks (Notifications Task 4) | System appearance/accent transition check pending | Pending: portal changes, missing setting, connection count and bus/portal restart | Not run |
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
- System-accent foreground selection is tested at 4.5:1 or better; composed-state accessibility remains pending.
- GPUI controls are custom-rendered. No AppKit widgets,
  vibrancy or complete accessibility is claimed.

## Current limitations

- Native sliding sync is required; servers without it are unsupported.
- One account; one running instance per OS user.
- Logout deletes the local store after revoking the device. If remote revocation
  fails, the session and store are kept so logout can be retried.
- OAuth refresh, verification and recovery are implemented; Element interoperability and physical acceptance remain pending.

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
| `oauth_cancel_and_wrong_state_discard_checkpoint` (Task 1) | MAS 1.27.0 / Synapse 1.164.2: cancel and wrong state reject authorization, delete scoped Keychain checkpoint/store, close listener; pass |
| `oauth_keychain_lifecycle_across_processes` (Task 1) | MAS 1.27.0 / Synapse 1.164.2: generated account, dynamic client ID, S256 PKCE, token exchange, same-device relaunch, automatic expired-token refresh, synchronous Keychain rotation, third-process refresh and logout; pass |
| `sso_local_oidc_token_exchange_and_restore` (Task 2) | Synapse 1.164.2 with MAS 1.27.0 as local OIDC provider: provider discovery, browser redirects/consent, one-use loginToken exchange, scoped Keychain restore and logout; pass. Internal Docker HTTP metadata validation relaxed only in fixture; browser endpoints use TLS |
| `revoked_device_and_soft_logout_recovery` (Task 3) | Synapse 1.164.2 (normal and five-second-token fixture): second-client revocation, hard/soft status, clean sync stop, local data retained, invalid-password retry, same-device/store reauth and explicit hard cleanup; pass |
| `secure_backup_fresh_store_recovery_and_sas` (Task 4) | Synapse 1.164.2, fresh generated account: cross-signing/backup, missing-key timeline, wrong key, fresh-store backup recovery with live re-decryption, SAS between SDK clients, cancelled verification, other-device revocation, reset rejects old key and accepts new key; pass |
| `attachment_plain_encrypted_cancel_restart_round_trip` (Task 5) | Synapse 1.164.2: files and image thumbnails in plain/encrypted rooms, encrypted media source checked, server downloads bypass cache, oversize sparse file rejected, queued cancellation and store-reopen upload/download pass |
| `encrypted_delivery_profile_and_same_device_store_restore`, `timeline_metadata_and_send_queue_actions`, `members_typing_and_read_receipts`, `pagination_preserves_message_identity`, `room_list_service_tracks_recency_and_details` (Task 0 regression) | Synapse 1.164.2, `matrixclient.test`: all five existing service checks pass; generated accounts only |
| `threads_send_paginate_receipts_and_closed_unread` (M3 Task 1) | Synapse 1.164.2, `matrixclient.test`: SDK-created thread, two-way sending, 68 replies, pagination/unique IDs, typed/scoped receipts, root unread summary, hidden inline replies and unread preservation after reading main; pass |
| Existing five service checks (M3 Task 1 regression) | Synapse 1.164.2, `matrixclient.test`: all pass with threading enabled and independent initial pagination; fixture runs serialized |
| `spaces_hierarchy_invites_creation_restricted_join_and_selective_leave` (M3a Task 2) | Synapse 1.164.2, `matrixclient.test`, fresh generated pair: nested/private spaces, 55 accessible children across actual hierarchy pages, inaccessible child, suggested restricted join, room/space accepts and DM decline, encrypted creation, denied parent permissions, multi-parent navigation and selected-space-only leave; pass |
| Existing five service checks and thread acceptance (M3a Task 2 regression) | Synapse 1.164.2, `matrixclient.test`: pass; member fixture waits for requested SDK permission state before updating powers |
| `image_viewer_plain_encrypted_animation_limits_and_corrupt` (M3 Task 3) | Synapse 1.164.2, `matrixclient.test`: plain/encrypted PNG, animated GIF/WebP, >8k image and corrupt file; SDK decryption, no cached decrypted media, explicit original-byte save and cancellation; pass |
| `attachment_plain_encrypted_cancel_restart_round_trip` (M3 Task 3 regression) | Synapse 1.164.2, `matrixclient.test`: attachment round trips, cancellation and persisted queue restart after shared atomic-save/cache changes; pass |
| `timeline_metadata_and_send_queue_actions` (Task 6 regression) | Synapse 1.164.2: existing SDK Unicode reaction add/remove and stable timeline identity pass after shared picker integration |
| `encrypted_delivery_profile_and_same_device_store_restore` (M4 Task 1) | Synapse 1.164.2, `matrixclient.test`, loopback TLS: core SDK display-name update, bounded avatar upload/removal, profile and encrypted delivery/store reopen pass |
| `timeline_metadata_and_send_queue_actions` (M4 Task 4) | Synapse 1.164.2, `matrixclient.test`: existing SDK metadata and send-queue regression passes after edit-state projection |
| `threads_send_paginate_receipts_and_closed_unread` (Notifications Task 1) | Synapse 1.164.2, `matrixclient.test`: SDK send/pagination/scoped receipts, main filtering and closed-thread unread pass with deferred unread snapshots |
| `notification_rules_round_trip_and_second_client_changes` (Notifications Task 2) | Synapse 1.164.2, `matrixclient.test`: four defaults round-trip, keyword add/remove, room modes/reset, actual Bob-message unread then mute exclusion and second-client observation pass |
| `encrypted_notification_delivery_and_private_preview` (Notifications Task 3) | Synapse 1.164.2, `matrixclient.test`: second-client encrypted event stored as ciphertext; SDK resolver emits decrypted typed room/sender/body, preview-off removes sender/avatar/body; pass |

## Operator reports (not independently reproduced)

- GPUI sign-in, sync and sending work on the operator's account; homeserver not
  recorded. Relaunch restored the session after a Keychain approval prompt.
- Ad-hoc signed development builds change their code hash on every build, so
  macOS may ask for Keychain access again. Stable signing is release work.

## Remaining gates

- CI deferred by operator decision; no CI gate applied to this feature work.
- Physical checks: logout from the GUI, trackpad pagination, IME, VoiceOver.
- Element interoperability, verification, key backup/recovery, attachments,
  thread view, large-account performance.
