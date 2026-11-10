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
profile inspector. Use system font selection, restrained blue accents, semantic
surfaces and standard window decorations. The product palette adapts to light/dark
appearance but is not native macOS accent-color matching or Liquid Glass.

The user-requested UI/UX Pro Max skill is installed repo-locally at the revision
in AGENTS.md. Its Apple Native Experience and icon-semantics results inform the
review. Its design-system generator returned a marketing-page pattern, which was
not persisted or adopted. GPUI has no preset in that skill: GPUI Kit's actual
component APIs and design/coding guides remain authoritative. Mobile target-size
or web cursor/layout rules are applied only when appropriate to desktop behavior.

Measured token pairs: white on outgoing blue is 4.79:1 in dark mode and 5.59:1 in
light mode; secondary labels against sidebar surfaces are 5.89:1 / 5.13:1. These
checks do not establish full accessibility or composed-state contrast acceptance.

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
