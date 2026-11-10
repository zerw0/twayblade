# macOS-first desktop interface

Decision: 2026-11-10, requested by the user. macOS is the primary UX platform;
Linux and Windows remain supported targets. Retain Iced provisionally and keep
protocol/account state separate from platform presentation.

## Reference and target composition

The user's 2026-11-10 Cinny screenshot is the information-layout reference:
space/home rail, compact room list, room header, conversation, composer and an
optional member inspector. Adapt that structure to a rounded macOS visual style;
do not reproduce Cinny's browser chrome or imply a web-based chat UI.

- Space rail: Home/All, DMs, joined spaces and a compact add/join affordance.
- Sidebar: selected space name, search, rooms/DM sections, subtle unread badges,
  compact row spacing, rounded selection and distinct room identities.
- Header: room avatar/name and real search, members, details and call actions as
  those features become functional. Avoid decorative inactive toolbars.
- Timeline: real display names with Matrix identity available, compact avatars,
  timestamps, consecutive-message grouping, date/unread separators, reply context,
  attachment previews, reactions and visible delivery/decryption states.
- Inspector: optional members/details/thread panel with independent scrolling;
  close it to maximize the conversation area.
- Composer: rounded field, attachments, formatting, emoji/sticker pickers and
  send action. Add each control with its working behavior and keyboard support.
- macOS treatment: proportional bold/semibold headings, restrained hierarchy,
  soft separators, 6–12 point corner radii and system appearance. Keep the main
  conversation surface quiet and the navigation visually distinct.

Use semantic color roles (window, sidebar, field, label, secondary label,
separator, accent and selection). The current palette approximates Mac colors
and tracks system light/dark mode; it does **not** read native semantic colors or
the user's accent setting yet. Add a macOS platform adapter for actual system
colors, high-contrast/reduced-transparency preferences and live appearance/accent
changes, with portable defaults. Do not promise vibrancy without implementation.

Acceptance: compare light/dark and high-contrast appearances on macOS, inspect
Retina rendering, long room/display names, populated timelines and attachments,
and verify keyboard/VoiceOver order. Screenshots with synthetic data must be
marked as offline previews. Preserve Linux/Windows native operation.

## Implemented in source

- Standard decorated, centered 640 × 700 login window; minimum 480 × 560.
- Narrow 360-pixel form, scrollable for smaller windows.
- Compact labels, 14-pixel inputs/actions, restrained rounded borders, blue accent.
- System light/dark theme query on launch and subscription to appearance changes.
- Server selection first, account form second, with a Change server action.
- Masked password field, Return-to-continue/sign-in, disabled controls during work.
- Plain product language rather than raw Matrix login flow identifiers.
- Shared appearance module that can evolve with the eventual conversation UI.
- Explicit macOS San Francisco family with bold/semibold headings and portable fallback.
- Signed-in 1120 × 760 split view with sidebar/search, unread counts and room selection.
- Unified SDK room/encryption synchronization with reconnect and connection indicators; live acceptance pending.
- Tab/Shift-Tab navigation, Command-F search and Command-Q handling.
- Credential-free previews and app-window-only capture for visual QA.

The native workspace preview was launched and inspected on macOS arm64. Build,
Clippy and the current regression tests pass. An observed monospaced bold fallback was
corrected. Native menus, live room loading, physical keyboard and accessibility
remain separate acceptance steps.

These are custom-rendered Iced controls. They do not establish AppKit control
integration, system accent matching, vibrancy, accessible contrast or screen-reader
coverage. Check native input behavior and appearance on real hardware.

## Next platform work

1. Validate light/dark transition, Retina rendering, resizing, text selection,
   keyboard focus/Tab order, VoiceOver and password input behavior.
2. Add application/Edit/Window menus and standard Command shortcuts through a
   macOS platform adapter; retain familiar equivalents on other OSes.
3. Define close/hide/reopen and background-call behavior, then implement it.
4. Build a split-view conversation window: sidebar navigation, room list,
   conversation and optional thread inspector. No fabricated conversations.
5. Add native preferences, notification activation, browser callback handling,
   camera/microphone entitlements and signed application identity.

If the Iced feasibility gate cannot meet required text/accessibility/platform
integration quality, compare Slint and a thin SwiftUI macOS frontend sharing the
Rust core before expanding the UI. A SwiftUI frontend would require a separate
portable frontend strategy and is not the current implementation.

## Joined-space navigation

A 64-point rail now sits before the conversation sidebar. It provides All,
joined-space initials, rounded selection, unread totals, hover names and a nested
entry marker. Selecting a space changes the sidebar title and filters by room ID.
The offline populated preview starts in a sample parent space and excludes rooms
from the other sample space. This preview is explicitly labeled and cannot send.

The core uses SDK SpaceService hierarchy filters; the UI retains drafts when
navigation hides a selected conversation. Two navigation levels are currently
exposed by the SDK. Discovery and space membership/management controls will be
added with working behavior and separate interoperability acceptance.

## Apple-design guidance and revised Home

Reference skill:
https://github.com/emilkowalski/skills/blob/main/skills/apple-design/SKILL.md

Apply typography hierarchy, restraint, familiar controls and responsive feedback
within Rust/Iced. Motion and native material/accessibility work need separate
implementation; this iteration adds no animation or transparency effects.

The current redesign loads San Francisco from the installed macOS font before
opening the window, uses vector icons and compact room rows, moves account
controls behind a bottom menu, and insets the conversation with rounded surfaces.
Home shows only rooms outside known spaces; it is not a duplicate space room list.
Inspect both Home and selected-space populated previews before reporting QA.

QA: both redesigned offline Home and selected-space captures were inspected.
The loaded macOS font uses semibold emphasis to avoid a renderer fallback seen
with the bold weight. This pass does not claim AppKit controls or native vibrancy.

## Additional reference: Apple Messages

The user's 2026-11-10 Messages screenshot is a second design reference alongside
Cinny. Use its circular avatars, conversation previews and timestamps, floating
toolbar controls, rounded composer and incoming/outgoing bubbles as direction.
Do not copy private conversation content, contact identities or media into
fixtures or repository assets.

Consider Messages-style bubbles for DMs. Group rooms still need sender identity,
threads, reactions and dense history. Preserve space navigation and Home's
exclusion of space-owned rooms. Keep conversation content readable, with glass
used for navigation and controls where appropriate.

### Proposed frontend direction — not implemented

For a macOS-first product requiring actual Liquid Glass, prefer a SwiftUI macOS
frontend with focused AppKit integration over continued imitation in Iced.
Retain the Rust core for Matrix authentication, stores, E2EE, sync, timelines,
spaces and account lifecycle. Retain Iced for Linux/Windows.

Evaluate UniFFI for a narrow Swift boundary exposing commands, observable state
and cancellation, not raw SDK clients or account tokens. Prove same-device
session restoration, task ownership and one real timeline through the bridge
before committing to a full frontend migration.

This means Rust application logic with a Swift Mac UI, rather than an exclusively
Rust codebase. It introduces two UI implementations and separate platform QA.
Rust-to-AppKit bindings are another option if an all-Rust frontend is required,
with more custom platform work. This is a proposal, not a completed migration.

Check glass API availability against deployment targets and provide standard
material fallbacks on older supported macOS versions. Native chat chrome does
not establish Element Call media support, which remains a separate spike.

Primary references:

- https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass
- https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views
- https://developer.apple.com/documentation/appkit/nsglasseffectview
- https://mozilla.github.io/uniffi-rs/latest/
