# Shared GPUI frontend

GPUI + GPUI Kit is the only frontend (decided 2026-11-10). It replaced the
SwiftUI-on-Mac / Iced-on-Linux-and-Windows split; those frontends were deleted.

GPUI Kit 0.9.1 is pinned. It supplies the matching GPUI snapshot, controls, semantic
themes, icons, text input, selectable text and virtual message scrolling. Cargo.lock
pins compatible Taffy 0.15.0 and unicode-properties 0.1.3 because Kit's optional test
backend graph constrains those versions. No web/JavaScript shell is used for chat.

## Ownership

- `core`: SDK clients, authentication, encrypted stores, sync, spaces, profiles and
  timelines. This remains independent of GPUI.
- `gpui/backend.rs`: background runtime, account/selection task lifecycle and
  coalesced presentation snapshots on a `watch` channel that the view awaits
  directly (no polling). Holds the single-instance lock. Tokens and SDK objects
  do not enter UI records.
  Core room, space, profile, message and member projections are used directly;
  the frontend maps typed core outcomes into display wording.
  Epoch/selection guards reject stale stream results. Blocking operations run on
  background workers; networking/storage never run in render.
- `gpui/app.rs`: retained view state, commands, inputs, request guards and task
  completion. Profile callbacks check request/account lifetime. Selection commands
  are serialized so rapid navigation cannot reach the backend out of order.
- `gpui/model.rs`: Home filtering and grouping. Room IDs and SDK timeline identities
  are used instead of display names or list positions.
- `gpui/workspace.rs`: Kit-based sign-in, navigation, grouped transcript, composer
  and profile inspector. The Kit Root owns window facilities.
- `gpui/theme.rs`: product semantic colors and typography. UI code reads theme
  roles; source palette literals are confined to the theme definition.

`preferences.rs` owns versioned, serde-defaulted frontend JSON in the OS config directory. Invalid/unknown files are preserved as `.bak`; a failed backup disables writes. File replacement is atomic and background saves reject stale revisions. Settings shares the main entity in one separate window; Security entry is discarded on exit. Semantic themes and one logical-pixel helper apply appearance without replacing drafts or scrollers. Notification rules/delivery remain unimplemented; no local preference simulates a server rule.

Each visited room retains its own Textarea state and subscription. Events capture
the room ID and read that room's input entity, so delayed changes cannot overwrite
another room's draft. Switching rooms preserves buffer/cursor/undo state. A queued
send clears only an unchanged matching room buffer. Account changes drop those
states. This behavior has a control-driven regression test.

Kit's MessageScroller virtualizes variable-height sender groups. Incremental
splices use stable group/message IDs and preserve the reading anchor; existing
rows are remeasured after content updates. The 10,000-message fixture is a runtime
smoke check, not a timing/memory benchmark or proof of all live pagination behavior.
RoomListService adapters project incremental joined-room diffs in SDK recency
order. Home/space membership filtering remains in the frontend model. Top-entry
scroll notifications request SDK history; retained group boundaries preserve
anchors when a prepend would otherwise merge into the existing first group.
Typing uses SDK throttling, and receipts target an explicit event only when its
row intersects the timeline clip in an active window.

Plain message text is escaped before going through the selectable text renderer,
preventing Markdown image syntax from initiating remote image loads.

## Design and platform boundaries

Use the user's Cinny layout and Apple Messages references: space rail, compact
sidebar, quiet header, grouped messages, rounded composer and a dismissible
profile inspector. Use system font selection, the system accent, semantic
surfaces and standard window decorations. The product palette follows light/dark appearance and the OS accent. macOS reads
AppKit controlAccentColor; Windows uses UISettings; Linux uses the desktop portal.
A background read updates primary/selection roles within two seconds; unavailable
accent data uses the fallback in theme.rs. This is not Liquid Glass or AppKit widgets.

The user-requested UI/UX Pro Max skill is installed repo-locally at the revision
in AGENTS.md. Its Apple Native Experience and icon-semantics results inform the
review. Its design-system generator returned a marketing-page pattern, which was
not persisted or adopted. GPUI has no preset in that skill: GPUI Kit's actual
component APIs and design/coding guides remain authoritative. Mobile target-size
or web cursor/layout rules are applied only when appropriate to desktop behavior.

System-accent text uses whichever of black/white has higher contrast (tested at
4.5:1 or better for representative accents). Selection surfaces derive restrained
tints from the same accent. Source palette literals remain in theme.rs; native
platform adapters return only numeric RGB components. Windows/Linux accent reads
and physical live-preference changes have not been exercised.

## Verification and remaining gates

macOS arm64: primary build/run, ad-hoc bundle launch, offline Home/profile/sign-in
captures and the 10,000-message fixture have been exercised. Headless Kit tests
exercise actual controls and Unicode draft persistence. Physical IME/VoiceOver,
keyboard/focus completeness, light/dark transitions, live account flows, Linux /
Windows execution and call-window/media integration remain pending.

A lock file prevents two instances from sharing the credential entry and store.
Release packaging
must remove local Nix runtime dependencies and add the usual signing/notarization
and clean-machine acceptance. Element Call remains a separate restricted runtime.

References: [GPUI Kit](https://github.com/longbridge/gpui-kit),
[GPUI examples](https://gpui.rs/examples/),
[UI/UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill).

Security state and SAS/backup/recovery operations live in core/security.rs. The
frontend receives typed device/verification projections, never SDK handles or
recovery secrets. A generated recovery key travels separately in a Zeroizing
buffer and is discarded after copy/confirmation. Key/passphrase entry is cleared
and its input entity replaced after use. SDK room-key imports drive timeline
re-decryption; verification updates are guarded by flow ID and account epoch.

Media preparation, queue cancellation, encryption and downloads stay in core.
The frontend owns native dialogs and confirmation; four selection-scoped thumbnail
workers keep timeline updates responsive. Decode and render caches are bounded.
Emoji search, cursor ranges and recents are frontend state; reactions use core.

Thread panels retain their own composer and scroller. SDK thread focus owns relation
encoding, pagination and unread state; main timelines hide threaded events. Public
receipts infer Main/thread scope from the SDK timeline; only the main timeline moves
the fully-read marker. Room badges use SDK server totals without adding thread counts
a second time. Initial pagination uses a selection-scoped JoinSet so it progresses
even while a snapshot consumer awaits another page.

Space directories and leave plans wrap SDK services; the UI receives typed room,
access and owner records. Short-lived, reference-counted room subscriptions request
the permission/routing state needed by operations without moving the conversation
selection. Native sliding sync stays partial; readiness checks specific SDK records,
not the legacy fully-synced flag. Creation distinguishes partial parent-link failure
from creation failure so the UI cannot encourage duplicate rooms.

Full-resolution viewer media uses SDK decryption with the media disk cache disabled
(including thumbnails). A cancellable request and single decode worker bound active
work; encoded input, dimensions, total decoded frames and frame count have limits.
GPUI renders composited animation frames. Only an explicit save destination writes
original bytes atomically; Open in Default App first asks for that destination.
Navigation preserves loaded SDK timeline order, includes the active thread and
requests older main-timeline pages when stepping beyond the first known image.
